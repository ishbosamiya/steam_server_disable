use std::{collections::VecDeque, sync::Mutex};

use egui_glfw::egui;
use lazy_static::lazy_static;
use log::{Level, LevelFilter, Log, SetLoggerError};

lazy_static! {
    static ref LOGGER: EguiLogger = EguiLogger {
        records: Mutex::new(VecDeque::new()),
        previous_ui_sizes: Mutex::new(None),
    };
}

pub struct EguiLogger {
    records: Mutex<VecDeque<Record>>,
    previous_ui_sizes: Mutex<Option<UiSizes>>,
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
                        let ui_sizes = records.iter().fold(UiSizes::zero(), |acc, record| {
                            let ui_sizes =
                                record.draw_ui(ui, self.previous_ui_sizes.lock().unwrap().as_ref());
                            ui.end_row();

                            acc.max(&ui_sizes)
                        });

                        *self.previous_ui_sizes.lock().unwrap() = Some(ui_sizes);
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

    pub fn draw_ui(&self, ui: &mut egui::Ui, previous_sizes: Option<&UiSizes>) -> UiSizes {
        ui.horizontal(|ui| {
            let color = match self.level {
                Level::Error => Some(egui::Color32::RED),
                Level::Warn => Some(egui::Color32::YELLOW),
                Level::Info => Some(egui::Color32::LIGHT_BLUE),
                Level::Debug => Some(egui::Color32::from_rgb(78, 39, 138)),
                Level::Trace => None,
            };

            let level_size = ui
                .scope(|ui| {
                    if let Some(previous_sizes) = previous_sizes {
                        ui.set_min_size(previous_sizes.level);
                    }

                    if let Some(color) = color {
                        ui.colored_label(color, self.level.as_str());
                    } else {
                        ui.label(self.level.as_str());
                    };
                })
                .response
                .rect
                .size();

            let file_line_size = ui
                .scope(|ui| {
                    if let Some(previous_sizes) = previous_sizes {
                        ui.set_min_size(previous_sizes.file_line);
                    }

                    if let Some(file) = &self.file {
                        if let Some(line) = &self.line {
                            ui.label(format!("{}:{}", file, line));
                        }
                    }
                })
                .response
                .rect
                .size();

            let args_size = ui
                .scope(|ui| {
                    if let Some(previous_sizes) = previous_sizes {
                        ui.set_min_size(previous_sizes.args);
                    }

                    ui.label(&self.args);
                })
                .response
                .rect
                .size();

            UiSizes::new(level_size, file_line_size, args_size)
        })
        .inner
    }
}

#[derive(Debug)]
struct UiSizes {
    level: egui::Vec2,
    file_line: egui::Vec2,
    args: egui::Vec2,
}

impl UiSizes {
    pub fn new(level: egui::Vec2, file_line: egui::Vec2, args: egui::Vec2) -> Self {
        Self {
            level,
            file_line,
            args,
        }
    }

    pub fn zero() -> Self {
        Self::new(egui::Vec2::ZERO, egui::Vec2::ZERO, egui::Vec2::ZERO)
    }

    pub fn max(&self, other: &UiSizes) -> Self {
        Self::new(
            self.level.max(other.level),
            self.file_line.max(other.file_line),
            self.args.max(other.args),
        )
    }
}
