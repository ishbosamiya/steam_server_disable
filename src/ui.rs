use crossbeam_channel::{bounded, Receiver};
use fastping_rs::{PingResult, Pinger};
use iced::{
    button, executor, scrollable, Application, Button, Clipboard, Command, Element, Length, Row,
    Scrollable, Subscription, Text,
};

use std::collections::VecDeque;
use std::convert::TryInto;
use std::sync::{Arc, RwLock};
use std::thread;

use crate::steam_server;
use crate::steam_server::{PingInfo, ServerObject, ServerState};

struct IPTables(iptables::IPTables);

#[derive(Default)]
pub struct UI {
    scroll: scrollable::State,
    server_obj: Arc<RwLock<ServerObject>>,
    ipt: IPTables,
    buttons: Vec<Server>,
    download_button: button::State,
    enable_all_button: button::State,
    disable_all_button: button::State,
    ping_receiver: Option<Receiver<(String, PingInfo)>>,
}

struct Server {
    abr: String,
    enable_button: button::State,
    disable_button: button::State,
    state: ServerState,
    ping: VecDeque<PingInfo>,
}

impl Server {
    fn new(
        abr: String,
        enable_button: button::State,
        disable_button: button::State,
        state: ServerState,
    ) -> Self {
        return Self {
            abr,
            enable_button,
            disable_button,
            state,
            ping: VecDeque::with_capacity(4),
        };
    }
}

impl Default for IPTables {
    fn default() -> Self {
        return IPTables(iptables::new(false).unwrap());
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    EnableServer(String),
    DisableServer(String),
    EnableAll,
    DisableAll,
    DownloadFile,
    UpdatePingInfo,
}

impl Application for UI {
    type Executor = executor::Default;
    type Message = Message;
    type Flags = ();

    fn new(_flags: ()) -> (Self, Command<Self::Message>) {
        let mut ui = Self::default();
        let server_obj = ui.server_obj.clone();
        let server_obj = server_obj.read().unwrap();
        let server_list = server_obj.get_server_list();
        let server_list: Vec<String> = server_list
            .iter()
            .filter(|server| {
                if let Ok(_) = server_obj.get_server_ips(server) {
                    return true;
                }
                return false;
            })
            .map(|server| server.to_string())
            .collect();
        server_list.iter().for_each(|server| {
            ui.buttons.push(Server::new(
                server.to_string(),
                button::State::new(),
                button::State::new(),
                server_obj
                    .get_server_state(&ui.ipt.0, server)
                    .expect("couldnt get state of some server"),
            ))
        });

        let (ping_sender, ping_receiver) = bounded(server_list.len());
        let server_obj = ui.server_obj.clone();

        server_list.iter().for_each(|server| {
            let server = server.clone();
            let ping_sender = ping_sender.clone();
            let server_obj = server_obj.clone();
            thread::spawn(move || {
                let (pinger, results) = Pinger::new(None, None).expect("couldn't create pinger");
                let server_obj = server_obj.read().unwrap();
                let ips = server_obj.get_server_ips(&server).unwrap();
                ips.iter().for_each(|ip| pinger.add_ipaddr(ip));
                pinger.run_pinger();
                loop {
                    let total_elapsed = results.iter().take(ips.len()).try_fold(
                        std::time::Duration::from_millis(0),
                        |elapsed, result| match result {
                            PingResult::Idle { addr: _ } => {
                                Err(steam_server::Error::ServerUnreachable)
                            }
                            PingResult::Receive { addr: _, rtt } => Ok(elapsed + rtt),
                        },
                    );
                    match total_elapsed {
                        Ok(rtt) => {
                            let rtt = rtt / ips.len().try_into().unwrap();
                            ping_sender
                                .send((server.to_string(), PingInfo::Rtt(rtt)))
                                .expect("couldn't send ping info");
                        }
                        Err(_) => {
                            ping_sender
                                .send((server.to_string(), PingInfo::Unreachable))
                                .expect("couldn't send ping info");
                        }
                    }
                    thread::sleep(std::time::Duration::from_millis(500));
                }
            });
        });

        ui.ping_receiver = Some(ping_receiver);

        return (ui, Command::none());
    }

    fn title(&self) -> String {
        return String::from("Steam Server Toggle");
    }

