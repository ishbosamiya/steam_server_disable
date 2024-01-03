use std::{
    collections::{HashMap, VecDeque},
    convert::TryInto,
    net::Ipv4Addr,
    sync::{mpsc, Arc},
    thread,
    time::Duration,
};

use crate::{
    egui,
    firewall::Firewall,
    ping::{self, PingInfo, Pinger},
    steam_server::{ServerInfo, ServerState, Servers},
};

#[derive(Debug)]
pub enum PingerMessage {
    PushToList(Ipv4Addr),
    RemoveFromList(Ipv4Addr),
    AppendToList(Vec<Ipv4Addr>),
    ClearList,
    KillThread,
}

pub enum ServerStatusMessage {
    AppendToList(Vec<(String, Vec<Ipv4Addr>)>),
    RemoveServer(String),
    ClearList,
    KillThread,
}

pub struct App {
    servers: Servers,
    firewall: Arc<Firewall>,

    ip_selection_status: HashMap<Ipv4Addr, bool>,

    ping_info: HashMap<Ipv4Addr, VecDeque<Result<PingInfo, ping::Error>>>,

    pinger_message_sender: mpsc::Sender<PingerMessage>,
    ping_receiver: mpsc::Receiver<(Ipv4Addr, Result<PingInfo, ping::Error>)>,
    pinger_thread_handle: Option<thread::JoinHandle<()>>,

    server_status_info: HashMap<String, ServerState>,
    server_status_message_sender: mpsc::Sender<ServerStatusMessage>,
    server_status_receiver: mpsc::Receiver<(String, ServerState)>,
    server_status_thread_handle: Option<thread::JoinHandle<()>>,
}

impl Drop for App {
    fn drop(&mut self) {
        // request threads to stop
        self.server_status_message_sender
            .send(ServerStatusMessage::KillThread)
            .unwrap();
        self.pinger_message_sender
            .send(PingerMessage::KillThread)
            .unwrap();

        // wait for threads to join
        self.server_status_thread_handle
            .take()
            .unwrap()
            .join()
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
                        // add ip if it doesn't already exist in the list
                        if !list.iter().any(|ip| *ip == add_ip) {
                            list.push(add_ip);
                        }
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
                        ip_list.into_iter().for_each(|add_ip| {
                            // add ip if it doesn't already exist in the list
                            if !list.iter().any(|ip| *ip == add_ip) {
                                list.push(add_ip);
                            }
                        });
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

        let firewall = Arc::new(Firewall::new());

        let (server_status_message_sender, server_status_message_receiver) =
            mpsc::channel::<ServerStatusMessage>();
        let (server_status_sender, server_status_receiver) =
            mpsc::channel::<(String, ServerState)>();

        let thread_firewall = firewall.clone();
        let server_status_thread_handle = thread::spawn(|| {
            let server_status_message_receiver = server_status_message_receiver;
            let server_status_sender = server_status_sender;
            let firewall = thread_firewall;

            let mut list = VecDeque::new();
            loop {
                let messages: Vec<_> = server_status_message_receiver.try_iter().collect();
                if messages
                    .iter()
                    .any(|message| matches!(message, ServerStatusMessage::KillThread))
                {
                    break;
                }

                messages.into_iter().for_each(|message| match message {
                    ServerStatusMessage::AppendToList(add_list) => {
                        debug_assert!(
                            !list.iter().any(|(server, _)| add_list
                                .iter()
                                .any(|(add_server, _add_ip_list)| server == add_server)),
                            "attempting to add duplicate server to the server status list"
                        );
                        list.extend(add_list.into_iter());
                    }
                    ServerStatusMessage::RemoveServer(remove_server) => {
                        // Remove server from list if it exists, no
                        // error if it does not exist
                        if let Some(server_index) =
                            list.iter().enumerate().find_map(|(index, (server, _))| {
                                (server == &remove_server).then(|| index)
                            })
                        {
                            list.remove(server_index);
                        }
                    }
                    ServerStatusMessage::ClearList => list.clear(),
                    ServerStatusMessage::KillThread => unreachable!(),
                });

                if let Some((server, ip_list)) = list.pop_front() {
                    let ip_list_len = ip_list.len();
                    let blocked_ip_list = ip_list
                        .into_iter()
                        .filter_map(|ip| {
                            if let Ok(blocked) = firewall.is_blocked(ip) {
                                blocked.then(|| ip)
                            } else {
                                // Drop the firewall error
                                None
                            }
                        })
                        .collect::<Vec<_>>();
                    let server_state = if blocked_ip_list.len() == ip_list_len {
                        ServerState::AllDisabled
                    } else if blocked_ip_list.is_empty() {
                        ServerState::NoneDisabled
                    } else {
                        ServerState::SomeDisabled(blocked_ip_list)
                    };

                    server_status_sender.send((server, server_state)).unwrap();
                } else {
                    // not a high priority
                    thread::sleep(Duration::from_millis(500));
                }
            }
        });

        let servers = Servers::new();
        let ip_selection_status = servers
            .get_servers()
            .iter()
            .flat_map(|server| server.get_ipv4s().iter().map(|ip| (*ip, false)))
            .collect();

        let res = Self {
            servers,
            firewall,

            ip_selection_status,

            ping_info: HashMap::new(),
            pinger_message_sender,
            ping_receiver,
            pinger_thread_handle: Some(pinger_thread_handle),

            server_status_info: HashMap::new(),
            server_status_message_sender,
            server_status_receiver,
            server_status_thread_handle: Some(server_status_thread_handle),
        };

        // send all the servers to the server status gatherer thread
        res.server_status_message_sender
            .send(ServerStatusMessage::AppendToList(
                res.servers
                    .get_servers()
                    .iter()
                    .map(|info| {
                        let server = info.get_abr().to_string();
                        let ips = info.get_ipv4s().to_vec();
                        (server, ips)
                    })
                    .collect(),
            ))
            .unwrap();

        res.send_currently_active_ip_list_to_pinger();

        res
    }

