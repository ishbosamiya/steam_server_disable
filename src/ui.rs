use iced::{button, scrollable, Button, Element, Length, Row, Sandbox, Scrollable, Text};

use crate::steam_server::{ServerObject, ServerState};

struct IPTables(iptables::IPTables);

#[derive(Default)]
pub struct UI {
    scroll: scrollable::State,
    server_obj: ServerObject,
    ipt: IPTables,
    buttons: Vec<Server>,
    download_button: button::State,
}

struct Server {
    abr: String,
    enable_button: button::State,
    disable_button: button::State,
    state: ServerState,
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
    DownloadFile,
}

impl Sandbox for UI {
    type Message = Message;

    fn new() -> Self {
        let mut ui = Self::default();
        let server_list = ui.server_obj.get_server_list();
        let server_list: Vec<String> = server_list
            .iter()
            .filter(|server| {
                if let Ok(_) = ui.server_obj.get_server_ips(server) {
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
                ui.server_obj
                    .get_server_state(&ui.ipt.0, server)
                    .expect("couldnt get state of some server"),
            ))
        });
        return ui;
    }

    fn title(&self) -> String {
        return String::from("Steam Server Toggle");
    }

    fn update(&mut self, message: Message) {
        match message {
            Message::EnableServer(server_abr) => {
                self.server_obj
                    .unban_server(&self.ipt.0, &server_abr)
                    .unwrap();
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
                self.server_obj
                    .ban_server(&self.ipt.0, &server_abr)
                    .unwrap();
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
            Message::DownloadFile => {
                ServerObject::download_file()
                    .expect("couldn't download file, todo: make it not panic");
            }
        }
    }

    fn view(&mut self) -> Element<Message> {
        let mut content = Scrollable::new(&mut self.scroll)
            .width(Length::Fill)
            .spacing(10);
        content = content.push(
            Button::new(&mut self.download_button, Text::new("Download file"))
                .on_press(Message::DownloadFile),
        );
        for server in &mut self.buttons {
            let mut row = Row::new().spacing(10);
            row = row.push(Text::new(server.abr.clone()).size(20));
            row = row.push(
                Button::new(&mut server.enable_button, Text::new("Enable"))
                    .on_press(Message::EnableServer(server.abr.clone())),
            );
            row = row.push(
                Button::new(&mut server.disable_button, Text::new("Disable"))
                    .on_press(Message::DisableServer(server.abr.clone())),
            );
            row = row.push(Text::new(format!("{}", server.state)).size(20));
            content = content.push(row);
        }
        content.into()
    }
}
