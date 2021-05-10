pub mod downloader;
pub mod ui;

use std::fs::File;
use std::io::prelude::*;

pub struct ServerObject {
    json_obj: json::JsonValue,
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
                Self::download_file();
                File::open(file_path)
            })
            .expect("didn't find the file, tried to download, but even that might have failed");
        let mut json_data = String::new();
        file.read_to_string(&mut json_data).unwrap();

        let json_obj = json::parse(&json_data).unwrap();
        Self { json_obj }
    }

    pub fn download_file() {
        let file_path = "network_datagram_config.json";
        downloader::Download::from_url("https://raw.githubusercontent.com/SteamDatabase/SteamTracking/master/Random/NetworkDatagramConfig.json", file_path).unwrap();
    }

    pub fn get_server_ips(&self, server_abr: &str) -> Result<Vec<&str>, ()> {
        let obj = &self.json_obj;

        let obj = &obj["pops"];

        let server = &obj[server_abr];

        let mut ips = Vec::new();

        if let json::JsonValue::Array(relays) = &server["relays"] {
            for relay in relays {
                if let json::JsonValue::Short(ip) = &relay["ipv4"] {
                    ips.push(ip.as_str());
                } else {
                    panic!("couldn't find ip within relays, got {:?}", relay["ipv4"]);
                }
            }
        } else {
            return Err(());
        }

        return Ok(ips);
    }

    pub fn get_server_list(&self) -> Vec<&str> {
        let obj = &self.json_obj;
        let obj = &obj["pops"];

        let mut names = Vec::new();
        if let json::JsonValue::Object(servers) = &obj {
            for (server, _) in servers.iter() {
                names.push(server);
            }
        } else {
            panic!("couldn't find array of servers in pops, got {:?}", obj);
        }

        return names;
    }

    pub fn get_server_state(
        &self,
        ipt: &iptables::IPTables,
        server_abr: &str,
    ) -> Result<ServerState, ()> {
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

    fn ban_ip(&self, ipt: &iptables::IPTables, ip: &str) -> Result<(), ()> {
        let rule = format!("-s {} -j DROP", ip);
        ipt.append_replace("filter", "INPUT", &rule)
            .or_else(|_| return Err(()))?;
        return Ok(());
    }

    fn unban_ip(&self, ipt: &iptables::IPTables, ip: &str) -> Result<(), ()> {
        let rule = format!("-s {} -j DROP", ip);
        ipt.delete_all("filter", "INPUT", &rule)
            .or_else(|_| return Err(()))?;
        return Ok(());
    }

    pub fn ban_server(&self, ipt: &iptables::IPTables, server_abr: &str) -> Result<(), ()> {
        let ip_list = self.get_server_ips(server_abr)?;
        for ip in ip_list {
            self.ban_ip(ipt, ip)?;
        }
        return Ok(());
    }

    pub fn unban_server(&self, ipt: &iptables::IPTables, server_abr: &str) -> Result<(), ()> {
        let ip_list = self.get_server_ips(server_abr)?;
        for ip in ip_list {
            self.unban_ip(ipt, ip)?;
        }
        return Ok(());
    }
}