    /// note: it is generally a good idea to clear the list before
    /// sending the complete server ip list to the pinger thread, it
    /// can lead to duplications otherwise
    fn send_currently_active_ip_list_to_pinger(&self) {
        self.servers.get_servers().iter().for_each(|info| {
            if !matches!(
                self.server_status_info
                    .get(info.get_abr())
                    .unwrap_or(&ServerState::Unknown),
                ServerState::AllDisabled
            ) {
                self.pinger_message_sender
                    .send(PingerMessage::AppendToList(info.get_ipv4s().to_vec()))
                    .unwrap();
            }
        });
    }

    /// Update server status info by flushing the server status messages channel.
    fn update_server_status_info(&mut self) {
        let server_status_info = &mut self.server_status_info;
        let servers = &self.servers;
        let pinger_message_sender = &self.pinger_message_sender;
        let mut ping_info_remove_ips = Vec::new();
        self.server_status_receiver
            .try_iter()
            .for_each(|(server_abr, status)| {
                let server = servers
                    .get_servers()
                    .iter()
                    .find(|info| info.get_abr() == server_abr)
                    .unwrap();

                match &status {
                    ServerState::AllDisabled => {
                        server.get_ipv4s().iter().for_each(|ip| {
                            pinger_message_sender
                                .send(PingerMessage::RemoveFromList(*ip))
                                .unwrap();
                        });

                        ping_info_remove_ips.extend(server.get_ipv4s().iter().copied());
                    }
                    ServerState::SomeDisabled(disabled_ips) => {
                        // remove disabled ips from the list
                        disabled_ips.iter().for_each(|ip| {
                            pinger_message_sender
                                .send(PingerMessage::RemoveFromList(*ip))
                                .unwrap();
                        });

                        // tell to ping non disabled ips
                        pinger_message_sender
                            .send(PingerMessage::AppendToList(
                                server
                                    .get_ipv4s()
                                    .iter()
                                    .copied()
                                    .filter(|ip| {
                                        !disabled_ips.iter().any(|disabled_ip| disabled_ip == ip)
                                    })
                                    .collect(),
                            ))
                            .unwrap();

                        ping_info_remove_ips.extend(disabled_ips.iter());
                    }
                    ServerState::NoneDisabled => {
                        pinger_message_sender
                            .send(PingerMessage::AppendToList(server.get_ipv4s().to_vec()))
                            .unwrap();
                    }
                    ServerState::Unknown => unreachable!(),
                }

                let server_status = server_status_info
                    .entry(server_abr)
                    .or_insert(ServerState::Unknown);
                *server_status = status;
            });

        if !ping_info_remove_ips.is_empty() {
            // hack: wait for the channel to get all the
            // messages before flushing them
            std::thread::sleep(Duration::from_secs(1));
            // flush the ping messages channel
            self.update_ping_info();

            ping_info_remove_ips.iter().for_each(|ip| {
                self.ping_info.remove(ip);
            });
        }
    }

