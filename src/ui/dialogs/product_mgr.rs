// src/ui/dialogs/product_mgr.rs
use crate::config::{save_config_dat, AppConfig, ProductConfig, RidOverride};
use crate::i18n::t;
use crate::ui::theme::{hex_to_color, CURRENT_THEME};
use eframe::egui::{self, Color32, RichText, ScrollArea, Window};
use std::collections::HashMap;

pub struct ProductManagerDialog {
    pub open: bool,
    selected_name: Option<String>,
    is_editing: bool,
    is_creating_new: bool,

    edit_name: String,
    edit_tx: String,
    edit_rx: String,
    edit_baud: String,
    edit_app: String,
    edit_boot: String,

    enable_security_override: bool,
    edit_sec_service: String,
    edit_sec_algo_or_cert: String,
    edit_verify_method: String,

    enable_rid_override: bool,
    rts_erase_rid: String,
    rts_verify_rid: String,
    app_erase_rid: String,
    app_verify_rid: String,
    mcu_erase_rid: String,
    mcu_verify_rid: String,
    boot_erase_rid: String,
    boot_verify_rid: String,
}

impl Default for ProductManagerDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl ProductManagerDialog {
    pub fn new() -> Self {
        Self {
            open: false,
            selected_name: None,
            is_editing: false,
            is_creating_new: false,

            edit_name: String::new(),
            edit_tx: String::new(),
            edit_rx: String::new(),
            edit_baud: String::new(),
            edit_app: String::new(),
            edit_boot: String::new(),

            enable_security_override: false,
            edit_sec_service: "0x27".to_string(),
            edit_sec_algo_or_cert: "SonnePower".to_string(),
            edit_verify_method: "crc32".to_string(),

            enable_rid_override: false,
            rts_erase_rid: "0x1001".to_string(),
            rts_verify_rid: "0x1002".to_string(),
            app_erase_rid: "0x1011".to_string(),
            app_verify_rid: "0x1012".to_string(),
            mcu_erase_rid: "0x1021".to_string(),
            mcu_verify_rid: "0x1022".to_string(),
            boot_erase_rid: "0x1041".to_string(),
            boot_verify_rid: "0x1042".to_string(),
        }
    }

    fn load_product_to_ui(&mut self, p: &ProductConfig) {
        self.edit_name = p.name.clone();
        self.edit_tx = p.txid.clone();
        self.edit_rx = p.rxid.clone();
        self.edit_baud = p.baudrate.clone();
        self.edit_app = p.app_address.clone();
        self.edit_boot = p.boot_address.clone();

        self.enable_security_override = p.override_security;
        self.edit_sec_service = p
            .security_service
            .clone()
            .unwrap_or_else(|| "0x27".to_string());
        self.edit_sec_algo_or_cert = p
            .security_algo_or_cert
            .clone()
            .unwrap_or_else(|| "SonnePower".to_string());
        self.edit_verify_method = p
            .verify_method
            .clone()
            .unwrap_or_else(|| "crc32".to_string());

        self.enable_rid_override = !p.flash_rid_overrides.is_empty();

        let get_r = |key: &str, def_e: &str, def_v: &str| -> (String, String) {
            if let Some(r) = p.flash_rid_overrides.get(key) {
                (
                    r.erase_rid.clone().unwrap_or_else(|| def_e.to_string()),
                    r.verify_rid.clone().unwrap_or_else(|| def_v.to_string()),
                )
            } else {
                (def_e.to_string(), def_v.to_string())
            }
        };

        let (e, v) = get_r("主RTS程序", "0x1001", "0x1002");
        self.rts_erase_rid = e;
        self.rts_verify_rid = v;

        let (e, v) = get_r("主APP程序", "0x1011", "0x1012");
        self.app_erase_rid = e;
        self.app_verify_rid = v;

        let (e, v) = get_r("从MCU程序", "0x1021", "0x1022");
        self.mcu_erase_rid = e;
        self.mcu_verify_rid = v;

        let (e, v) = get_r("BOOT程序", "0x1041", "0x1042");
        self.boot_erase_rid = e;
        self.boot_verify_rid = v;
    }

