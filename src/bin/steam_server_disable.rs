use std::sync::mpsc;

use clap::Parser;
use egui_glfw::{
    egui::{self, FontDefinitions, FontFamily, TextStyle},
    EguiBackend,
};
use glfw::{self, Context};
use nalgebra_glm as glm;
use steam_server_disable::{app::App, logger};

/// Command line arguments for the `steam_server_disable`.
#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
pub struct CommandLineArguments {
    /// No GUI.
    #[arg(long, default_value_t)]
    pub no_gui: bool,

    /// Enable all the IPs of the server regions matching the given
    /// regex.
    #[arg(long)]
    pub enable: Option<String>,
}

fn main() {
    #[cfg(unix)]
    {
        sudo::escalate_if_needed().unwrap();
    }
    // TODO: need to find something to auto escalate to sudo on
    // windows

    let is_running_as_sudo = {
        #[cfg(unix)]
        {
            matches!(sudo::check(), sudo::RunningAs::Root)
        }
        #[cfg(windows)]
        {
            is_elevated::is_elevated()
        }
    };

    logger::init().unwrap();

    if !is_running_as_sudo {
        log::error!("Not running as sudo/administrator. Rerun application as sudo/admin.");
    }

    let command_line_arguments = CommandLineArguments::parse();

    log::info!("command_line_arguments: {:#?}", command_line_arguments);

    let mut app = App::new();

    if let Some(enable) = &command_line_arguments.enable {
        let enable = regex::Regex::new(enable).expect("Invalid `--enable` regex");

        app.enable_matching(&enable);
    }

    if command_line_arguments.no_gui {
        return;
    }

    log::info!("starting GUI");

    let mut glfw = glfw::init(glfw::FAIL_ON_ERRORS).unwrap();

    // set to opengl 3.3 or higher
    glfw.window_hint(glfw::WindowHint::ContextVersion(3, 3));
    glfw.window_hint(glfw::WindowHint::OpenGlProfile(
        glfw::OpenGlProfileHint::Core,
    ));
    // if msaa is available, use it
    glfw.window_hint(glfw::WindowHint::Samples(Some(16)));
    glfw.window_hint(glfw::WindowHint::ScaleToMonitor(true));
    #[cfg(target_os = "macos")]
    glfw.window_hint(glfw::WindowHint::OpenGlForwardCompat(true));
    let (mut window, events) = glfw
        .create_window(
            1280,
            720,
            "Steam Server Disable",
            glfw::WindowMode::Windowed,
        )
        .expect("Failed to create glfw window");

    // setup bunch of polling data
    window.set_key_polling(true);
    window.set_cursor_pos_polling(true);
    window.set_mouse_button_polling(true);
    window.set_framebuffer_size_polling(true);
    window.set_scroll_polling(true);
    window.set_char_polling(true);
    window.make_current();

    // load opengl symbols
    gl::load_with(|symbol| window.get_proc_address(symbol));

    // enable vsync
    glfw.set_swap_interval(glfw::SwapInterval::Sync(1));

    // enable and disable certain opengl features
    unsafe {
        gl::Disable(gl::CULL_FACE);
        gl::Enable(gl::DEPTH_TEST);
        gl::Enable(gl::MULTISAMPLE);
        gl::Enable(gl::FRAMEBUFFER_SRGB);
    }

    let mut egui = EguiBackend::new(&mut window, &mut glfw);

    let mut fonts = FontDefinitions::default();
    // larger text
    fonts
        .family_and_size
        .insert(TextStyle::Button, (FontFamily::Proportional, 18.0));
    fonts
        .family_and_size
        .insert(TextStyle::Body, (FontFamily::Proportional, 18.0));
    fonts
        .family_and_size
        .insert(TextStyle::Small, (FontFamily::Proportional, 15.0));
    egui.get_egui_ctx().set_fonts(fonts);

    unsafe {
        gl::ClearColor(0.2, 0.2, 0.2, 1.0);
    }

    if !is_running_as_sudo {
        non_sudo_gui(glfw, window, events, egui);

        return;
    }

    let mut open_logging_window = false;

    while !window.should_close() {
        glfw.poll_events();

        glfw::flush_messages(&events).for_each(|(_, event)| {
            egui.handle_event(&event, &window);
            handle_window_events(&event, &mut open_logging_window);
        });

        unsafe {
            gl::Clear(gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT);
        }

        app.update();

        egui.begin_frame(&window, &mut glfw);

        egui::CentralPanel::default().show(egui.get_egui_ctx(), |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    app.draw_ui(ui);
                });
        });

        logger::LOGGER
            .first()
            .draw_ui(egui.get_egui_ctx(), &mut open_logging_window);

        let (width, height) = window.get_framebuffer_size();
        let _output = egui.end_frame(glm::vec2(width as _, height as _));

        window.swap_buffers();
    }
}

fn handle_window_events(event: &glfw::WindowEvent, open_logging_window: &mut bool) {
    #[allow(clippy::single_match)]
    match event {
        glfw::WindowEvent::Key(glfw::Key::GraveAccent, _, glfw::Action::Press, modifiers) => {
            if modifiers.is_empty() {
                *open_logging_window = !*open_logging_window;
            }
        }
        _ => {}
    }
}

fn non_sudo_gui(
    mut glfw: glfw::Glfw,
    mut window: glfw::Window,
    events: mpsc::Receiver<(f64, glfw::WindowEvent)>,
    mut egui: egui_glfw::EguiBackend,
) {
    while !window.should_close() {
        glfw.poll_events();

        glfw::flush_messages(&events).for_each(|(_, event)| {
            egui.handle_event(&event, &window);
        });

        unsafe {
            gl::Clear(gl::COLOR_BUFFER_BIT | gl::DEPTH_BUFFER_BIT);
        }

        egui.begin_frame(&window, &mut glfw);

        logger::LOGGER
            .first()
            .draw_ui(egui.get_egui_ctx(), &mut true);

        let (width, height) = window.get_framebuffer_size();
        let _output = egui.end_frame(glm::vec2(width as _, height as _));

        window.swap_buffers();
    }
}
