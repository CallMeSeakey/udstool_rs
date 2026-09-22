use eframe::egui::{self, Align2, Color32, RichText, Window};
use crate::config::AppConfig;
use crate::i18n::t;

pub struct SecurityConfigDialog {
    pub open: bool,
    selected_sec_service: String,
    selected_algo: String,
    selected_verify: String,
    tx_padding: String,
    initialized: bool,
}

impl Default for SecurityConfigDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl SecurityConfigDialog {
    pub fn new() -> Self {
        Self {
            open: false,
            selected_sec_service: "0x27".to_string(),
            selected_algo: "SonnePower".to_string(),
            selected_verify: "crc32".to_string(),
            tx_padding: "0x55".to_string(),
            initialized: false,
        }
    }

    pub fn show(&mut self, ctx: &egui::Context, cfg: &mut AppConfig) {
        if !self.open {
            self.initialized = false;
            return;
        }

        // 每次打开时从当前 cfg 中同步当前值
        if !self.initialized {
            self.selected_sec_service = cfg.security_service.clone();
            self.selected_verify = cfg.verify_method.clone();
            self.tx_padding = cfg.isotp_tx_padding.clone();
            self.initialized = true;
        }

        let mut is_open = self.open;
        Window::new(t("menu_config_security"))
            .open(&mut is_open)
            .collapsible(false)
            .resizable(false)
            .movable(true)
            .default_width(420.0)
            .pivot(Align2::CENTER_CENTER)
            .default_pos(ctx.screen_rect().center())
            .show(ctx, |ui| {
                ui.add_space(4.0);

                // 1. 安全访问服务
                ui.group(|ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("安全访问服务 (Session Unlock)").strong().color(Color32::from_rgb(0, 188, 212)));
                    ui.add_space(6.0);

                    egui::Grid::new("sec_srv_grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                        ui.label("认证服务类型:");
                        egui::ComboBox::from_id_source("cb_sec_srv")
                            .selected_text(if self.selected_sec_service.contains("29") {
                                "0x29 (Authentication - 证书认证)"
                            } else {
                                "0x27 (SecurityAccess - 种子密钥)"
                            })
                            .width(220.0)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.selected_sec_service, "0x27".to_string(), "0x27 (SecurityAccess - 种子密钥)");
                                ui.selectable_value(&mut self.selected_sec_service, "0x29".to_string(), "0x29 (Authentication - 证书认证)");
                            });
                        ui.end_row();

                        if self.selected_sec_service.contains("27") {
                            ui.label("0x27 算法插件:");
                            egui::ComboBox::from_id_source("cb_sec_algo")
                                .selected_text(&self.selected_algo)
                                .width(220.0)
                                .show_ui(ui, |ui| {
                                    let mut algos: Vec<String> = cfg.security_algos.keys().cloned().collect();
                                    if !algos.contains(&"SonnePower".to_string()) {
                                        algos.insert(0, "SonnePower".to_string());
                                    }
                                    algos.sort();
                                    for a in algos {
                                        ui.selectable_value(&mut self.selected_algo, a.clone(), &a);
                                    }
                                });
                            ui.end_row();
                        }
                    });
                });

                ui.add_space(6.0);

                // 2. 固件完整性校验
                ui.group(|ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("固件完整性校验 (0x31 CheckIntegrity)").strong().color(Color32::from_rgb(0, 188, 212)));
                    ui.add_space(6.0);

                    egui::Grid::new("sec_ver_grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                        ui.label("固件验签模式:");
                        let cur_disp = match self.selected_verify.to_lowercase().as_str() {
                            "signature" => "Digital Signature (固件数字签名)",
                            "crc16" => "CRC16",
                            _ => "CRC32",
                        };

                        egui::ComboBox::from_id_source("cb_sec_verify")
                            .selected_text(cur_disp)
                            .width(220.0)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.selected_verify, "crc32".to_string(), "CRC32");
                                ui.selectable_value(&mut self.selected_verify, "crc16".to_string(), "CRC16");
                                ui.selectable_value(&mut self.selected_verify, "signature".to_string(), "Digital Signature (固件数字签名)");
                            });
                        ui.end_row();
                    });

                    ui.add_space(4.0);
                    ui.label(RichText::new("提示: 选为 Signature 时，主界面固件面板将显示固件签名加载区。").size(11.0).color(Color32::from_rgb(140, 140, 140)));
                });

                ui.add_space(6.0);

                // 3. 底层协议参数
                ui.group(|ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new("底层协议参数 (ISO 15765-2)").strong().color(Color32::from_rgb(0, 188, 212)));
                    ui.add_space(6.0);

                    egui::Grid::new("sec_param_grid").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                        ui.label("ISO-TP 填充字节:");
                        ui.add_sized([90.0, 22.0], egui::TextEdit::singleline(&mut self.tx_padding));
                        ui.end_row();
                    });
                });

                ui.add_space(12.0);

                // 底部按钮
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(t("dlg_cancel")).clicked() {
                        self.open = false;
                    }

                    ui.add_space(8.0);

                    let save_btn = egui::Button::new(RichText::new(t("dlg_save")).strong().color(Color32::WHITE))
                        .fill(Color32::from_rgb(0, 122, 204))
                        .min_size(egui::vec2(60.0, 24.0));

                    if ui.add(save_btn).clicked() {
                        // 彻底写回 AppConfig
                        cfg.security_service = self.selected_sec_service.clone();
                        cfg.verify_method = self.selected_verify.clone();
                        cfg.isotp_tx_padding = self.tx_padding.clone();

                        // 同步到当前产品
                        if let Some(p) = cfg.products.get_mut(&cfg.default_product) {
                            if p.override_security {
                                p.security_service = Some(self.selected_sec_service.clone());
                                p.verify_method = Some(self.selected_verify.clone());
                            }
                        }

                        self.open = false;
                    }
                });
            });

        self.open = is_open;
    }
}