    pub fn show(&mut self, ctx: &egui::Context, cfg: &mut AppConfig) {
        if !self.open {
            self.is_editing = false;
            self.is_creating_new = false;
            return;
        }

        let theme = CURRENT_THEME.read().unwrap().clone();

        // 提取排序列表（排除 __custom__ 等内部特殊项）
        let mut sorted_names: Vec<String> = cfg
            .products
            .keys()
            .filter(|k| *k != crate::ui::left_panel::CUSTOM_PRODUCT_ID)
            .cloned()
            .collect();
        sorted_names.sort();

        // 1. 自动选中逻辑：仅在非新增且没有选中项时，兜底选择第 1 项
        if !self.is_creating_new && self.selected_name.is_none() && !sorted_names.is_empty() {
            let first_name = sorted_names[0].clone();
            if let Some(p) = cfg.products.get(&first_name) {
                self.load_product_to_ui(p);
                self.selected_name = Some(first_name);
                self.is_editing = false;
            }
        }

        let has_selection = self.selected_name.is_some();
        let mut is_open = self.open;
        let mut close_requested = false;

        // i18n 文本与兜底
        let title_txt = {
            let s = t("pm_title");
            if s == "pm_title" { "Product Model Manager".to_string() } else { s }
        };
        let list_title_txt = {
            let s = t("pm_list_title");
            if s == "pm_list_title" { "Product List:".to_string() } else { s }
        };
        let new_btn_raw = {
            let s = t("btn_new");
            if s == "btn_new" {
                let s2 = t("pm_new_btn");
                if s2 == "pm_new_btn" { "New".to_string() } else { s2 }
            } else {
                s
            }
        };
        let del_btn_raw = {
            let s = t("btn_del");
            if s == "btn_del" {
                let s2 = t("pm_del_btn");
                if s2 == "pm_del_btn" { "Delete".to_string() } else { s2 }
            } else {
                s
            }
        };
        let close_btn_txt = {
            let s = t("dlg_close");
            if s == "dlg_close" {
                let s2 = t("pm_close_btn");
                if s2 == "pm_close_btn" { "Close".to_string() } else { s2 }
            } else {
                s
            }
        };
        let cancel_btn_txt = {
            let s = t("dlg_cancel");
            if s == "dlg_cancel" { "Cancel".to_string() } else { s }
        };
        let save_btn_txt = {
            let s = t("dlg_save");
            if s == "dlg_save" {
                let s2 = t("pm_save_btn");
                if s2 == "pm_save_btn" { "Save".to_string() } else { s2 }
            } else {
                s
            }
        };
        let edit_btn_txt = {
            let s = t("dlg_edit");
            if s == "dlg_edit" {
                let s2 = t("pm_edit_btn");
                if s2 == "pm_edit_btn" { "Edit".to_string() } else { s2 }
            } else {
                s
            }
        };

        Window::new(title_txt)
            .open(&mut is_open)
            .resizable(true)
            .collapsible(false)
            .min_width(860.0)
            .min_height(600.0)
            .default_size([880.0, 620.0])
            .default_pos([260.0, 100.0])
            .show(ctx, |ui| {
                ui.horizontal_top(|ui| {
                    // ==================== 1. 左侧列表栏 ====================
                    ui.vertical(|ui| {
                        ui.set_width(220.0);
                        ui.label(RichText::new(list_title_txt).strong().size(13.5));
                        ui.add_space(4.0);

                        egui::Frame::none()
                            .fill(ui.visuals().faint_bg_color)
                            .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
                            .rounding(4.0)
                            .inner_margin(4.0)
                            .show(ui, |ui| {
                                ScrollArea::vertical()
                                    .id_source("prod_list_scroll")
                                    .max_height(460.0)
                                    .min_scrolled_height(460.0)
                                    .auto_shrink([false; 2])
                                    .show(ui, |ui| {
                                        ui.set_width(ui.available_width());

                                        for name in &sorted_names {
                                            let is_selected = self.selected_name.as_ref() == Some(name) && !self.is_creating_new;
                                            let resp = ui.add_sized(
                                                [ui.available_width(), 26.0],
                                                egui::SelectableLabel::new(is_selected, name),
                                            );
                                            if resp.clicked() {
                                                self.selected_name = Some(name.clone());
                                                self.is_editing = false;
                                                self.is_creating_new = false;
                                                if let Some(p) = cfg.products.get(name) {
                                                    self.load_product_to_ui(p);
                                                }
                                            }
                                        }

                                        // 若正在新建，在列表尾部提示正在创建的新项目
                                        if self.is_creating_new {
                                            ui.add_sized(
                                                [ui.available_width(), 26.0],
                                                egui::SelectableLabel::new(true, format!("* {}", self.edit_name)),
                                            );
                                        }
                                    });
                            });

                        ui.add_space(8.0);

                        // 左侧底部：+ New 与 - Delete
                        ui.horizontal(|ui| {
                            let btn_w = (ui.available_width() - 8.0) / 2.0;

                            // 核心修复：点击新增，自动生成不重名默认配置并激活新建编辑态
                            if ui
                                .add_sized([btn_w, 28.0], egui::Button::new(format!("+ {}", new_btn_raw)))
                                .clicked()
                            {
                                let mut idx = 1;
                                let mut new_name = format!("New_Product_{}", idx);
                                while cfg.products.contains_key(&new_name) {
                                    idx += 1;
                                    new_name = format!("New_Product_{}", idx);
                                }

                                self.selected_name = None;
                                self.is_editing = true;
                                self.is_creating_new = true;

                                self.edit_name = new_name;
                                self.edit_tx = "0x18FFFE00".to_string();
                                self.edit_rx = "0x18FF00FE".to_string();
                                self.edit_baud = "500000".to_string();
                                self.edit_app = "0x08010000".to_string();
                                self.edit_boot = "0x08000000".to_string();

                                self.enable_security_override = false;
                                self.edit_sec_service = "0x27".to_string();
                                self.edit_sec_algo_or_cert = "SonnePower".to_string();
                                self.edit_verify_method = "crc32".to_string();

                                self.enable_rid_override = true;
                                self.rts_erase_rid = "0x1001".to_string();
                                self.rts_verify_rid = "0x1002".to_string();
                                self.app_erase_rid = "0x1011".to_string();
                                self.app_verify_rid = "0x1012".to_string();
                                self.mcu_erase_rid = "0x1021".to_string();
                                self.mcu_verify_rid = "0x1022".to_string();
                                self.boot_erase_rid = "0x1041".to_string();
                                self.boot_verify_rid = "0x1042".to_string();
                            }

                            // 删除按钮：仅在选中已有项目时可用
                            let can_del = has_selection && !self.is_creating_new && sorted_names.len() > 1;
                            ui.add_enabled_ui(can_del, |ui| {
                                if ui
                                    .add_sized([btn_w, 28.0], egui::Button::new(format!("- {}", del_btn_raw)))
                                    .clicked()
                                {
                                    if let Some(sel) = &self.selected_name {
                                        cfg.products.remove(sel);
                                        save_config_dat(cfg);
                                        self.selected_name = None;
                                        self.is_editing = false;
                                        self.is_creating_new = false;
                                    }
                                }
                            });
                        });
                    });

                    ui.separator();

                    // ==================== 2. 右侧配置区 ====================
                    ui.vertical(|ui| {
                        ui.set_width(ui.available_width());

                        // 2.1 顶部表单滚动区
                        ScrollArea::vertical()
                            .id_source("prod_right_form_scroll")
                            .max_height(470.0)
                            .min_scrolled_height(470.0)
                            .auto_shrink([false; 2])
                            .show(ui, |ui| {
                                ui.vertical(|ui| {
                                    // 基础参数组
                                    ui.group(|ui| {
                                        ui.set_width(ui.available_width());
                                        ui.horizontal(|ui| {
                                            ui.label(
                                                RichText::new(t("pm_grp_title"))
                                                    .strong()
                                                    .color(hex_to_color(&theme.accent)),
                                            );
                                            if !self.is_editing {
                                                ui.label(RichText::new(t("pm_readonly_hint")).small().color(Color32::GRAY));
                                            } else if self.is_creating_new {
                                                ui.label(
                                                    RichText::new("(* 新增中 / Creating)")
                                                        .small()
                                                        .color(hex_to_color(&theme.accent)),
                                                );
                                            } else {
                                                ui.label(
                                                    RichText::new(t("pm_editing_hint"))
                                                        .small()
                                                        .color(hex_to_color(&theme.accent)),
                                                );
                                            }
                                        });
                                        ui.add_space(4.0);

                                        ui.add_enabled_ui(self.is_editing, |ui| {
                                            egui::Grid::new("prod_base_grid")
                                                .num_columns(2)
                                                .spacing([14.0, 7.0])
                                                .show(ui, |ui| {
                                                    ui.label(t("pm_field_name"));
                                                    ui.add(egui::TextEdit::singleline(&mut self.edit_name).desired_width(280.0));
                                                    ui.end_row();

                                                    ui.label(format!("{}:", t("pm_field_tx")));
                                                    ui.add(egui::TextEdit::singleline(&mut self.edit_tx).desired_width(280.0));
                                                    ui.end_row();

                                                    ui.label(format!("{}:", t("pm_field_rx")));
                                                    ui.add(egui::TextEdit::singleline(&mut self.edit_rx).desired_width(280.0));
                                                    ui.end_row();

                                                    ui.label(t("pm_field_baud"));
                                                    egui::ComboBox::from_id_source("pm_baud_combo")
                                                        .selected_text(&self.edit_baud)
                                                        .width(260.0)
                                                        .show_ui(ui, |ui| {
                                                            for b in &["100000", "125000", "250000", "500000", "1000000"] {
                                                                ui.selectable_value(&mut self.edit_baud, b.to_string(), *b);
                                                            }
                                                        });
                                                    ui.end_row();

                                                    ui.label(t("pm_field_app"));
                                                    ui.add(egui::TextEdit::singleline(&mut self.edit_app).desired_width(280.0));
                                                    ui.end_row();

                                                    ui.label(t("pm_field_boot"));
                                                    ui.add(egui::TextEdit::singleline(&mut self.edit_boot).desired_width(280.0));
                                                    ui.end_row();
                                                });
                                        });
                                    });

                                    ui.add_space(6.0);

                                    // 安全访问与校验覆盖组
                                    ui.add_enabled_ui(self.is_editing, |ui| {
                                        ui.checkbox(&mut self.enable_security_override, t("pm_sec_override_cb"));
                                        if self.enable_security_override {
                                            ui.group(|ui| {
                                                ui.set_width(ui.available_width());
                                                ui.label(
                                                    RichText::new(t("pm_sec_frame_title"))
                                                        .small()
                                                        .color(hex_to_color(&theme.accent)),
                                                );
                                                egui::Grid::new("sec_override_grid")
                                                    .num_columns(2)
                                                    .spacing([14.0, 7.0])
                                                    .show(ui, |ui| {
                                                        ui.label(t("pm_sec_service_type"));
                                                        egui::ComboBox::from_id_source("pm_cb_sec_service")
                                                            .selected_text(if self.edit_sec_service == "0x29" {
                                                                t("pm_sec_service_0x29")
                                                            } else {
                                                                t("pm_sec_service_0x27")
                                                            })
                                                            .width(260.0)
                                                            .show_ui(ui, |ui| {
                                                                ui.selectable_value(
                                                                    &mut self.edit_sec_service,
                                                                    "0x27".to_string(),
                                                                    t("pm_sec_service_0x27"),
                                                                );
                                                                ui.selectable_value(
                                                                    &mut self.edit_sec_service,
                                                                    "0x29".to_string(),
                                                                    t("pm_sec_service_0x29"),
                                                                );
                                                            });
                                                        ui.end_row();

                                                        if self.edit_sec_service == "0x27" {
                                                            ui.label(t("pm_sec_algo_title"));
                                                            egui::ComboBox::from_id_source("pm_cb_sec_algo")
                                                                .selected_text(&self.edit_sec_algo_or_cert)
                                                                .width(260.0)
                                                                .show_ui(ui, |ui| {
                                                                    for algo in cfg.security_algos.keys() {
                                                                        ui.selectable_value(&mut self.edit_sec_algo_or_cert, algo.clone(), algo);
                                                                    }
                                                                    ui.selectable_value(&mut self.edit_sec_algo_or_cert, "SonnePower".to_string(), "SonnePower");
                                                                    ui.selectable_value(&mut self.edit_sec_algo_or_cert, "Lovol T-Box".to_string(), "Lovol T-Box");
                                                                });
                                                            ui.end_row();
                                                        } else {
                                                            ui.label(t("pm_sec_cert_title"));
                                                            ui.horizontal(|ui| {
                                                                ui.add(egui::TextEdit::singleline(&mut self.edit_sec_algo_or_cert).desired_width(190.0));
                                                                if ui.button(t("browse_btn")).clicked() {
                                                                    if let Some(p) = rfd::FileDialog::new()
                                                                        .add_filter("Cert", &["crt", "pem", "py", "dll"])
                                                                        .pick_file()
                                                                    {
                                                                        self.edit_sec_algo_or_cert = p.to_string_lossy().to_string();
                                                                    }
                                                                }
                                                            });
                                                            ui.end_row();
                                                        }

                                                        ui.label(t("pm_sec_verify_mode"));
                                                        egui::ComboBox::from_id_source("pm_cb_verify_method")
                                                            .selected_text(if self.edit_verify_method == "signature" {
                                                                t("pm_verify_sig")
                                                            } else {
                                                                t("pm_verify_crc32")
                                                            })
                                                            .width(260.0)
                                                            .show_ui(ui, |ui| {
                                                                ui.selectable_value(&mut self.edit_verify_method, "crc32".to_string(), t("pm_verify_crc32"));
                                                                ui.selectable_value(&mut self.edit_verify_method, "signature".to_string(), t("pm_verify_sig"));
                                                            });
                                                        ui.end_row();
                                                    });
                                            });
                                        }
                                    });

                                    ui.add_space(6.0);

                                    // 4项完整 RID 覆盖组
                                    ui.add_enabled_ui(self.is_editing, |ui| {
                                        ui.checkbox(&mut self.enable_rid_override, t("pm_rid_override_cb"));
                                        if self.enable_rid_override {
                                            ui.group(|ui| {
                                                ui.set_width(ui.available_width());
                                                ui.label(
                                                    RichText::new(t("pm_rid_frame"))
                                                        .small()
                                                        .color(hex_to_color(&theme.accent)),
                                                );
                                                egui::Grid::new("rid_table_grid_4")
                                                    .num_columns(3)
                                                    .spacing([16.0, 6.0])
                                                    .show(ui, |ui| {
                                                        ui.label(RichText::new(t("pm_rid_col_type")).strong());
                                                        ui.label(RichText::new(t("pm_rid_col_erase")).strong());
                                                        ui.label(RichText::new(t("pm_rid_col_verify")).strong());
                                                        ui.end_row();

                                                        // 1. 主RTS程序
                                                        ui.label(t("pm_type_rts"));
                                                        ui.add(egui::TextEdit::singleline(&mut self.rts_erase_rid).desired_width(110.0));
                                                        ui.add(egui::TextEdit::singleline(&mut self.rts_verify_rid).desired_width(110.0));
                                                        ui.end_row();

                                                        // 2. 主APP程序
                                                        ui.label(t("pm_type_app"));
                                                        ui.add(egui::TextEdit::singleline(&mut self.app_erase_rid).desired_width(110.0));
                                                        ui.add(egui::TextEdit::singleline(&mut self.app_verify_rid).desired_width(110.0));
                                                        ui.end_row();

                                                        // 3. 从MCU程序
                                                        ui.label(t("pm_type_mcu"));
                                                        ui.add(egui::TextEdit::singleline(&mut self.mcu_erase_rid).desired_width(110.0));
                                                        ui.add(egui::TextEdit::singleline(&mut self.mcu_verify_rid).desired_width(110.0));
                                                        ui.end_row();

                                                        // 4. BOOT程序
                                                        ui.label(t("pm_type_boot"));
                                                        ui.add(egui::TextEdit::singleline(&mut self.boot_erase_rid).desired_width(110.0));
                                                        ui.add(egui::TextEdit::singleline(&mut self.boot_verify_rid).desired_width(110.0));
                                                        ui.end_row();
                                                    });
                                            });
                                        }
                                    });

                                    ui.add_space(4.0);
                                });
                            });

                        // 2.2 底部固定操作栏
                        ui.add_space(8.0);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let btn_size = egui::vec2(72.0, 26.0);

                            // 编辑/新增状态下显示取消，非编辑状态下显示关闭
                            let exit_txt = if self.is_editing { cancel_btn_txt } else { close_btn_txt };
                            if ui.add_sized(btn_size, egui::Button::new(exit_txt)).clicked() {
                                if self.is_editing {
                                    self.is_editing = false;
                                    self.is_creating_new = false;
                                    // 恢复原本选中的项目
                                    if let Some(name) = &self.selected_name {
                                        if let Some(p) = cfg.products.get(name) {
                                            self.load_product_to_ui(p);
                                        }
                                    }
                                } else {
                                    close_requested = true;
                                }
                            }

                            ui.add_space(8.0);

                            let (btn_text, btn_fill) = if self.is_editing {
                                (save_btn_txt, hex_to_color(&theme.primary_btn))
                            } else {
                                (edit_btn_txt, hex_to_color(&theme.success_btn))
                            };

                            let action_btn = egui::Button::new(
                                RichText::new(btn_text).strong().color(Color32::WHITE),
                            )
                            .fill(btn_fill);

                            let can_act = self.is_editing || has_selection;
                            ui.add_enabled_ui(can_act, |ui| {
                                if ui.add_sized(btn_size, action_btn).clicked() {
                                    if !self.is_editing {
                                        self.is_editing = true;
                                    } else if !self.edit_name.trim().is_empty() {
                                        let mut overrides = HashMap::new();
                                        if self.enable_rid_override {
                                            overrides.insert(
                                                "主RTS程序".to_string(),
                                                RidOverride {
                                                    erase_rid: Some(self.rts_erase_rid.clone()),
                                                    verify_rid: Some(self.rts_verify_rid.clone()),
                                                },
                                            );
                                            overrides.insert(
                                                "主APP程序".to_string(),
                                                RidOverride {
                                                    erase_rid: Some(self.app_erase_rid.clone()),
                                                    verify_rid: Some(self.app_verify_rid.clone()),
                                                },
                                            );
                                            overrides.insert(
                                                "从MCU程序".to_string(),
                                                RidOverride {
                                                    erase_rid: Some(self.mcu_erase_rid.clone()),
                                                    verify_rid: Some(self.mcu_verify_rid.clone()),
                                                },
                                            );
                                            overrides.insert(
                                                "BOOT程序".to_string(),
                                                RidOverride {
                                                    erase_rid: Some(self.boot_erase_rid.clone()),
                                                    verify_rid: Some(self.boot_verify_rid.clone()),
                                                },
                                            );
                                        }

                                        let new_k = self.edit_name.trim().to_string();

                                        // 若用户重命名了旧项，先清除旧项
                                        if let Some(old_k) = &self.selected_name {
                                            if !self.is_creating_new && *old_k != new_k {
                                                cfg.products.remove(old_k);
                                            }
                                        }

                                        cfg.products.insert(
                                            new_k.clone(),
                                            ProductConfig {
                                                name: new_k.clone(),
                                                txid: self.edit_tx.clone(),
                                                rxid: self.edit_rx.clone(),
                                                baudrate: self.edit_baud.clone(),
                                                app_address: self.edit_app.clone(),
                                                boot_address: self.edit_boot.clone(),
                                                override_security: self.enable_security_override,
                                                security_service: if self.enable_security_override {
                                                    Some(self.edit_sec_service.clone())
                                                } else {
                                                    None
                                                },
                                                security_algo_or_cert: if self.enable_security_override {
                                                    Some(self.edit_sec_algo_or_cert.clone())
                                                } else {
                                                    None
                                                },
                                                verify_method: if self.enable_security_override {
                                                    Some(self.edit_verify_method.clone())
                                                } else {
                                                    None
                                                },
                                                flash_rid_overrides: overrides,
                                            },
                                        );

                                        // 持久化到本地配置文件
                                        save_config_dat(cfg);

                                        self.selected_name = Some(new_k);
                                        self.is_editing = false;
                                        self.is_creating_new = false;
                                    }
                                }
                            });
                        });
                    });
                });
            });

        if close_requested || !is_open {
            self.open = false;
        }
    }
}