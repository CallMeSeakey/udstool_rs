// src/ui/dialogs/about.rs
use crate::config::{APP_AUTHOR, APP_COMPANY, APP_EMAIL, APP_VERSION};
use crate::i18n::t;
use eframe::egui::{self, Align2, Color32, Margin, RichText, Stroke, Window};
use std::sync::OnceLock;

static ABOUT_ICON_CACHE: OnceLock<egui::ColorImage> = OnceLock::new();

fn get_about_icon() -> &'static egui::ColorImage {
    ABOUT_ICON_CACHE.get_or_init(|| {
        let png_bytes = include_bytes!("../../../udstools.png");
        if let Ok(img) = image::load_from_memory(png_bytes) {
            let resized = img.resize(192, 192, image::imageops::FilterType::Lanczos3);
            let rgba = resized.to_rgba8();
            let (w, h) = rgba.dimensions();
            egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba)
        } else {
            egui::ColorImage::example()
        }
    })
}

pub struct AboutDialog {
    pub open: bool,
    texture: Option<egui::TextureHandle>,
}

impl Default for AboutDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl AboutDialog {
    pub fn new() -> Self {
        Self {
            open: false,
            texture: None,
        }
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        if !self.open {
            return;
        }

        if self.texture.is_none() {
            let color_image = get_about_icon().clone();
            self.texture = Some(ctx.load_texture(
                "about_logo_retina",
                color_image,
                egui::TextureOptions::LINEAR,
            ));
        }

        let mut close_requested = false;

        Window::new(t("about_title"))
            .open(&mut self.open)
            .resizable(false)
            .collapsible(false)
            .pivot(Align2::CENTER_CENTER)
            .default_pos(ctx.screen_rect().center())
            .default_size([490.0, 0.0])
            .show(ctx, |ui| {
                ui.set_width(490.0);

                ui.vertical(|ui| {
                    ui.add_space(6.0);

                    ui.horizontal_top(|ui| {
                        // 放大至 96x96，使三个芯片和内部纹理完全展开清晰可见
                        egui::Frame::none()
                            .fill(if ui.visuals().dark_mode {
                                Color32::from_rgb(18, 22, 28)
                            } else {
                                Color32::from_rgb(240, 243, 246)
                            })
                            .stroke(Stroke::new(1.0_f32, ui.visuals().widgets.noninteractive.bg_stroke.color))
                            .rounding(10.0)
                            .inner_margin(Margin::same(4.0))
                            .show(ui, |ui| {
                                if let Some(tex) = &self.texture {
                                    ui.add(
                                        egui::Image::from_texture(tex)
                                            .fit_to_exact_size(egui::vec2(96.0, 96.0))
                                            .rounding(8.0),
                                    );
                                }
                            });

                        ui.add_space(16.0);

                        ui.vertical(|ui| {
                            ui.label(
                                RichText::new(t("about_header"))
                                    .size(17.5)
                                    .strong()
                                    .color(Color32::from_rgb(0, 188, 212)),
                            );

                            ui.add_space(8.0);

                            egui::Grid::new("about_info_grid_v6")
                                .num_columns(2)
                                .spacing([12.0, 6.0])
                                .show(ui, |ui| {
                                    ui.label(
                                        RichText::new(format!("{}:", t("about_version")))
                                            .strong()
                                            .color(ui.visuals().text_color()),
                                    );
                                    ui.label(
                                        RichText::new(format!("V{}", APP_VERSION))
                                            .strong()
                                            .color(ui.visuals().strong_text_color()),
                                    );
                                    ui.end_row();

                                    ui.label(
                                        RichText::new(format!("{}:", t("about_team")))
                                            .strong()
                                            .color(ui.visuals().text_color()),
                                    );
                                    ui.label(APP_COMPANY);
                                    ui.end_row();

                                    ui.label(
                                        RichText::new(format!("{}:", t("about_author")))
                                            .strong()
                                            .color(ui.visuals().text_color()),
                                    );
                                    ui.label(APP_AUTHOR);
                                    ui.end_row();

                                    ui.label(
                                        RichText::new(format!("{}:", t("about_email")))
                                            .strong()
                                            .color(ui.visuals().text_color()),
                                    );
                                    ui.hyperlink_to(APP_EMAIL, format!("mailto:{}", APP_EMAIL));
                                    ui.end_row();
                                });
                        });
                    });

                    ui.add_space(14.0);
                    ui.separator();
                    ui.add_space(6.0);

                    ui.label(
                        RichText::new(t("about_desc"))
                            .size(11.5)
                            .color(ui.visuals().weak_text_color()),
                    );

                    ui.add_space(12.0);

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let btn = egui::Button::new(
                            RichText::new(t("dlg_confirm"))
                                .strong()
                                .color(Color32::WHITE),
                        )
                        .fill(Color32::from_rgb(0, 122, 204))
                        .rounding(4.0);

                        if ui.add_sized([80.0, 26.0], btn).clicked() {
                            close_requested = true;
                        }
                    });
                });
            });

        if close_requested {
            self.open = false;
        }
    }
}