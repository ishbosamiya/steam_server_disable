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
    records: Mutex<VecDeque<String>>,
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

                records.iter().for_each(|record| {
                    ui.label(record);
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
            records.push_front(format!(
                "{} - {}:{} - {}",
                record.level(),
                record.file().map_or("None", |file| file),
                record
                    .line()
                    .map_or("None".to_string(), |line| line.to_string()),
                record.args()
            ));

            if records.len() > max_number_of_records {
                records.truncate(max_number_of_records);
            }
        }
    }

    fn flush(&self) {}
}
