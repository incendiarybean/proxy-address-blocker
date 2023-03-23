use std::{net::SocketAddr, sync::mpsc::Sender, thread};

use eframe::{
    egui::{self, CentralPanel, Label, Margin, RichText, TextEdit},
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
    // request_event_sender: Sender<RequestEvent>,
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
            let current_proxy_state = match properties.proxy_status.lock() {
                Ok(proxy_event) => proxy_event,
                Err(poisoned) => poisoned.into_inner(),
            };

            match *current_proxy_state {
                ProxyEvent::Error => {
                    properties.port_error = "Please check the port is available.".to_string();
                    properties.start_server_capable = true;
                }
                _ => (),
            };

            let label = Label::new("Enter a Port to run on:");
            ui.add(label);
            ui.add_space(2.0);

            let input = TextEdit::singleline(&mut properties.port).hint_text("Port, e.g. 8000");
            let input_response = ui.add(input);

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
                    properties.port_error = "Port contains invalid characters.".to_string();
                    properties.start_server_capable = false;
                    return;
                } else {
                    properties.start_server_capable = true;
                    properties.port_error = String::default();
                }
            }

            if !properties.port_error.is_empty() {
                // properties.start_server_capable = false;
                ui.add_space(3.0);
                ui.label(RichText::new(&properties.port_error).color(Color32::LIGHT_RED));
            }

            ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                ui.with_layout(egui::Layout::left_to_right(egui::Align::BOTTOM), |ui| {
                    match *current_proxy_state {
                        ProxyEvent::Running => {
                            let stop_button = egui::Button::new("Stop Proxy").min_size(Vec2 {
                                x: ui.available_width() / 2.,
                                y: 18.,
                            });
                            let stop_button_response =
                                ui.add_enabled(properties.start_server_capable, stop_button);

                            if stop_button_response.clicked() {
                                proxy_event_sender.send(ProxyEvent::Terminating).unwrap();
                            }
                        }
                        _ => {
                            let start_button = egui::Button::new(match *current_proxy_state {
                                ProxyEvent::Error => "Retry Proxy",
                                _ => "Start Proxy",
                            })
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
                                        // request_event_sender,
                                    )
                                });
                            }
                        }
                    }

                    let logs_button = egui::Button::new("View Logs").min_size(Vec2 {
                        x: ui.available_width(),
                        y: 18.,
                    });
                    ui.add_enabled(false, logs_button);
                });

                ui.with_layout(egui::Layout::left_to_right(egui::Align::BOTTOM), |ui| {
                    ui.add(egui::Label::new("Process is currently:"));
                    ui.add(egui::Label::new(
                        RichText::new(format!("{:?}", current_proxy_state)).color(
                            match *current_proxy_state {
                                ProxyEvent::Running => Color32::LIGHT_GREEN,
                                _ => Color32::LIGHT_RED,
                            },
                        ),
                    ));
                });

                // ui.with_layout(egui::Layout::left_to_right(egui::Align::BOTTOM), |ui| {

                // });
            });
        });
}
