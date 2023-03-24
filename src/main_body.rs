use std::{net::SocketAddr, sync::mpsc::Sender, thread};

use eframe::{
    egui::{self, CentralPanel, Margin, RichText, TextEdit},
    epaint::{Color32, Vec2},
};

use crate::{
    default_window::{MainWindow, ProxyEvent},
    proxy_handler::proxy_service,
};

pub fn main_body(
    properties: &mut MainWindow,
    ui: &mut egui::Ui,
    proxy_event_sender: Sender<ProxyEvent>,
) {
    let panel_frame = egui::Frame {
        fill: ui.ctx().style().visuals.window_fill(),
        outer_margin: Margin {
            left: 5.0.into(),
            right: 5.0.into(),
            top: 27.0.into(),
            bottom: 5.0.into(),
        },
        inner_margin: 5.0.into(),
        ..Default::default()
    };

    CentralPanel::default()
        .frame(panel_frame)
        .show(ui.ctx(), |ui| {
            let mut open = true;
            egui::Window::new("Proxy Settings")
                .open(&mut open)
                .min_height(100.0)
                .min_width(200.0)
                .show(ui.ctx(), |ui| {
                    let proxy_state = match properties.proxy_status.lock() {
                        Ok(proxy_event) => proxy_event,
                        Err(poisoned) => poisoned.into_inner(),
                    };

                    let current_proxy_status = match *proxy_state {
                        ProxyEvent::Running => "RUNNING",
                        ProxyEvent::Stopped => "STOPPED",
                        ProxyEvent::Error => "ERROR",
                        ProxyEvent::Terminating => "TERMINATING",
                        ProxyEvent::Terminated => "TERMINATED",
                    };

                    if current_proxy_status == "ERROR" {
                        properties.port_error = "Please check the port is available.".to_string();
                        properties.start_server_capable = true;
                    }

                    if current_proxy_status == "STOPPED" {
                        ui.label(RichText::new("Enter a Port to run on:").size(13.0));
                        ui.add_space(2.0);

                        let input =
                            TextEdit::singleline(&mut properties.port).hint_text("Port, e.g. 8000");
                        let input_response = ui.add(
                            input
                                .min_size(Vec2 {
                                    x: ui.available_width(),
                                    y: 20.0,
                                })
                                .vertical_align(eframe::emath::Align::Center),
                        );

                        if input_response.changed() {
                            // TODO: Something about this mess, there is definitely a nicer way
                            if properties.port.char_indices().count() < 2 {
                                properties.port_error = "Port too short!".to_string();
                                return;
                            } else {
                                properties.start_server_capable = true;
                                properties.port_error = String::default();
                            }

                            if properties.port.char_indices().count() > 5 {
                                properties.port_error = "Port too long!".to_string();
                                return;
                            } else {
                                properties.start_server_capable = true;
                                properties.port_error = String::default();
                            }

                            if let Err(_) = properties.port.trim().parse::<u32>() {
                                properties.port_error =
                                    "Port contains invalid characters.".to_string();
                                properties.start_server_capable = false;
                                return;
                            } else {
                                properties.start_server_capable = true;
                                properties.port_error = String::default();
                            }
                        }

                        if !properties.port_error.is_empty() {
                            ui.add_space(3.0);
                            ui.label(
                                RichText::new(&properties.port_error)
                                    .size(11.0)
                                    .color(Color32::LIGHT_RED),
                            );
                        }
                    }

                    ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::BOTTOM), |ui| {
                            if current_proxy_status == "RUNNING" {
                                let stop_button = egui::Button::new("Stop Proxy").min_size(Vec2 {
                                    x: ui.available_width() / 2.,
                                    y: 18.,
                                });
                                let stop_button_response =
                                    ui.add_enabled(properties.start_server_capable, stop_button);

                                if stop_button_response.clicked() {
                                    proxy_event_sender.send(ProxyEvent::Terminating).unwrap();
                                }
                            } else {
                                let start_button = egui::Button::new(
                                    RichText::new(match current_proxy_status {
                                        "RUNNING" => "Retry Proxy",
                                        _ => "Start Proxy",
                                    })
                                    .size(13.0),
                                )
                                .min_size(Vec2 {
                                    x: ui.available_width() / 2.,
                                    y: 18.,
                                });
                                let start_button_response =
                                    ui.add_enabled(properties.start_server_capable, start_button);

                                if start_button_response.clicked() {
                                    let port_copy =
                                        properties.port.trim().parse::<u16>().unwrap().clone();
                                    let proxy_status = properties.proxy_status.clone();

                                    // Create a thread and assign the server to it
                                    // This stops the UI from freezing
                                    thread::spawn(move || {
                                        proxy_service(
                                            SocketAddr::from(([127, 0, 0, 1], port_copy)),
                                            proxy_event_sender,
                                            proxy_status,
                                        )
                                    });
                                }
                            }

                            let logs_button = egui::Button::new(
                                RichText::new("View Logs").size(13.0),
                            )
                            .min_size(Vec2 {
                                x: ui.available_width(),
                                y: 18.,
                            });
                            let logs_button_response =
                                ui.add_enabled(current_proxy_status == "RUNNING", logs_button);
                            if logs_button_response.clicked() {
                                //do send
                            }
                        });

                        ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                            if current_proxy_status == "RUNNING" {
                                ui.with_layout(
                                    egui::Layout::left_to_right(egui::Align::TOP),
                                    |ui| {
                                        ui.add(egui::Label::new("Hosting on: "));
                                        ui.add(egui::Label::new(
                                            RichText::new(format!("127.0.0.1:{}", properties.port))
                                                .color(Color32::LIGHT_GREEN),
                                        ));
                                    },
                                );
                            }

                            ui.with_layout(
                                egui::Layout::left_to_right(egui::Align::BOTTOM),
                                |ui| {
                                    ui.add(egui::Label::new("Process is currently:"));
                                    ui.add(egui::Label::new(
                                        RichText::new(format!("{}", current_proxy_status)).color(
                                            match current_proxy_status {
                                                "RUNNING" => Color32::LIGHT_GREEN,
                                                _ => Color32::LIGHT_RED,
                                            },
                                        ),
                                    ));
                                },
                            );
                        });
                    });
                });
        });
    ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
        let panel_frame = egui::Frame {
            fill: ui.ctx().style().visuals.window_fill(),
            outer_margin: Margin {
                left: 5.0.into(),
                right: 5.0.into(),
                top: 27.0.into(),
                bottom: 5.0.into(),
            },
            inner_margin: 5.0.into(),
            ..Default::default()
        };

        CentralPanel::default()
            .frame(panel_frame)
            .show(ui.ctx(), |ui| {
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Min), |ui| {
                    let btn = egui::Button::new("new").min_size(Vec2 {
                        x: ui.available_width() / 3.0,
                        y: 20.,
                    });
                    ui.add(btn);

                    let btn2 = egui::Button::new("new").min_size(Vec2 {
                        x: ui.available_width() / 2.0,
                        y: 20.,
                    });
                    ui.add(btn2);

                    let btn3 = egui::Button::new("new").min_size(Vec2 {
                        x: ui.available_width(),
                        y: 20.,
                    });
                    ui.add(btn3);
                });
            });
    });
}