    fn update(
        &mut self,
        message: Self::Message,
        _clipboard: &mut Clipboard,
    ) -> Command<Self::Message> {
        let server_obj = self.server_obj.read().unwrap();
        match message {
            Message::EnableServer(server_abr) => {
                server_obj.unban_server(&self.ipt.0, &server_abr).unwrap();
                self.buttons
                    .iter_mut()
                    .filter(|server| {
                        if server.abr == server_abr {
                            return true;
                        }
                        return false;
                    })
                    .for_each(|server| server.state = ServerState::NoneDisabled);
            }
            Message::DisableServer(server_abr) => {
                server_obj.ban_server(&self.ipt.0, &server_abr).unwrap();
                self.buttons
                    .iter_mut()
                    .filter(|server| {
                        if server.abr == server_abr {
                            return true;
                        }
                        return false;
                    })
                    .for_each(|server| server.state = ServerState::AllDisabled);
            }
            Message::EnableAll => {
                self.buttons.iter().for_each(|server| {
                    server_obj.unban_server(&self.ipt.0, &server.abr).unwrap();
                });
                self.buttons.iter_mut().for_each(|server| {
                    server.state = ServerState::NoneDisabled;
                });
            }
            Message::DisableAll => {
                self.buttons
                    .iter()
                    .for_each(|server| server_obj.ban_server(&self.ipt.0, &server.abr).unwrap());
                self.buttons.iter_mut().for_each(|server| {
                    server.state = ServerState::AllDisabled;
                });
            }
            Message::DownloadFile => {
                ServerObject::download_file()
                    .expect("couldn't download file, todo: make it not panic");
            }
            Message::UpdatePingInfo => {
                // in case there were ping measurements done by the other thread, retrieve them here
                let ping_receiver = self.ping_receiver.as_ref().unwrap();
                while let Ok((server, info)) = ping_receiver.try_recv() {
                    self.buttons
                        .iter_mut()
                        .filter(|button| button.abr == server)
                        .for_each(|button| {
                            if button.ping.len() > 15 {
                                button.ping.pop_front();
                            }
                            button.ping.push_back(info);
                        });
                }
            }
        }

        return Command::none();
    }

    fn view(&mut self) -> Element<Message> {
        let mut content = Scrollable::new(&mut self.scroll)
            .width(Length::Fill)
            .spacing(10);
        content = content.push(
            Row::new()
                .spacing(10)
                .push(
                    Button::new(&mut self.download_button, Text::new("Download file"))
                        .on_press(Message::DownloadFile),
                )
                .push(
                    Button::new(&mut self.enable_all_button, Text::new("Enable All"))
                        .on_press(Message::EnableAll),
                )
                .push(
                    Button::new(&mut self.disable_all_button, Text::new("Disable All"))
                        .on_press(Message::DisableAll),
                ),
        );
        for server in &mut self.buttons {
            let mut row = Row::new().spacing(10);
            row = row.push(
                Text::new(server.abr.clone())
                    .size(20)
                    .width(Length::Units(60)),
            );
            row = row.push(
                Button::new(&mut server.enable_button, Text::new("Enable"))
                    .on_press(Message::EnableServer(server.abr.clone()))
                    .width(Length::Units(80)),
            );
            row = row.push(
                Button::new(&mut server.disable_button, Text::new("Disable"))
                    .on_press(Message::DisableServer(server.abr.clone()))
                    .width(Length::Units(80)),
            );
            row = row.push(
                Text::new(format!("{}", server.state))
                    .size(20)
                    .width(Length::Units(150)),
            );
            let ping_info;
            if server.ping.len() <= 4 {
                ping_info = PingInfo::Unknown;
            } else {
                let rtt = server
                    .ping
                    .range(server.ping.len() - 3..)
                    .filter(|info| {
                        if let PingInfo::Rtt(_) = info {
                            return true;
                        } else {
                            return false;
                        }
                    })
                    .fold(
                        (0, std::time::Duration::from_millis(0)),
                        |(num_valid, elapsed), info| match info {
                            PingInfo::Rtt(rtt) => (num_valid + 1, elapsed + *rtt),
                            _ => panic!("filter didn't filter out non Rtt of PingInfo"),
                        },
                    );
                if rtt.0 == 0 {
                    ping_info = PingInfo::Unreachable;
                } else {
                    ping_info = PingInfo::Rtt(rtt.1 / rtt.0);
                }
            }
            row = row.push(
                Text::new(format!("{}", ping_info))
                    .size(20)
                    .width(Length::Units(180)),
            );
            let loss_info;
            if server.ping.len() == 0 {
                loss_info = 100.0;
            } else {
                loss_info = (server.ping.len()
                    - server
                        .ping
                        .iter()
                        .filter(|info| {
                            if let PingInfo::Rtt(_) = info {
                                return true;
                            } else {
                                return false;
                            }
                        })
                        .count()) as f64
                    / server.ping.len() as f64
                    * 100.0;
            }
            row = row.push(
                Text::new(format!("{:.2} loss", loss_info))
                    .size(20)
                    .width(Length::Units(180)),
            );
            content = content.push(row);
        }
        content.into()
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        iced::time::every(std::time::Duration::from_secs(2)).map(|_| Message::UpdatePingInfo)
    }
}
