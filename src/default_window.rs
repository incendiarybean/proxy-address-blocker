use eframe::{
    egui::{self, CentralPanel},
    epaint::{Color32, Stroke, Vec2},
};

use crate::{main_body, proxy::Proxy, task_bar};

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub struct MainWindow {
    pub close_button_tint: Color32,
    pub minimise_button_tint: Color32,
    pub maximise_button_tint: Color32,

    // Handle all Proxy Details
    pub proxy: Proxy,
}

impl Default for MainWindow {
    fn default() -> Self {
        let proxy = Proxy::default();

        Self {
            close_button_tint: Color32::WHITE,
            minimise_button_tint: Color32::WHITE,
            maximise_button_tint: Color32::WHITE,

            proxy,
        }
    }
}

impl MainWindow {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        if let Some(storage) = cc.storage {
            // We can manipulate Proxy here, might be worth setting some default values
            // Maybe a custom impl function to overwrite some items
            // Mutex doesn't like being copied over
            return eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default();
        }

        Default::default()
    }
}

impl eframe::App for MainWindow {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        egui::Rgba::TRANSPARENT.to_array()
    }

    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        if self.proxy.logs && !frame.info().window_info.maximized {
            frame.set_window_size(Vec2 { x: 650.0, y: 500.0 });
        } else if !self.proxy.logs && !frame.info().window_info.maximized {
            frame.set_window_size(Vec2 { x: 250.0, y: 160.0 });
        }

        let panel_frame = egui::Frame {
            fill: ctx.style().visuals.window_fill(),
            rounding: 7.0.into(),
            stroke: Stroke::new(1.0, Color32::LIGHT_GRAY),
            outer_margin: 0.1.into(),
            ..Default::default()
        };

        CentralPanel::default().frame(panel_frame).show(ctx, |ui| {
            ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                task_bar::task_bar(self, ui, frame);
                main_body::main_body(&mut self.proxy, ui);
            });
        });
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }

    fn persist_native_window(&self) -> bool {
        true
    }
}
