use crossbeam_channel::{bounded, Receiver};
use iced::{button, scrollable, Button, Element, Length, Row, Sandbox, Scrollable, Text};

use std::sync::{Arc, RwLock};
use std::thread;

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
    ping: PingInfo,
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
            ping: PingInfo::Unknown,
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
}

impl Sandbox for UI {
    type Message = Message;

    fn new() -> Self {
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
        thread::spawn(|| {
            let ping_sender = ping_sender;
            let server_list = server_list;
            let server_obj = server_obj;
            let server_obj = server_obj.read().unwrap();
            loop {
                server_list
                    .iter()
                    .for_each(|server| match server_obj.get_server_ping(&server) {
                        Ok(rtt) => {
                            ping_sender
                                .send((server.to_string(), PingInfo::Rtt(rtt)))
                                .unwrap();
                        }
                        Err(_) => {
                            ping_sender
                                .send((server.to_string(), PingInfo::Unreachable))
                                .unwrap();
                        }
                    });
            }
        });

        ui.ping_receiver = Some(ping_receiver);

        return ui;
    }

    fn title(&self) -> String {
        return String::from("Steam Server Toggle");
    }

    fn update(&mut self, message: Message) {
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
        }
    }

    fn view(&mut self) -> Element<Message> {
        // in case there were ping measurements done by the other thread, retrieve them here
        let ping_receiver = self.ping_receiver.as_ref().unwrap();
        while let Ok((server, info)) = ping_receiver.try_recv() {
            self.buttons
                .iter_mut()
                .filter(|button| button.abr == server)
                .for_each(|button| {
                    button.ping = info;
                });
        }

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
            row = row.push(
                Text::new(format!("{}", server.ping))
                    .size(20)
                    .width(Length::Units(180)),
            );
            content = content.push(row);
        }
        content.into()
    }
}
