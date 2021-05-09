use iced::{button, Button, Column, Element, Sandbox, Text};

use crate::ServerObject;

struct IPTables(iptables::IPTables);

#[derive(Default)]
pub struct UI {
    server_obj: ServerObject,
    ipt: IPTables,
    buttons: Vec<(String, (button::State, button::State))>,
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
}

impl Sandbox for UI {
    type Message = Message;

    fn new() -> Self {
        let mut ui = Self::default();
        let server_list = ui.server_obj.get_server_list();
        let server_list: Vec<String> = server_list
            .iter()
            .map(|server| server.to_string())
            .collect();
        server_list.iter().for_each(|server| {
            ui.buttons.push((
                server.to_string(),
                (button::State::new(), button::State::new()),
            ))
        });
        return ui;
    }

    fn title(&self) -> String {
        return String::from("Steam Server Toggle");
    }

    fn update(&mut self, message: Message) {
        match message {
            Message::EnableServer(server) => {
                self.server_obj.unban_server(&self.ipt.0, &server).unwrap();
            }
            Message::DisableServer(server) => {
                self.server_obj.ban_server(&self.ipt.0, &server).unwrap();
            }
        }
    }

    fn view(&mut self) -> Element<Message> {
        let mut column = Column::new().padding(20);
        for (server, (enable_button, disable_button)) in &mut self.buttons {
            column = column.push(Text::new(server.to_string()).size(20));
            column = column.push(
                Button::new(enable_button, Text::new("Enable"))
                    .on_press(Message::EnableServer(server.to_string())),
            );
            column = column.push(
                Button::new(disable_button, Text::new("Disable"))
                    .on_press(Message::DisableServer(server.to_string())),
            );
        }
        column.into()
    }
}
