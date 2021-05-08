pub mod downloader;

use std::fs::File;
use std::io::prelude::*;

pub struct ServerObject {
    json_obj: json::JsonValue,
}

impl ServerObject {
    pub fn new() -> Self {
        let mut downloader = downloader::Download::from_url("https://raw.githubusercontent.com/SteamDatabase/SteamTracking/master/Random/NetworkDatagramConfig.json");
        let file_path = "network_datagram_config.json";
        downloader.store_to_file(file_path);

        let mut file = File::open(file_path).unwrap();
        let mut json_data = String::new();
        file.read_to_string(&mut json_data).unwrap();

        let json_obj = json::parse(&json_data).unwrap();

        Self { json_obj }
    }

    pub fn get_server_ips(&self, server: &str) -> Vec<&str> {
        let obj = &self.json_obj;

        let obj = &obj["pops"];

        let server = &obj[server];

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
            panic!("couldn't get relays");
        }

        return ips;
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
}
