use std::sync::Mutex;

use crate::downloader;

use self::parse::ServerObject;

mod parse {
    use serde::{Deserialize, Serialize};

    use std::collections::HashMap;
    use std::fs::File;
    use std::io::prelude::*;

    use crate::downloader;

    use super::Error;

    #[derive(Serialize, Deserialize)]
    pub struct ServerObject {
        revision: usize,
        certs: Vec<String>,
        p2p_share_ip: HashMap<String, usize>,
        pops: HashMap<String, ServerInfo>,
        relay_public_key: String,
        revoked_keys: Vec<String>,
    }

    #[derive(Serialize, Deserialize)]
    pub(crate) struct ServerInfo {
        desc: Option<String>,
        geo: Option<Vec<f32>>,
        groups: Vec<String>,
        relays: Option<Vec<RelayInfo>>,
    }

    impl ServerInfo {
        /// Get a reference to the server info's relays.
        pub(crate) fn get_relays(&self) -> Option<&Vec<RelayInfo>> {
            self.relays.as_ref()
        }
    }

    #[derive(Serialize, Deserialize)]
    pub(crate) struct RelayInfo {
        ipv4: String,
        port_range: Vec<usize>,
    }

    impl RelayInfo {
        /// Get a reference to the relay info's ipv4.
        pub(crate) fn get_ipv4(&self) -> &str {
            self.ipv4.as_ref()
        }
    }

    impl Default for ServerObject {
        fn default() -> Self {
            Self::new()
        }
    }

    impl ServerObject {
        pub fn new() -> Self {
            let file_path = "network_datagram_config.json";
            let mut file = File::open(file_path)
                .or_else(|_| {
                    match Self::download_file() {
                        Ok(_) => {}
                        Err(error) => {
                            panic!(
                        "{} didn't exist, tried to download, check your internet connection? {}",
                        file_path, error
                    )
                        }
                    }
                    File::open(file_path)
                })
                .expect("didn't find the file, tried to download, but even that might have failed");
            let mut json_data = String::new();
            file.read_to_string(&mut json_data).unwrap();

            serde_json::from_str(&json_data).expect("network datagram config file json structure might have changed, unable to parse, contact developer")
        }

        pub fn download_file() -> Result<(), Error> {
            let file_path = "network_datagram_config.json";
            downloader::Download::from_url("https://raw.githubusercontent.com/SteamDatabase/SteamTracking/master/Random/NetworkDatagramConfig.json", file_path)?;
            Ok(())
        }

        /// Get a reference to the server object's pops.
        pub(crate) fn get_pops(&self) -> &HashMap<String, ServerInfo> {
            &self.pops
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ServerState {
    AllDisabled,
    SomeDisabled,
    NoneDisabled,
}

impl std::fmt::Display for ServerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                ServerState::AllDisabled => "All Disabled",
                ServerState::SomeDisabled => "Some Disabled",
                ServerState::NoneDisabled => "None Disabled",
            }
        )
    }
}

#[derive(Debug)]
pub enum Error {
    Downloader(downloader::Error),
    IPTables(iptables::error::IptablesError),
    NoServer,
    NoRelay,
    UnsuccessfulBan,
    UnsuccessfulUnban,
    ServerUnreachable,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl From<downloader::Error> for Error {
    fn from(error: downloader::Error) -> Self {
        Error::Downloader(error)
    }
}

impl From<iptables::error::IptablesError> for Error {
    fn from(error: iptables::error::IptablesError) -> Self {
        Error::IPTables(error)
    }
}

impl std::error::Error for Error {}

pub fn ban_ip(ipt: &iptables::IPTables, ip: &str) -> Result<(), Error> {
    let rule = format!("-s {} -j DROP", ip);
    ipt.append_replace("filter", "INPUT", &rule)
        .map_err(|_| Error::UnsuccessfulBan)?;
    Ok(())
}

pub fn unban_ip(ipt: &iptables::IPTables, ip: &str) -> Result<(), Error> {
    let rule = format!("-s {} -j DROP", ip);
    ipt.delete_all("filter", "INPUT", &rule)
        .map_err(|_| Error::UnsuccessfulUnban)?;
    Ok(())
}

pub struct ServerInfo {
    abr: String,
    ipv4s: Vec<String>,

    /// Cached state of the server
    state: Mutex<Option<ServerState>>,
}

impl ServerInfo {
    /// Get cached state of the server, will cache the current state
    /// if state is not cached yet
    pub fn get_cached_server_state(&self, ipt: &iptables::IPTables) -> ServerState {
        let mut state = self.state.lock().unwrap();
        if let Some(state) = &*state {
            *state
        } else {
            let mut all_dropped = true;
            let mut one_exists = false;
            self.get_ipv4s().iter().for_each(|ip| {
                let rule = format!("-s {} -j DROP", ip);
                if let Ok(exists) = ipt.exists("filter", "INPUT", &rule) {
                    if exists {
                        one_exists = true;
                    } else {
                        all_dropped = false;
                    }
                } else {
                    all_dropped = false;
                }
            });
            let server_state = if all_dropped {
                ServerState::AllDisabled
            } else if one_exists {
                ServerState::SomeDisabled
            } else {
                ServerState::NoneDisabled
            };

            *state = Some(server_state);
            server_state
        }
    }

    pub fn ban(&self, ipt: &iptables::IPTables) -> Result<(), Error> {
        *self.state.lock().unwrap() = None;
        self.get_ipv4s().iter().try_for_each(|ip| ban_ip(ipt, ip))
    }

    pub fn unban(&self, ipt: &iptables::IPTables) -> Result<(), Error> {
        *self.state.lock().unwrap() = None;
        self.get_ipv4s().iter().try_for_each(|ip| unban_ip(ipt, ip))
    }

    /// Get a reference to the server info's ipv4s.
    pub fn get_ipv4s(&self) -> &[String] {
        self.ipv4s.as_ref()
    }

    /// Get a reference to the server info's abr.
    pub fn get_abr(&self) -> &str {
        self.abr.as_ref()
    }
}

pub struct Servers {
    servers: Vec<ServerInfo>,
}

impl Servers {
    pub fn new() -> Self {
        ServerObject::new().into()
    }

    pub fn download_file() -> Result<(), Error> {
        ServerObject::download_file()
    }

    /// Get a reference to the servers's servers.
    pub fn get_servers(&self) -> &[ServerInfo] {
        self.servers.as_ref()
    }
}

impl Default for Servers {
    fn default() -> Self {
        Self::new()
    }
}

impl From<ServerObject> for Servers {
    fn from(server_object: ServerObject) -> Self {
        let mut servers: Vec<_> = server_object
            .get_pops()
            .iter()
            .filter_map(|(server, info)| {
                let ipv4s = info
                    .get_relays()?
                    .iter()
                    .map(|info| info.get_ipv4().to_string())
                    .collect();
                Some(ServerInfo {
                    abr: server.to_string(),
                    ipv4s,
                    state: Mutex::new(None),
                })
            })
            .collect();

        servers.sort_unstable_by_key(|info| info.abr.to_string());

        Servers { servers }
    }
}
