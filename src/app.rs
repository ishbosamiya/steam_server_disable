use std::{
    collections::{HashMap, VecDeque},
    convert::TryInto,
    net::Ipv4Addr,
    sync::mpsc,
    thread,
    time::Duration,
};

use crate::{
    egui,
    firewall::Firewall,
    ping::{self, PingInfo, Pinger},
    steam_server::{ServerState, Servers},
};

pub enum PingerMessage {
    PushToList(Ipv4Addr),
    RemoveFromList(Ipv4Addr),
    AppendToList(Vec<Ipv4Addr>),
    ClearList,
    KillThread,
}

pub struct App {
    servers: Servers,
    firewall: Firewall,

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

                messages.into_iter().for_each(|message| match message {
                    PingerMessage::PushToList(add_ip) => {
                        debug_assert!(
                            !list.iter().any(|ip| *ip == add_ip),
                            "attempting to add duplicate ip to the pinger list"
                        );
                        list.push(add_ip);
                    }
                    PingerMessage::RemoveFromList(remove_ip) => {
                        if let Some(index) = list.iter().enumerate().find_map(|(index, ip)| {
                            if *ip == remove_ip {
                                Some(index)
                            } else {
                                None
                            }
                        }) {
                            list.swap_remove(index);
                        }
                    }
                    PingerMessage::AppendToList(ip_list) => {
                        debug_assert!(
                            !list
                                .iter()
                                .any(|ip| ip_list.iter().any(|add_ip| { add_ip == ip })),
                            "attempting to add duplicate ip to the pinger list"
                        );
                        list.extend(ip_list.into_iter());
                    }
                    PingerMessage::ClearList => list.clear(),
                    PingerMessage::KillThread => unreachable!(),
                });

