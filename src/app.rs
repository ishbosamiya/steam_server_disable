use std::{
    collections::{HashMap, VecDeque},
    convert::TryInto,
    net::Ipv4Addr,
    sync::{mpsc, Mutex},
    thread,
    time::Duration,
};

use crate::{
    egui,
    ping::{self, PingInfo, Pinger},
    steam_server::{self, Error, ServerObject, ServerState},
};

pub struct Servers {
    servers: Vec<ServerInfo>,
}

impl Servers {
    /// Get a reference to the servers's servers.
    pub fn get_servers(&self) -> &[ServerInfo] {
        self.servers.as_ref()
    }
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
        self.get_ipv4s()
            .iter()
            .try_for_each(|ip| steam_server::ban_ip(ipt, ip))
    }

    pub fn unban(&self, ipt: &iptables::IPTables) -> Result<(), Error> {
        *self.state.lock().unwrap() = None;
        self.get_ipv4s()
            .iter()
            .try_for_each(|ip| steam_server::unban_ip(ipt, ip))
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

pub enum PingerMessage {
    PushToList(Ipv4Addr),
    ClearList,
    KillThread,
}

pub struct App {
    servers: Servers,
    ipt: iptables::IPTables,

    ping_info: HashMap<Ipv4Addr, VecDeque<Result<PingInfo, ping::Error>>>,

    pinger_message_sender: mpsc::Sender<PingerMessage>,
    ping_receiver: mpsc::Receiver<(Ipv4Addr, Result<PingInfo, ping::Error>)>,
    pinger_thread_handle: Option<thread::JoinHandle<()>>,
}

impl Drop for App {
    fn drop(&mut self) {
        self.pinger_message_sender
            .send(PingerMessage::KillThread)
            .unwrap();
        self.pinger_thread_handle.take().unwrap().join().unwrap();
    }
}

impl App {
    pub fn new() -> Self {
        let (pinger_message_sender, pinger_message_receiver) = mpsc::channel::<PingerMessage>();
        let (ping_sender, ping_receiver) =
            mpsc::channel::<(Ipv4Addr, Result<PingInfo, ping::Error>)>();

        let pinger_thread_handle = thread::spawn(move || {
            let pinger_message_receiver = pinger_message_receiver;
            let ping_sender = ping_sender;
            let mut list = Vec::new();
            let mut pinger = Pinger::new();
            pinger.set_timeout(Duration::from_millis(500));
            let mut index = 0;
            loop {
                let messages: Vec<_> = pinger_message_receiver.try_iter().collect();
                if messages
                    .iter()
                    .any(|message| matches!(message, PingerMessage::KillThread))
                {
                    break;
                }
                messages.iter().for_each(|message| match message {
                    PingerMessage::PushToList(ip) => list.push(*ip),
                    PingerMessage::ClearList => list.clear(),
                    PingerMessage::KillThread => unreachable!(),
                });

                if !list.is_empty() {
                    if index > list.len() {
                        index = list.len() - 1;
                    }
                    let ping_data = pinger.ping(list[index], 0);
                    ping_sender.send((list[index], ping_data)).unwrap();
                    index += 1;
                    if index > list.len() {
                        index = 0;
                    }
                } else {
                    thread::sleep(Duration::from_millis(50));
                }
            }
        });

        let res = Self {
            servers: ServerObject::new().into(),
            ipt: iptables::new(false).unwrap(),
            ping_info: HashMap::new(),
            pinger_message_sender,
            ping_receiver,
            pinger_thread_handle: Some(pinger_thread_handle),
        };

        res.servers.get_servers().iter().for_each(|info| {
            info.get_ipv4s().iter().for_each(|ip| {
                res.pinger_message_sender
                    .send(PingerMessage::PushToList(ip.parse::<Ipv4Addr>().unwrap()))
                    .unwrap();
            })
        });

        res
    }

    /// Update all information that must happen very so often. eg:
    /// ping information receiving
    pub fn update(&mut self) {
        let ping_info = &mut self.ping_info;
        self.ping_receiver.try_iter().for_each(|(ip, info)| {
            let ip_info = ping_info.entry(ip).or_insert_with(VecDeque::new);
            ip_info.push_front(info);

            if ip_info.len() > 10 {
                ip_info.truncate(10);
            }
        });
    }

    /// Calculate the ping for the given ip. Returns the rtt, total
    /// number of packets number of packets dropped.
    fn calculate_ping_for_ip(&self, ip: Ipv4Addr) -> (Duration, usize, usize) {
        self.ping_info
            .get(&ip)
            .map(|list| {
                let (total_ping, num_lost_packets) =
                    list.iter()
                        .fold((Duration::ZERO, 0), |acc, info| match info {
                            Ok(info) => (acc.0 + info.get_rtt(), acc.1),
                            Err(_) => (acc.0, acc.1 + 1),
                        });

                let num_valid_packets = (list.len() - num_lost_packets).try_into().unwrap();
                let total_ping = if num_valid_packets == 0 {
                    total_ping
                } else {
                    total_ping / num_valid_packets
                };
                (total_ping, list.len(), num_lost_packets)
            })
            .unwrap_or((Duration::ZERO, 0, 0))
    }

    pub fn draw_ui(&mut self, ui: &mut egui::Ui) {
        if ui.button("Download Server List").clicked() {
            ServerObject::download_file().unwrap();
            self.servers = ServerObject::new().into();
        }

        let num_columns = 6;
        egui::Grid::new("ui_grid")
            .min_col_width(ui.available_width() / num_columns as f32)
            .max_col_width(ui.available_width())
            .num_columns(num_columns)
            .striped(true)
            .show(ui, |ui| {
                ui.columns(num_columns, |columns| {
                    columns[0].label("Region");
                    columns[1].label("State");
                    if columns[2].button("Enable All").clicked() {
                        self.servers.get_servers().iter().for_each(|server| {
                            server.unban(&self.ipt).unwrap();
                        });
                    }
                    if columns[3].button("Disable All").clicked() {
                        self.servers.get_servers().iter().for_each(|server| {
                            server.ban(&self.ipt).unwrap();
                        });
                    }
                    columns[4].label("Ping");
                    columns[5].label("Loss");
                });
                ui.end_row();

                self.servers.get_servers().iter().for_each(|server| {
                    ui.columns(num_columns, |columns| {
                        columns[0].label(server.get_abr());

                        columns[1].label(server.get_cached_server_state(&self.ipt).to_string());

                        if columns[2].button("Enable").clicked() {
                            server.unban(&self.ipt).unwrap();
                        }

                        if columns[3].button("Disable").clicked() {
                            server.ban(&self.ipt).unwrap();
                        }

                        let (total_ping, total_num_packets, lost_packets) = server
                            .get_ipv4s()
                            .iter()
                            .map(|ip| ip.parse::<Ipv4Addr>().unwrap())
                            .fold((Duration::ZERO, 0, 0), |acc, ip| {
                                let (ping, total_num_packets, lost_packets) =
                                    self.calculate_ping_for_ip(ip);
                                (
                                    acc.0 + ping,
                                    acc.1 + total_num_packets,
                                    acc.2 + lost_packets,
                                )
                            });

                        let num_valid_packets =
                            (total_num_packets - lost_packets).try_into().unwrap();
                        let ping = if num_valid_packets == 0 {
                            total_ping
                        } else {
                            total_ping / num_valid_packets
                        };

                        if total_num_packets == lost_packets {
                            columns[4].label("Total Loss");
                            columns[5].label("100.00%");
                        } else {
                            columns[4].label(format!("{}", PingInfo::new(ping)));
                            columns[5].label(format!(
                                "{:.2}%",
                                lost_packets as f64 / total_num_packets as f64 * 100.0
                            ));
                        }
                    });

                    ui.end_row();
                });
            });
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
