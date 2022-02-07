use std::sync::Mutex;

use crate::{
    egui,
    steam_server::{self, Error, ServerObject, ServerState},
};

pub struct UI {
    servers: Servers,
    ipt: iptables::IPTables,
}

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

impl UI {
    pub fn new() -> Self {
        Self {
            servers: ServerObject::new().into(),
            ipt: iptables::new(false).unwrap(),
        }
    }

    pub fn draw_ui(&mut self, ui: &mut egui::Ui) {
        if ui.button("Download Server List").clicked() {
            ServerObject::download_file().unwrap();
            self.servers = ServerObject::new().into();
        }

        let num_columns = 4;
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