                if !list.is_empty() {
                    if index >= list.len() {
                        index = 0;
                    }
                    let ping_data = pinger.ping(list[index], 0);
                    if let Err(ping::Error::SendError) = &ping_data {
                        log::error!("Check your internet connection, unable to send packets");
                        thread::sleep(Duration::from_secs(1));
                    }
                    ping_sender.send((list[index], ping_data)).unwrap();
                    index += 1;
                } else {
                    thread::sleep(Duration::from_millis(50));
                }
            }
        });

        let res = Self {
            servers: Servers::new(),
            firewall: Firewall::new(),
            ping_info: HashMap::new(),
            pinger_message_sender,
            ping_receiver,
            pinger_thread_handle: Some(pinger_thread_handle),
        };

        res.send_currently_active_ip_list_to_pinger();

        res
    }

    /// note: it is generally a good idea to clear the list before
    /// sending the complete server ip list to the pinger thread, it
    /// can lead to duplications otherwise
    fn send_currently_active_ip_list_to_pinger(&self) {
        self.servers.get_servers().iter().for_each(|info| {
            match info.get_cached_server_state(&self.firewall) {
                ServerState::SomeDisabled | ServerState::NoneDisabled => {
                    self.pinger_message_sender
                        .send(PingerMessage::AppendToList(info.get_ipv4s().to_vec()))
                        .unwrap();
                }
                _ => { // do nothing }
                }
            }
        });
    }

    /// Update all information that must happen very so often. eg:
    /// ping information receiving
    pub fn update(&mut self) {
        let max_pings_per_ip = 20;

        let ping_info = &mut self.ping_info;
        self.ping_receiver.try_iter().for_each(|(ip, info)| {
            let ip_info = ping_info.entry(ip).or_insert_with(VecDeque::new);
            ip_info.push_front(info);

            if ip_info.len() > max_pings_per_ip {
                ip_info.truncate(max_pings_per_ip);
            }
        });
    }

    /// Calculate the total ping for the given ip. Returns the rtt, total
    /// number of packets number of packets dropped.
    ///
    /// note: this returns the total ping not the average ping of the
    /// packets
    fn calculate_total_ping_for_ip(&self, ip: Ipv4Addr) -> (Duration, usize, usize) {
        self.ping_info
            .get(&ip)
            .map(|list| {
                let (total_ping, num_lost_packets) =
                    list.iter()
                        .fold((Duration::ZERO, 0), |acc, info| match info {
                            Ok(info) => (acc.0 + info.get_rtt(), acc.1),
                            Err(_) => (acc.0, acc.1 + 1),
                        });

                (total_ping, list.len(), num_lost_packets)
            })
            .unwrap_or((Duration::ZERO, 0, 0))
    }

    pub fn draw_ui(&mut self, ui: &mut egui::Ui) {
        if ui.button("Download Server List").clicked() {
            let download_file_res = Servers::download_file();
            if let Err(err) = download_file_res {
                log::error!("{}", err);
            }
            self.servers = Servers::new();
        }

        // debug ping info
        if false {
            egui::Grid::new("debug_ping_info_grid")
                .striped(true)
                .min_col_width(ui.available_width() / 2.0)
                .max_col_width(ui.available_width())
                .show(ui, |ui| {
                    self.ping_info.iter().for_each(|(ip, ping_list)| {
                        ui.columns(2, |columns| {
                            columns[0].label(ip.to_string());
                            ping_list.iter().for_each(|info| {
                                columns[1].label(match info {
                                    Ok(ping) => ping.to_string(),
                                    Err(_) => "Error".to_string(),
                                });
                            });
                        });
                        ui.end_row();
                    });
                });
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
                            let unban_res = server.unban(&self.firewall);
                            if let Err(err) = unban_res {
                                log::error!("{}", err);
                            }
                        });
                        self.pinger_message_sender
                            .send(PingerMessage::ClearList)
                            .unwrap();
                        self.send_currently_active_ip_list_to_pinger();
                    }
                    if columns[3].button("Disable All").clicked() {
                        self.servers.get_servers().iter().for_each(|server| {
                            let ban_res = server.ban(&self.firewall);
                            if let Err(err) = ban_res {
                                log::error!("{}", err);
                            }
                        });
                        self.ping_info.clear();
                        self.pinger_message_sender
                            .send(PingerMessage::ClearList)
                            .unwrap();
                    }
                    columns[4].label("Ping");
                    columns[5].label("Loss");
                });
                ui.end_row();

                for server in self.servers.get_servers() {
                    let ping_info_remove_ips = ui.columns(num_columns, |columns| {
                        let mut ping_info_remove_ips = None;

                        columns[0].label(server.get_abr());

                        columns[1]
                            .label(server.get_cached_server_state(&self.firewall).to_string());

                        if columns[2].button("Enable").clicked() {
                            let unban_res = server.unban(&self.firewall);
                            if let Err(err) = unban_res {
                                log::error!("{}", err);
                            }

                            // update pinger ip list
                            let ips = server.get_ipv4s().to_vec();
                            ips.iter().for_each(|ip| {
                                self.pinger_message_sender
                                    .send(PingerMessage::RemoveFromList(*ip))
                                    .unwrap();
                            });
                            self.pinger_message_sender
                                .send(PingerMessage::AppendToList(ips))
                                .unwrap();
                        }

                        if columns[3].button("Disable").clicked() {
                            let ban_res = server.ban(&self.firewall);
                            if let Err(err) = ban_res {
                                log::error!("{}", err);
                            }

                            let ips = server.get_ipv4s().to_vec();

                            // update pinger ip list
                            ips.iter().for_each(|ip| {
                                self.pinger_message_sender
                                    .send(PingerMessage::RemoveFromList(*ip))
                                    .unwrap();
                            });

                            ping_info_remove_ips = Some(ips);
                        }

                        let (total_ping, total_num_packets, lost_packets) = server
                            .get_ipv4s()
                            .iter()
                            .fold((Duration::ZERO, 0, 0), |acc, ip| {
                                let (ping, total_num_packets, lost_packets) =
                                    self.calculate_total_ping_for_ip(*ip);
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
                            columns[4].label("NA");
                            columns[5].label("100.00%");
                        } else {
                            columns[4].label(format!("{}", PingInfo::new(ping)));
                            columns[5].label(format!(
                                "{:.2}%",
                                lost_packets as f64 / total_num_packets as f64 * 100.0
                            ));
                        }

                        ping_info_remove_ips
                    });

                    if let Some(ip_list) = ping_info_remove_ips {
                        for ip in ip_list.iter() {
                            self.ping_info.remove(ip);
                        }
                    }

                    ui.end_row();
                }
            });
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
