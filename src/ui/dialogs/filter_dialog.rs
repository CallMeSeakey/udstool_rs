use eframe::egui::{self, RichText, Window};
use crate::i18n::t;

pub struct CustomFilterDialog {
    pub open: bool,
    pub filter_std: bool,
    pub filter_ext: bool,
    pub filter_ids_text: String,
}

impl Default for CustomFilterDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl CustomFilterDialog {
    pub fn new() -> Self {
        Self {
            open: false,
            filter_std: true,
            filter_ext: true,
            filter_ids_text: String::new(),
        }
    }

    pub fn show(&mut self, ctx: &egui::Context, active_ids: &mut std::collections::HashSet<u32>) {
        if !self.open {
            return;
        }

        let mut open_state = self.open;
        Window::new(t("filter_settings_btn"))
            .open(&mut open_state)
            .resizable(false)
            .collapsible(false)
            .fixed_size([450.0, 320.0])
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.group(|ui| {
                    ui.label(RichText::new("Frame Type").strong());
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut self.filter_std, "Standard (11bit)");
                        ui.checkbox(&mut self.filter_ext, "Extended (29bit)");
                    });
                });

                ui.add_space(8.0);
                ui.label("ID List (Hex 0x... or decimal, comma-separated):");
                ui.text_edit_multiline(&mut self.filter_ids_text);

                ui.add_space(10.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(t("dlg_confirm")).clicked() {
                        active_ids.clear();
                        for part in self.filter_ids_text.split(',') {
                            let s = part.trim();
                            if !s.is_empty() {
                                if let Ok(id) = if s.to_lowercase().starts_with("0x") {
                                    u32::from_str_radix(&s[2..], 16)
                                } else {
                                    s.parse::<u32>()
                                } {
                                    active_ids.insert(id);
                                }
                            }
                        }
                        self.open = false;
                    }
                    if ui.button(t("pm_close_btn")).clicked() {
                        self.open = false;
                    }
                });
            });

        self.open = open_state;
    }
}
