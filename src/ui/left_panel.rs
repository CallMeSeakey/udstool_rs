// src/ui/left_panel.rs
use crate::config::AppConfig;
use crate::i18n::t;
use crate::ui::theme::{hex_to_color, CURRENT_THEME};
use egui::{RichText, ScrollArea, Stroke, Ui};

pub const CUSTOM_PRODUCT_ID: &str = "__custom__";

#[derive(Clone, Debug)]
pub struct LeftPanelState {
    pub selected_product: String,
    pub selected_interface: String,
    pub selected_channel: String,
    pub selected_baudrate: String,
    pub txid: String,
    pub rxid: String,
    pub flash_address: String,
    pub flash_type_key: String,
    pub did_values: std::collections::HashMap<String, String>,
    pub did_original_values: std::collections::HashMap<String, String>,
}

impl LeftPanelState {
    pub fn new(cfg: &AppConfig) -> Self {
        let prod_key = if cfg.default_product == CUSTOM_PRODUCT_ID
            || cfg.default_product == "自定义"
            || cfg.default_product.eq_ignore_ascii_case("custom")
        {
            CUSTOM_PRODUCT_ID.to_string()
        } else {
            cfg.default_product.clone()
        };

        let p = cfg
            .products
            .get(&prod_key)
            .cloned()
            .unwrap_or_else(|| cfg.products.values().next().cloned().unwrap_or_default());

        let flash_type = if !cfg.default_flash_type.is_empty() {
            cfg.default_flash_type.clone()
        } else {
            "app".to_string()
        };

        let addr = if flash_type == "boot" {
            p.boot_address.clone()
        } else {
            p.app_address.clone()
        };

        Self {
            selected_product: prod_key,
            selected_interface: cfg.default_interface.clone(),
            selected_channel: cfg.default_channel.clone(),
            selected_baudrate: if !p.baudrate.is_empty() {
                p.baudrate
            } else {
                cfg.default_baudrate.clone()
            },
            txid: p.txid,
            rxid: p.rxid,
            flash_address: addr,
            flash_type_key: flash_type,
            did_values: std::collections::HashMap::new(),
            did_original_values: std::collections::HashMap::new(),
        }
    }

    pub fn is_custom(&self) -> bool {
        self.selected_product == CUSTOM_PRODUCT_ID
    }

    pub fn sync_from_product(&mut self, cfg: &AppConfig) {
        if let Some(p) = cfg.products.get(&self.selected_product) {
            self.txid = p.txid.clone();
            self.rxid = p.rxid.clone();
            self.selected_baudrate = p.baudrate.clone();
            self.flash_address = if self.flash_type_key == "boot" {
                p.boot_address.clone()
            } else {
                p.app_address.clone()
            };
        }
    }
}

fn render_text_edit(
    ui: &mut Ui,
    text: &mut String,
    width: f32,
    height: f32,
    interactive: bool,
    hint: &str,
) -> bool {
    let theme = CURRENT_THEME.read().unwrap().clone();

    if interactive {
        let resp = ui.add_sized(
            [width, height],
            egui::TextEdit::singleline(text).hint_text(hint),
        );
        resp.changed()
    } else {
        egui::Frame::none()
            .fill(hex_to_color(&theme.readonly_bg))
            .stroke(Stroke::new(1.0_f32, hex_to_color(&theme.readonly_border)))
            .rounding(3.0)
            .inner_margin(egui::Margin::symmetric(4.0, 2.0))
            .show(ui, |ui| {
                ui.set_min_size(egui::vec2(width.max(20.0) - 8.0, height - 4.0));
                let display_text = if text.trim().is_empty() { hint } else { text };
                ui.label(
                    RichText::new(display_text)
                        .color(hex_to_color(&theme.readonly_text))
                        .size(12.5),
                );
            });
        false
    }
}

