use crate::{egui, steam_server::ServerObject};

pub struct UI {
    server_object: ServerObject,
    ipt: iptables::IPTables,
}

impl UI {
    pub fn new() -> Self {
        Self {
            server_object: ServerObject::new(),
            ipt: iptables::new(false).unwrap(),
        }
    }

    pub fn draw_ui(&self, ui: &mut egui::Ui) {
        ui.label(format!("{}", ui.available_width()));
        let num_columns = 4;
        egui::Grid::new("ui_grid")
            .min_col_width(ui.available_width() / num_columns as f32)
            .num_columns(num_columns)
            .striped(true)
            .show(ui, |ui| {
                ui.columns(num_columns, |columns| {
                    columns[0].label("Region");
                    columns[1].label("State");
                    if columns[2].button("Enable All").clicked() {
                        self.server_object
                            .get_server_list()
                            .iter()
                            .for_each(|server| {
                                self.server_object.unban_server(&self.ipt, server).unwrap();
                            });
                    }
                    if columns[3].button("Disable All").clicked() {
                        self.server_object
                            .get_server_list()
                            .iter()
                            .for_each(|server| {
                                self.server_object.ban_server(&self.ipt, server).unwrap();
                            });
                    }
                });
                ui.end_row();

                // iterate over all the servers in the server object
                self.server_object
                    .get_server_list()
                    .iter()
                    .for_each(|server| {
                        ui.columns(num_columns, |columns| {
                            columns[0].label(server.as_str());

                            columns[1].label(
                                self.server_object
                                    .get_server_state(&self.ipt, server)
                                    .map(|state| state.to_string())
                                    .unwrap_or_else(|_| "State Unknown".to_string()),
                            );

                            if columns[2].button("Enable").clicked() {
                                self.server_object.unban_server(&self.ipt, server).unwrap();
                            }

                            if columns[3].button("Disable").clicked() {
                                self.server_object.ban_server(&self.ipt, server).unwrap();
                            }
                        });

                        ui.end_row();
                    });
            });
    }
}

impl Default for UI {
    fn default() -> Self {
        Self::new()
    }
}