    /// Update ping info by flushing the ping messages channel.
    fn update_ping_info(&mut self) {
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

    /// Update all information that must happen very so often. eg:
    /// ping information receiving
    pub fn update(&mut self) {
        self.update_ping_info();
        self.update_server_status_info();
    }

    /// Calculate the total ping for the given ip. Returns the rtt, total
    /// number of packets number of packets dropped.
    ///
    /// note: this returns the total ping not the average ping of the
    /// packets
    fn calculate_total_ping_for_ip(
        ping_info: &HashMap<Ipv4Addr, VecDeque<Result<PingInfo, ping::Error>>>,
        ip: Ipv4Addr,
    ) -> (Duration, usize, usize) {
        ping_info
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

    /// Enable all servers.
    fn enable_all_servers(&self) {
        for server in self.servers.get_servers().iter() {
            let unban_res = server.unban(&self.firewall);
            if let Err(err) = unban_res {
                log::error!("{}: {}", server.get_abr(), err);
            }

            // send message to server status checker
            // to update server status
            self.server_status_message_sender
                .send(ServerStatusMessage::AppendToList(vec![(
                    server.get_abr().to_string(),
                    server.get_ipv4s().to_vec(),
                )]))
                .unwrap();
        }
        self.pinger_message_sender
            .send(PingerMessage::ClearList)
            .unwrap();
        self.send_currently_active_ip_list_to_pinger();
    }

    /// Disable all servers.
    fn disable_all_servers(&mut self) {
        for server in self.servers.get_servers().iter() {
            let ban_res = server.ban(&self.firewall);
            if let Err(err) = ban_res {
                log::error!("{}: {}", server.get_abr(), err);
            }

            // send message to server status checker
            // to update server status
            self.server_status_message_sender
                .send(ServerStatusMessage::AppendToList(vec![(
                    server.get_abr().to_string(),
                    server.get_ipv4s().to_vec(),
                )]))
                .unwrap();
        }

        self.pinger_message_sender
            .send(PingerMessage::ClearList)
            .unwrap();

        // hack: wait for the channel to get all the
        // messages before flushing them
        std::thread::sleep(Duration::from_secs(1));
        // flush the ping messages channel
        self.update_ping_info();

        self.ping_info.clear();
    }

    /// Enable the given server.
    fn enable_server(
        server: &ServerInfo,
        firewall: &Firewall,
        server_status_message_sender: &mpsc::Sender<ServerStatusMessage>,
        pinger_message_sender: &mpsc::Sender<PingerMessage>,
    ) {
        let unban_res = server.unban(firewall);
        if let Err(err) = unban_res {
            log::error!("{}: {}", server.get_abr(), err);
        }

        // send message to server status checker
        // to update server status
        server_status_message_sender
            .send(ServerStatusMessage::AppendToList(vec![(
                server.get_abr().to_string(),
                server.get_ipv4s().to_vec(),
            )]))
            .unwrap();

        // update pinger ip list
        let ips = server.get_ipv4s().to_vec();
        ips.iter().for_each(|ip| {
            pinger_message_sender
                .send(PingerMessage::RemoveFromList(*ip))
                .unwrap();
        });
        pinger_message_sender
            .send(PingerMessage::AppendToList(ips))
            .unwrap();
    }

    /// Disable the given server.
    fn disable_server(
        server: &ServerInfo,
        firewall: &Firewall,
        server_status_message_sender: &mpsc::Sender<ServerStatusMessage>,
        pinger_message_sender: &mpsc::Sender<PingerMessage>,
        ping_info_remove_ips: &mut Option<Vec<Ipv4Addr>>,
    ) {
        let ban_res = server.ban(firewall);
        if let Err(err) = ban_res {
            log::error!("{}: {}", server.get_abr(), err);
        }

        // send message to server status checker
        // to update server status
        server_status_message_sender
            .send(ServerStatusMessage::AppendToList(vec![(
                server.get_abr().to_string(),
                server.get_ipv4s().to_vec(),
            )]))
            .unwrap();

        let ips = server.get_ipv4s().to_vec();

        // update pinger ip list
        ips.iter().for_each(|ip| {
            pinger_message_sender
                .send(PingerMessage::RemoveFromList(*ip))
                .unwrap();
        });

        if let Some(prev_removed_ips) = ping_info_remove_ips {
            prev_removed_ips.extend(ips.into_iter());
        } else {
            *ping_info_remove_ips = Some(ips);
        }
    }

    /// Enable the given IP.
    fn enable_ip(
        ip: Ipv4Addr,
        server: &ServerInfo,
        firewall: &Firewall,
        server_status_message_sender: &mpsc::Sender<ServerStatusMessage>,
        pinger_message_sender: &mpsc::Sender<PingerMessage>,
    ) {
        let unban_res = firewall.unban_ip(ip);
        if let Err(err) = unban_res {
            log::error!("{}: {}", server.get_abr(), err);
        }

        // send message to server status checker
        // to update server status
        server_status_message_sender
            .send(ServerStatusMessage::RemoveServer(
                server.get_abr().to_string(),
            ))
            .unwrap();
        server_status_message_sender
            .send(ServerStatusMessage::AppendToList(vec![(
                server.get_abr().to_string(),
                server.get_ipv4s().to_vec(),
            )]))
            .unwrap();

        // update pinger ip list
        pinger_message_sender
            .send(PingerMessage::PushToList(ip))
            .unwrap();
    }

    /// Disable the given IP.
    fn disable_ip(
        ip: Ipv4Addr,
        server: &ServerInfo,
        firewall: &Firewall,
        server_status_message_sender: &mpsc::Sender<ServerStatusMessage>,
        pinger_message_sender: &mpsc::Sender<PingerMessage>,
        ping_info_remove_ips: &mut Option<Vec<Ipv4Addr>>,
    ) {
        let ban_res = firewall.ban_ip(ip);
        if let Err(err) = ban_res {
            log::error!("{}: {}", server.get_abr(), err);
        }

        // send message to server status checker
        // to update server status
        server_status_message_sender
            .send(ServerStatusMessage::RemoveServer(
                server.get_abr().to_string(),
            ))
            .unwrap();
        server_status_message_sender
            .send(ServerStatusMessage::AppendToList(vec![(
                server.get_abr().to_string(),
                server.get_ipv4s().to_vec(),
            )]))
            .unwrap();

        // update pinger ip list
        pinger_message_sender
            .send(PingerMessage::RemoveFromList(ip))
            .unwrap();

        if let Some(prev_removed_ips) = ping_info_remove_ips {
            prev_removed_ips.push(ip);
        } else {
            *ping_info_remove_ips = Some(vec![ip]);
        }
    }

    /// Get the [`ServerSelectionStatus`] for the given
    /// [`Servers`]. The returned vector will have the elements
    /// correspond exactly with the given servers (so zipping the
    /// result is possible).
    fn servers_selection_status(
        servers: &Servers,
        ip_selection_status: &HashMap<Ipv4Addr, bool>,
    ) -> Vec<ServerSelectionStatus> {
        servers
            .get_servers()
            .iter()
            .map(|server| {
                let num_ips_selected = server
                    .get_ipv4s()
                    .iter()
                    .filter(|ip| *ip_selection_status.get(*ip).unwrap_or(&false))
                    .count();

                if num_ips_selected == 0 {
                    ServerSelectionStatus::None
                } else if num_ips_selected == server.get_ipv4s().len() {
                    ServerSelectionStatus::All
                } else {
                    ServerSelectionStatus::Some
                }
            })
            .collect::<Vec<_>>()
    }

    /// Enable the IPs that are currently selected.
    fn enable_selected_ips(&self) {
        let servers_selected =
            Self::servers_selection_status(&self.servers, &self.ip_selection_status);
        if servers_selected
            .iter()
            .all(|selected| matches!(selected, ServerSelectionStatus::All))
        {
            // this is for optimization, if all the
            // servers are selected, then it is faster
            // to enable all the servers
            self.enable_all_servers();
        } else {
            self.servers
                .get_servers()
                .iter()
                .zip(servers_selected.into_iter())
                .for_each(|(server, status)| match status {
                    ServerSelectionStatus::All => {
                        Self::enable_server(
                            server,
                            &self.firewall,
                            &self.server_status_message_sender,
                            &self.pinger_message_sender,
                        );
                    }
                    ServerSelectionStatus::Some => {
                        server
                            .get_ipv4s()
                            .iter()
                            .filter(|ip| *self.ip_selection_status.get(ip).unwrap_or(&false))
                            .for_each(|ip| {
                                Self::enable_ip(
                                    *ip,
                                    server,
                                    &self.firewall,
                                    &self.server_status_message_sender,
                                    &self.pinger_message_sender,
                                )
                            });
                    }
                    ServerSelectionStatus::None => {
                        // do nothing
                    }
                });
        }
    }

    /// Disable the IPs that are currently selected.
    fn disable_selected_ips(&mut self) {
        let servers_selected =
            Self::servers_selection_status(&self.servers, &self.ip_selection_status);
        if servers_selected
            .iter()
            .all(|selected| matches!(selected, ServerSelectionStatus::All))
        {
            // this is for optimization, if all the
            // servers are selected, then it is faster
            // to enable all the servers
            self.disable_all_servers();
        } else {
            let mut ping_info_remove_ips: Option<Vec<Ipv4Addr>> = None;
            self.servers
                .get_servers()
                .iter()
                .zip(servers_selected.into_iter())
                .for_each(|(server, status)| match status {
                    ServerSelectionStatus::All => {
                        Self::disable_server(
                            server,
                            &self.firewall,
                            &self.server_status_message_sender,
                            &self.pinger_message_sender,
                            &mut ping_info_remove_ips,
                        );
                    }
                    ServerSelectionStatus::Some => {
                        server
                            .get_ipv4s()
                            .iter()
                            .filter(|ip| *self.ip_selection_status.get(ip).unwrap_or(&false))
                            .for_each(|ip| {
                                Self::disable_ip(
                                    *ip,
                                    server,
                                    &self.firewall,
                                    &self.server_status_message_sender,
                                    &self.pinger_message_sender,
                                    &mut ping_info_remove_ips,
                                )
                            });
                    }
                    ServerSelectionStatus::None => {
                        // do nothing
                    }
                });
            if let Some(ip_list) = ping_info_remove_ips {
                // HACK: wait for the channel to get all the
                // messages before flushing them
                std::thread::sleep(Duration::from_secs(1));
                // flush the ping messages channel
                self.update_ping_info();

                for ip in ip_list.iter() {
                    self.ping_info.remove(ip);
                }
            }
        }
    }

    /// Enable the matching IPs of the server regions matching the
    /// given regex.
    pub fn enable_matching(&mut self, regex: &regex::Regex) {
        self.servers
            .get_servers()
            .iter()
            .filter(|server| regex.is_match(server.get_abr()))
            .for_each(|server| {
                Self::enable_server(
                    server,
                    &self.firewall,
                    &self.server_status_message_sender,
                    &self.pinger_message_sender,
                );
            });
    }

    /// Draw the UI for the [`App`].
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
            egui::Window::new("debug_ping_info_window")
                .vscroll(true)
                .show(ui.ctx(), |ui| {
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
                });
        }

        let num_columns = 6;
        egui::Grid::new("ui_grid")
            .max_col_width(ui.available_width())
            .num_columns(num_columns)
            .striped(true)
            .show(ui, |ui| {
                ui.columns(num_columns, |columns| {
                    columns[0].label("Region");

                    columns[1].horizontal(|ui| {
                        let mut all_ips_selected =
                            self.ip_selection_status.values().all(|selected| *selected);
                        let prev_all_ips_selected = all_ips_selected;
                        ui.checkbox(&mut all_ips_selected, "");
                        if prev_all_ips_selected != all_ips_selected {
                            // the user selected or deselected all ips
                            self.ip_selection_status
                                .values_mut()
                                .for_each(|selected| *selected = all_ips_selected);
                        }

                        ui.label("State");
                    });
                    if columns[2].button("Enable Selected").clicked() {
                        self.enable_selected_ips();
                    }
                    if columns[3].button("Disable Selected").clicked() {
                        self.disable_selected_ips();
                    }
                    columns[4].label("Ping");
                    columns[5].label("Loss");
                });
                ui.end_row();

                let server_status_message_sender = &self.server_status_message_sender;
                let server_status_info = &self.server_status_info;
                let pinger_message_sender = &self.pinger_message_sender;
                let ping_info = &mut self.ping_info;
                let firewall = self.firewall.clone();
                let mut ping_info_remove_ips: Option<Vec<Ipv4Addr>> = None;
                for server in self.servers.get_servers() {
                    ui.columns(num_columns, |columns| {
                        let ip_list_shown = columns[0]
                            .collapsing(server.get_abr(), |ui| {
                                server.get_ipv4s().iter().for_each(|ip| {
                                    ui.label(ip.to_string());
                                });
                            })
                            .body_returned
                            .is_some();

                        let server_status = &*server_status_info
                            .get(server.get_abr())
                            .unwrap_or(&ServerState::Unknown);

                        columns[1].horizontal(|ui| {
                            let mut all_ips_selected = server
                                .get_ipv4s()
                                .iter()
                                .all(|ip| *self.ip_selection_status.entry(*ip).or_insert(false));
                            let prev_all_ips_selected = all_ips_selected;
                            ui.checkbox(&mut all_ips_selected, "");
                            if prev_all_ips_selected != all_ips_selected {
                                // the user selected or deselected all ips
                                server.get_ipv4s().iter().for_each(|ip| {
                                    *self.ip_selection_status.get_mut(ip).unwrap() =
                                        all_ips_selected
                                });
                            }
                            ui.label(server_status.to_string());
                        });

                        if columns[2].button("Enable").clicked() {
                            Self::enable_server(
                                server,
                                &firewall,
                                server_status_message_sender,
                                pinger_message_sender,
                            );
                        }

                        if ip_list_shown {
                            server.get_ipv4s().iter().for_each(|ip| {
                                columns[1]
                                    .checkbox(self.ip_selection_status.get_mut(ip).unwrap(), "");
                                if columns[2].button(format!("Enable {}", ip)).clicked() {
                                    Self::enable_ip(
                                        *ip,
                                        server,
                                        &firewall,
                                        server_status_message_sender,
                                        pinger_message_sender,
                                    );
                                }
                            });
                        }

                        if columns[3].button("Disable").clicked() {
                            Self::disable_server(
                                server,
                                &firewall,
                                server_status_message_sender,
                                pinger_message_sender,
                                &mut ping_info_remove_ips,
                            );
                        }

                        if ip_list_shown {
                            server.get_ipv4s().iter().for_each(|ip| {
                                if columns[3].button(format!("Disable {}", ip)).clicked() {
                                    Self::disable_ip(
                                        *ip,
                                        server,
                                        &firewall,
                                        server_status_message_sender,
                                        pinger_message_sender,
                                        &mut ping_info_remove_ips,
                                    );
                                }
                            });
                        }

                        if let ServerState::AllDisabled = server_status {
                            columns[4].label("Disabled");
                            columns[5].label("Disabled");
                        } else {
                            let server_ping_info: Vec<_> = server
                                .get_ipv4s()
                                .iter()
                                .map(|ip| {
                                    if ping_info.contains_key(ip) {
                                        Some(Self::calculate_total_ping_for_ip(ping_info, *ip))
                                    } else {
                                        None
                                    }
                                })
                                .collect();

                            let (server_total_ping, server_num_packets, server_lost_packets) =
                                server_ping_info
                                    .iter()
                                    .filter_map(|ping_info| ping_info.as_ref())
                                    .fold(
                                        (Duration::ZERO, 0, 0),
                                        |acc, (ping, total_num_packets, lost_packets)| {
                                            (
                                                acc.0 + *ping,
                                                acc.1 + total_num_packets,
                                                acc.2 + lost_packets,
                                            )
                                        },
                                    );

                            let ui_ping_info =
                                |ping_ui: &mut egui::Ui,
                                 loss_ui: &mut egui::Ui,
                                 total_ping: Duration,
                                 num_packets: usize,
                                 lost_packets: usize| {
                                    let num_valid_packets =
                                        (num_packets - lost_packets).try_into().unwrap();
                                    let ping = if num_valid_packets == 0 {
                                        total_ping
                                    } else {
                                        total_ping / num_valid_packets
                                    };

                                    if num_packets == lost_packets {
                                        ping_ui.label("NA");
                                        loss_ui.label("100.00%");
                                    } else {
                                        ping_ui.label(format!("{}", PingInfo::new(ping)));
                                        loss_ui.label(format!(
                                            "{:.2}%",
                                            lost_packets as f64 / num_packets as f64 * 100.0
                                        ));
                                    }
                                };

                            let (ping_ui, column_ui) = {
                                let splits = columns.split_at_mut(5);
                                (splits.0.last_mut().unwrap(), splits.1.first_mut().unwrap())
                            };

                            ui_ping_info(
                                ping_ui,
                                column_ui,
                                server_total_ping,
                                server_num_packets,
                                server_lost_packets,
                            );

                            if ip_list_shown {
                                server_ping_info.into_iter().for_each(|ping_info| {
                                    if let Some((total_ping, num_packets, lost_packets)) = ping_info
                                    {
                                        ui_ping_info(
                                            ping_ui,
                                            column_ui,
                                            total_ping,
                                            num_packets,
                                            lost_packets,
                                        );
                                    } else {
                                        ping_ui.label("NA");
                                        column_ui.label("100.00%");
                                    }
                                });
                            }
                        }
                    });

                    ui.end_row();
                }

                if let Some(ip_list) = ping_info_remove_ips {
                    // HACK: wait for the channel to get all the
                    // messages before flushing them
                    std::thread::sleep(Duration::from_secs(1));
                    // flush the ping messages channel
                    self.update_ping_info();

                    for ip in ip_list.iter() {
                        self.ping_info.remove(ip);
                    }
                }
            });
    }
}

/// Server selection status.
enum ServerSelectionStatus {
    /// All IPs are selected.
    All,
    /// Some IPs are selected.
    Some,
    /// No IPs are selected.
    None,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