pub fn render(
    ui: &mut Ui,
    state: &mut LeftPanelState,
    cfg: &AppConfig,
    is_connected: bool,
    is_flashing: bool,
    is_querying_dids: bool,
    on_connect_toggle: impl FnOnce(),
    on_query_all_dids: impl FnOnce(),
    mut on_write_did: impl FnMut(String, String),
    mut on_config_changed: impl FnMut(String, String),
) {
    let theme = CURRENT_THEME.read().unwrap().clone();
    let border_col = hex_to_color(&theme.border);
    let panel_bg = hex_to_color(&theme.window_bg);
    let accent_col = hex_to_color(&theme.accent);
    let text_normal_col = hex_to_color(&theme.text_normal);
    let total_h = ui.available_height();

    egui::Frame::none()
        .fill(panel_bg)
        .stroke(Stroke::new(1.0_f32, border_col))
        .rounding(4.0)
        .inner_margin(egui::Margin::same(8.0))
        .show(ui, |ui| {
            ui.set_min_height(total_h - 16.0);
            ScrollArea::vertical()
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    let full_w = ui.available_width();

                    // ==================== 1. 配置 查询与写入 ====================
                    ui.label(
                        RichText::new(t("config_frame"))
                            .strong()
                            .size(13.5)
                            .color(accent_col),
                    );
                    ui.add_space(4.0);

                    let label_col_w = 135.0;
                    let grid_spacing_x = 12.0;
                    let ctrl_width = (full_w - label_col_w - grid_spacing_x - 4.0).max(140.0);
                    let ctrl_height = 24.0;

                    let is_custom = state.is_custom();

                    egui::Grid::new("can_cfg_grid")
                        .num_columns(2)
                        .spacing([grid_spacing_x, 7.0])
                        .show(ui, |ui| {
                            // CAN 硬件 (完全从驱动注册表读取)
                            ui.add_sized(
                                [label_col_w, ctrl_height],
                                egui::Label::new(format!("{}:", t("can_hardware"))),
                            );

                            let hardware_list = crate::core::can::supported_hardware_list();
                            let prev_hardware = state.selected_interface.clone();

                            ui.add_sized([ctrl_width, ctrl_height], |ui: &mut egui::Ui| {
                                egui::ComboBox::from_id_source("cb_hw")
                                    .selected_text(&state.selected_interface)
                                    .width(ctrl_width)
                                    .show_ui(ui, |ui| {
                                        for &hw in &hardware_list {
                                            ui.selectable_value(
                                                &mut state.selected_interface,
                                                hw.to_string(),
                                                hw,
                                            );
                                        }
                                    })
                                    .response
                            });
                            ui.end_row();

                            // 联动刷新：一旦检测到硬件变更或通道不在新硬件支持列表中，自动重置为第一个可用通道
                            let available_channels =
                                crate::core::can::get_available_channels(&state.selected_interface);

                            if state.selected_interface != prev_hardware
                                || !available_channels.contains(&state.selected_channel)
                            {
                                if let Some(first_ch) = available_channels.first() {
                                    state.selected_channel = first_ch.clone();
                                }
                                on_config_changed(
                                    t("can_hardware"),
                                    state.selected_interface.clone(),
                                );
                                on_config_changed(t("can_channel"), state.selected_channel.clone());
                            }

                            // CAN 通道 (动态展示当前硬件探测到的所有通道)
                            ui.add_sized(
                                [label_col_w, ctrl_height],
                                egui::Label::new(format!("{}:", t("can_channel"))),
                            );
                            ui.add_sized([ctrl_width, ctrl_height], |ui: &mut egui::Ui| {
                                egui::ComboBox::from_id_source("cb_channel")
                                    .selected_text(&state.selected_channel)
                                    .width(ctrl_width)
                                    .show_ui(ui, |ui| {
                                        for ch in &available_channels {
                                            if ui
                                                .selectable_value(
                                                    &mut state.selected_channel,
                                                    ch.clone(),
                                                    ch,
                                                )
                                                .clicked()
                                            {
                                                on_config_changed(t("can_channel"), ch.clone());
                                            }
                                        }
                                    })
                                    .response
                            });
                            ui.end_row();

                            // CAN 波特率
                            ui.add_sized(
                                [label_col_w, ctrl_height],
                                egui::Label::new(format!("{}:", t("can_baudrate"))),
                            );
                            let baud_combo = |ui: &mut egui::Ui| {
                                egui::ComboBox::from_id_source("cb_baud")
                                    .selected_text(&state.selected_baudrate)
                                    .width(ctrl_width)
                                    .show_ui(ui, |ui| {
                                        for b in
                                            &["100000", "125000", "250000", "500000", "1000000"]
                                        {
                                            if ui
                                                .selectable_value(
                                                    &mut state.selected_baudrate,
                                                    b.to_string(),
                                                    *b,
                                                )
                                                .clicked()
                                            {
                                                on_config_changed(t("can_baudrate"), b.to_string());
                                            }
                                        }
                                    })
                                    .response
                            };
                            if is_custom {
                                ui.add_sized([ctrl_width, ctrl_height], baud_combo);
                            } else {
                                ui.add_enabled_ui(false, |ui| {
                                    ui.add_sized([ctrl_width, ctrl_height], baud_combo);
                                });
                            }
                            ui.end_row();

                            // 产品型号
                            ui.add_sized(
                                [label_col_w, ctrl_height],
                                egui::Label::new(format!("{}:", t("product_model"))),
                            );
                            let custom_label = t("pm_custom");
                            let current_prod_display = if is_custom {
                                custom_label.clone()
                            } else {
                                state.selected_product.clone()
                            };

                            ui.add_sized([ctrl_width, ctrl_height], |ui: &mut egui::Ui| {
                                egui::ComboBox::from_id_source("cb_prod")
                                    .selected_text(current_prod_display)
                                    .width(ctrl_width)
                                    .show_ui(ui, |ui| {
                                        let mut names: Vec<String> = cfg
                                            .products
                                            .keys()
                                            .filter(|k| *k != CUSTOM_PRODUCT_ID)
                                            .cloned()
                                            .collect();
                                        names.sort();

                                        for name in names {
                                            let is_selected = state.selected_product == name;
                                            if ui.selectable_label(is_selected, &name).clicked() {
                                                state.selected_product = name.clone();
                                                state.sync_from_product(cfg);
                                                on_config_changed(t("product_model"), name);
                                            }
                                        }

                                        ui.separator();

                                        if ui.selectable_label(is_custom, &custom_label).clicked() {
                                            state.selected_product = CUSTOM_PRODUCT_ID.to_string();
                                            state.sync_from_product(cfg);
                                            on_config_changed(
                                                t("product_model"),
                                                CUSTOM_PRODUCT_ID.to_string(),
                                            );
                                        }
                                    })
                                    .response
                            });
                            ui.end_row();

                            // 刷写类型
                            ui.add_sized(
                                [label_col_w, ctrl_height],
                                egui::Label::new(format!("{}:", t("flash_type"))),
                            );
                            let flash_types = [
                                ("rts", t("flash_type_rts")),
                                ("app", t("flash_type_app")),
                                ("mcu", t("flash_type_mcu")),
                                ("boot", t("flash_type_boot")),
                            ];
                            let current_flash_title = match state.flash_type_key.as_str() {
                                "app" => t("flash_type_app"),
                                "mcu" => t("flash_type_mcu"),
                                "boot" => t("flash_type_boot"),
                                _ => t("flash_type_rts"),
                            };

                            ui.add_sized([ctrl_width, ctrl_height], |ui: &mut egui::Ui| {
                                egui::ComboBox::from_id_source("cb_flash_type")
                                    .selected_text(&current_flash_title)
                                    .width(ctrl_width)
                                    .show_ui(ui, |ui| {
                                        for (key, title) in &flash_types {
                                            if ui
                                                .selectable_value(
                                                    &mut state.flash_type_key,
                                                    key.to_string(),
                                                    title,
                                                )
                                                .clicked()
                                            {
                                                if let Some(p) =
                                                    cfg.products.get(&state.selected_product)
                                                {
                                                    state.flash_address = if *key == "boot" {
                                                        p.boot_address.clone()
                                                    } else {
                                                        p.app_address.clone()
                                                    };
                                                }
                                                on_config_changed(t("flash_type"), title.clone());
                                            }
                                        }
                                    })
                                    .response
                            });
                            ui.end_row();

                            // 物理寻址 (Tester->ECU)
                            ui.add_sized(
                                [label_col_w, ctrl_height],
                                egui::Label::new(format!("{}:", t("txid_label"))),
                            );
                            if render_text_edit(
                                ui,
                                &mut state.txid,
                                ctrl_width,
                                ctrl_height,
                                is_custom,
                                "",
                            ) {
                                on_config_changed("custom_txid".to_string(), state.txid.clone());
                            }
                            ui.end_row();

                            // 物理寻址 (ECU->Tester)
                            ui.add_sized(
                                [label_col_w, ctrl_height],
                                egui::Label::new(format!("{}:", t("rxid_label"))),
                            );
                            if render_text_edit(
                                ui,
                                &mut state.rxid,
                                ctrl_width,
                                ctrl_height,
                                is_custom,
                                "",
                            ) {
                                on_config_changed("custom_rxid".to_string(), state.rxid.clone());
                            }
                            ui.end_row();

                            // 刷写地址 (hex)
                            ui.add_sized(
                                [label_col_w, ctrl_height],
                                egui::Label::new(format!("{}:", t("flash_address"))),
                            );
                            if render_text_edit(
                                ui,
                                &mut state.flash_address,
                                ctrl_width,
                                ctrl_height,
                                is_custom,
                                "",
                            ) {
                                on_config_changed(
                                    "custom_flash_address".to_string(),
                                    state.flash_address.clone(),
                                );
                            }
                            ui.end_row();
                        });

                    ui.add_space(8.0);

                    // ==================== CAN 连接按钮 ====================
                    let conn_btn_text = if is_connected {
                        format!("⏹ {}", t("disconnect_btn"))
                    } else {
                        format!("⚡ {}", t("connect_btn"))
                    };
                    let conn_btn_color = if is_connected {
                        hex_to_color(&theme.danger_btn)
                    } else {
                        hex_to_color(&theme.success_btn)
                    };

                    let btn_w = ctrl_width;
                    ui.horizontal(|ui| {
                        ui.add_space(label_col_w + grid_spacing_x);
                        let connect_btn = egui::Button::new(
                            RichText::new(conn_btn_text).strong().color(text_normal_col),
                        )
                        .fill(conn_btn_color)
                        .rounding(4.0);

                        if ui.add_sized([btn_w, 30.0], connect_btn).clicked() {
                            on_connect_toggle();
                        }
                    });

                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(8.0);

                    // ==================== 2. DID 数据交互 ====================
                    ui.label(
                        RichText::new(t("did_frame"))
                            .strong()
                            .size(13.5)
                            .color(accent_col),
                    );
                    ui.add_space(4.0);

                    let can_query = is_connected && !is_flashing && !is_querying_dids;

                    ui.horizontal(|ui| {
                        ui.add_space(label_col_w + grid_spacing_x);
                        let query_txt = if is_querying_dids {
                            format!("⏳ {}...", t("query_all_did"))
                        } else {
                            format!("🔍 {}", t("query_all_did"))
                        };

                        let mut query_btn = egui::Button::new(
                            if can_query {
                                RichText::new(query_txt).strong().color(egui::Color32::WHITE)
                            } else {
                                RichText::new(query_txt)
                            },
                        )
                        .min_size(egui::vec2(btn_w, 30.0))
                        .rounding(4.0);

                        if can_query {
                            query_btn = query_btn.fill(hex_to_color(&theme.query_btn_bg));
                        }

                        let query_resp = ui.add_enabled(can_query, query_btn);

                        if !can_query {
                            let hint = if is_flashing {
                                "Flashing"
                            } else if is_querying_dids {
                                "Querying DIDs..."
                            } else {
                                "Connect hardware first"
                            };
                            query_resp.on_disabled_hover_text(hint);
                        } else if query_resp.clicked() {
                            on_query_all_dids();
                        }
                    });

                    ui.add_space(6.0);

                    let did_input_height = 22.0;
                    let not_queried_str = t("not_queried");
                    let did_label_col_w = label_col_w;
                    let write_btn_w = 64.0;
                    let did_spacing_x = 8.0;
                    let did_input_width = (ctrl_width - write_btn_w - did_spacing_x).max(80.0);

                    let mut sorted_keys: Vec<String> = cfg.dids.keys().cloned().collect();
                    sorted_keys.sort_by_key(|k| cfg.dids.get(k).map(|d| d.did).unwrap_or(0));

                    egui::Grid::new("did_items_grid_dynamic")
                        .num_columns(3)
                        .spacing([did_spacing_x, 6.0])
                        .show(ui, |ui| {
                            for key in &sorted_keys {
                                if let Some(did_cfg) = cfg.dids.get(key) {
                                    let is_rw = did_cfg.rw.to_uppercase() == "RW";

                                    let i18n_key = format!("did_{}", key);
                                    let translated = t(&i18n_key);
                                    let label_str = if translated != i18n_key {
                                        translated
                                    } else {
                                        format!("{}:", did_cfg.name)
                                    };

                                    ui.add_sized(
                                        [did_label_col_w, did_input_height],
                                        |ui: &mut Ui| ui.label(RichText::new(label_str).size(12.0)),
                                    );

                                    let val_entry = state
                                        .did_values
                                        .entry(key.to_string())
                                        .or_insert_with(String::new);
                                    let orig_entry = state
                                        .did_original_values
                                        .entry(key.to_string())
                                        .or_insert_with(String::new);

                                    let has_queried = !orig_entry.trim().is_empty();
                                    let is_unsupported = orig_entry.contains("不支持")
                                        || orig_entry.to_lowercase().contains("unsupported")
                                        || orig_entry.to_lowercase().contains("nrc");

                                    let can_edit = is_connected
                                        && is_rw
                                        && has_queried
                                        && !is_unsupported
                                        && !is_flashing
                                        && !is_querying_dids;

                                    render_text_edit(
                                        ui,
                                        val_entry,
                                        did_input_width,
                                        did_input_height,
                                        can_edit,
                                        &not_queried_str,
                                    );

                                    if is_rw {
                                        let is_modified = val_entry.trim() != orig_entry.trim()
                                            && !val_entry.trim().is_empty()
                                            && !is_unsupported;
                                        let btn_enabled = is_connected
                                            && !is_flashing
                                            && !is_querying_dids
                                            && is_modified
                                            && !is_unsupported;

                                        let write_btn = egui::Button::new(
                                            RichText::new(format!("💾 {}", t("write_btn")))
                                                .size(12.0),
                                        )
                                        .min_size(egui::vec2(write_btn_w, did_input_height));

                                        let write_resp = ui.add_enabled(btn_enabled, write_btn);

                                        if !btn_enabled {
                                            let hint = if is_unsupported {
                                                "Unsupported"
                                            } else if is_flashing {
                                                "Flashing"
                                            } else if is_querying_dids {
                                                "Querying DIDs..."
                                            } else if !is_connected {
                                                "Offline"
                                            } else if !has_queried {
                                                "Query first"
                                            } else {
                                                "Not modified"
                                            };
                                            write_resp.on_disabled_hover_text(hint);
                                        } else if write_resp.clicked() {
                                            on_write_did(key.to_string(), val_entry.clone());
                                        }
                                    } else {
                                        ui.add_sized(
                                            [write_btn_w, did_input_height],
                                            egui::Label::new(""),
                                        );
                                    }
                                    ui.end_row();
                                }
                            }
                        });
                });
        });
}