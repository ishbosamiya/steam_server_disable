use iced::{button, Button, Column, Element, Sandbox, Text};

#[derive(Default)]
pub struct UI {
    val: u32,
    button: button::State,
}

#[derive(Debug, Clone, Copy)]
pub enum Message {
    ButtonPress,
}

impl Sandbox for UI {
    type Message = Message;

    fn new() -> Self {
        return Self::default();
    }

    fn title(&self) -> String {
        return String::from("Steam Server Toggle");
    }

    fn update(&mut self, message: Message) {
        match message {
            Message::ButtonPress => {
                self.val += 1;
            }
        }
    }

    fn view(&mut self) -> Element<Message> {
        Column::new()
            .padding(20)
            .push(Button::new(&mut self.button, Text::new("Button")).on_press(Message::ButtonPress))
            .push(Text::new(self.val.to_string()).size(72))
            .into()
    }
}
