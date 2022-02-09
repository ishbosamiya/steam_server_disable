use std::{collections::VecDeque, sync::Mutex};

use egui_glfw::egui;
use lazy_static::lazy_static;
use log::{Level, LevelFilter, Log, SetLoggerError};

lazy_static! {
    static ref LOGGER: EguiLogger = EguiLogger {
        records: Mutex::new(VecDeque::new()),
    };
}

pub struct EguiLogger {
    records: Mutex<VecDeque<Record>>,
}

pub fn init() -> Result<(), SetLoggerError> {
    log::set_logger(get_logger()).map(|()| log::set_max_level(LevelFilter::Trace))
}

pub fn get_logger() -> &'static EguiLogger {
    &LOGGER
}

impl EguiLogger {
    pub fn draw_ui(&self, ctx: &egui::CtxRef) {
        egui::Window::new("Logging Window")
            .scroll2([true, true])
            .show(ctx, |ui| {
                let records = self.records.lock().unwrap();

                egui::Grid::new("logging window grid")
                    .striped(true)
                    .show(ui, |ui| {
                        records.iter().for_each(|record| {
                            record.draw_ui(ui);
                            ui.end_row();
                        });
                    });
            });
    }
}

impl Log for EguiLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= Level::Trace
    }

    fn log(&self, record: &log::Record) {
        let max_number_of_records = 10000;

        if self.enabled(record.metadata()) {
            let mut records = self.records.lock().unwrap();
            records.push_front(Record::new(record));

            if records.len() > max_number_of_records {
                records.truncate(max_number_of_records);
            }
        }
    }

    fn flush(&self) {}
}

struct Record {
    level: log::Level,
    file: Option<String>,
    line: Option<u32>,
    args: String,
}

impl Record {
    pub fn new(record: &log::Record) -> Self {
        Self {
            level: record.level(),
            file: record.file().map(|string| string.to_string()),
            line: record.line(),
            args: record.args().to_string(),
        }
    }

    pub fn draw_ui(&self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let color = match self.level {
                Level::Error => Some(egui::Color32::RED),
                Level::Warn => Some(egui::Color32::YELLOW),
                Level::Info => Some(egui::Color32::LIGHT_BLUE),
                Level::Debug => Some(egui::Color32::from_rgb(78, 39, 138)),
                Level::Trace => None,
            };
            if let Some(color) = color {
                ui.colored_label(color, self.level.as_str());
            } else {
                ui.label(self.level.as_str());
            }

            if let Some(file) = &self.file {
                if let Some(line) = &self.line {
                    ui.label(format!("{}:{}", file, line));
                }
            }

            ui.label(&self.args);
        });
    }
}
