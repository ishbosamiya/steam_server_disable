use std::net::Ipv4Addr;

use crate::{
    downloader,
    firewall::{self, Firewall},
};

use self::parse::ServerObject;

mod parse {
    use serde::{Deserialize, Serialize};

    use std::collections::HashMap;
    use std::fs::File;
    use std::io::prelude::*;

    use crate::{downloader, file_ops};

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
            let file_path = file_ops::get_network_datagram_config_file_path();
            let mut file = File::open(file_path)
                .or_else(|_| {
                    match Self::download_file() {
                        Ok(_) => {}
                        Err(error) => {
                            panic!(
                        "{} didn't exist, tried to download, check your internet connection? {}",
                        file_path.to_str().unwrap(), error
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
            let file_path = file_ops::get_network_datagram_config_file_path();
            // `NetworkDatagramConfig.json` is no longer available on
            // the master branch, Valve doesn't publish this file
            // anymore, so use the latest version
            downloader::Download::from_url("https://raw.githubusercontent.com/SteamDatabase/SteamTracking/0ae12036fceb607d31a2cecb504f4ffa6f52d306/Random/NetworkDatagramConfig.json", file_path)?;
            Ok(())
        }

        /// Get a reference to the server object's pops.
        pub(crate) fn get_pops(&self) -> &HashMap<String, ServerInfo> {
            &self.pops
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerState {
    AllDisabled,
    /// Some IPs of the server are disabled. IPs that are disabled are
    /// passed along.
    SomeDisabled(Vec<Ipv4Addr>),
    NoneDisabled,
    Unknown,
}

impl std::fmt::Display for ServerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                ServerState::AllDisabled => "All Disabled",
                ServerState::SomeDisabled(_) => "Some Disabled",
                ServerState::NoneDisabled => "None Disabled",
                ServerState::Unknown => "Unknown",
            }
        )
    }
}

#[derive(Debug)]
pub enum Error {
    Downloader(downloader::Error),
    NoServer,
    NoRelay,
    Firewall(firewall::Error),
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

impl From<firewall::Error> for Error {
    fn from(error: firewall::Error) -> Self {
        Error::Firewall(error)
    }
}

impl std::error::Error for Error {}

pub struct ServerInfo {
    abr: String,
    ipv4s: Vec<Ipv4Addr>,
}

impl ServerInfo {
    pub fn ban(&self, firewall: &Firewall) -> Result<(), Error> {
        log::info!("banned {}", self.get_abr());
        Ok(self
            .get_ipv4s()
            .iter()
            .try_for_each(|ip| firewall.ban_ip(*ip))?)
    }

    pub fn unban(&self, firewall: &Firewall) -> Result<(), Error> {
        log::info!("unbanned {}", self.get_abr());
        Ok(self
            .get_ipv4s()
            .iter()
            .try_for_each(|ip| firewall.unban_ip(*ip))?)
    }

    /// Get a reference to the server info's ipv4s.
    pub fn get_ipv4s(&self) -> &[Ipv4Addr] {
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
                    .map(|info| info.get_ipv4().parse().unwrap())
                    .collect();
                Some(ServerInfo {
                    abr: server.to_string(),
                    ipv4s,
                })
            })
            .collect();

        servers.sort_unstable_by_key(|info| info.abr.to_string());

        Servers { servers }
    }
}
