use fastping_rs::{PingResult, Pinger};
use serde::{Deserialize, Serialize};

use std::collections::HashMap;
use std::convert::TryInto;
use std::fs::File;
use std::io::prelude::*;

use crate::downloader;

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
struct ServerInfo {
    desc: Option<String>,
    geo: Option<Vec<f32>>,
    groups: Vec<String>,
    relays: Option<Vec<RelayInfo>>,
}

#[derive(Serialize, Deserialize)]
struct RelayInfo {
    ipv4: String,
    port_range: Vec<usize>,
}

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

impl Default for ServerObject {
    fn default() -> Self {
        return Self::new();
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

    pub fn get_server_ping(&self, server_abr: &str) -> Result<std::time::Duration, Error> {
        let (pinger, results) = Pinger::new(None, None).expect("couldn't create pinger");

        let ips = self.get_server_ips(server_abr)?;
        ips.iter().for_each(|ip| pinger.add_ipaddr(ip));
        pinger.run_pinger();

        let total_elapsed = results.iter().take(ips.len()).try_fold(
            std::time::Duration::from_millis(0),
            |elapsed, result| match result {
                PingResult::Idle { addr: _ } => Err(Error::ServerUnreachable),
                PingResult::Receive { addr: _, rtt } => Ok(elapsed + rtt),
            },
        )?;

        return Ok(total_elapsed / ips.len().try_into().unwrap());
    }

    pub fn get_server_ips(&self, server_abr: &str) -> Result<Vec<&String>, Error> {
        let server = self.pops.get(server_abr).ok_or(Error::NoServer)?;
        let relays = server.relays.as_ref().ok_or(Error::NoRelay)?;
        let ips = relays.iter().map(|relay| &relay.ipv4).collect();
        return Ok(ips);
    }

    pub fn get_server_list(&self) -> Vec<&String> {
        let mut list: Vec<&String> = self.pops.keys().collect();
        list.sort();
        return list;
    }

    pub fn get_server_state(
        &self,
        ipt: &iptables::IPTables,
        server_abr: &str,
    ) -> Result<ServerState, Error> {
        let ip_list = self.get_server_ips(server_abr)?;
        let mut all_dropped = true;
        let mut one_exists = false;
        for ip in ip_list {
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
        }
        if all_dropped {
            return Ok(ServerState::AllDisabled);
        }
        if one_exists {
            return Ok(ServerState::SomeDisabled);
        }
        return Ok(ServerState::NoneDisabled);
    }

    fn ban_ip(&self, ipt: &iptables::IPTables, ip: &str) -> Result<(), Error> {
        let rule = format!("-s {} -j DROP", ip);
        ipt.append_replace("filter", "INPUT", &rule)
            .or_else(|_| return Err(Error::UnsuccessfulBan))?;
        return Ok(());
    }

    fn unban_ip(&self, ipt: &iptables::IPTables, ip: &str) -> Result<(), Error> {
        let rule = format!("-s {} -j DROP", ip);
        ipt.delete_all("filter", "INPUT", &rule)
            .or_else(|_| return Err(Error::UnsuccessfulUnban))?;
        return Ok(());
    }

    pub fn ban_server(&self, ipt: &iptables::IPTables, server_abr: &str) -> Result<(), Error> {
        let ip_list = self.get_server_ips(server_abr)?;
        for ip in ip_list {
            self.ban_ip(ipt, ip)?;
        }
        return Ok(());
    }

    pub fn unban_server(&self, ipt: &iptables::IPTables, server_abr: &str) -> Result<(), Error> {
        let ip_list = self.get_server_ips(server_abr)?;
        for ip in ip_list {
            self.unban_ip(ipt, ip)?;
        }
        return Ok(());
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
        return Error::Downloader(error);
    }
}

impl From<iptables::error::IptablesError> for Error {
    fn from(error: iptables::error::IptablesError) -> Self {
        return Error::IPTables(error);
    }
}

impl std::error::Error for Error {}

#[derive(Debug, Clone, Copy)]
pub enum PingInfo {
    Unknown,
    Unreachable,
    Rtt(std::time::Duration),
}

impl std::fmt::Display for PingInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let PingInfo::Rtt(duration) = self {
            write!(f, "{} ms", duration.as_millis())
        } else {
            write!(
                f,
                "{}",
                match self {
                    PingInfo::Unknown => "Unknown Ping",
                    PingInfo::Unreachable => "Server Unreachable",
                    _ => "",
                }
            )
        }
    }
}
