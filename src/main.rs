#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use eframe::egui;

mod default_window;
mod main_body;
mod proxy_handler;
mod task_bar;

fn main() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        decorated: false,
        transparent: true,
        min_window_size: Some(egui::vec2(300.0, 300.0)),
        initial_window_size: Some(egui::vec2(600.0, 600.0)),
        resizable: true,
        follow_system_theme: false,
        ..Default::default()
    };
    eframe::run_native(
        "Proxy Blocker",
        options.clone(),
        Box::new(|_cc| Box::new(default_window::MainWindow::default())),
    )
}
