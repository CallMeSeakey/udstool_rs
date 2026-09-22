// src/ui/dialogs/did_mgr.rs
use crate::config::{save_config_dat, AppConfig, DidConfig};
use crate::i18n::{remove_did_i18n, sync_did_i18n, t};
use eframe::egui::{self, Color32, RichText, ScrollArea, Window};
use std::collections::HashMap;

pub struct DidManagerDialog {
    pub open: bool,
    active_tab: usize,
    selected_key: Option<String>,
    edit_key: String,
    edit_did: String,
    edit_zh: String,
    edit_en: String,
    edit_fmt: String,
    edit_len: usize,
}

impl Default for DidManagerDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl DidManagerDialog {
    pub fn new() -> Self {
        Self {
            open: false,
            active_tab: 0,
            selected_key: None,
            edit_key: String::new(),
            edit_did: String::new(),
            edit_zh: String::new(),
            edit_en: String::new(),
            edit_fmt: "hex".to_string(),
            edit_len: 4,
        }
    }

    pub fn show(&mut self, ctx: &egui::Context, cfg: &mut AppConfig) {
        if !self.open {
            return;
        }

        let mut is_open = self.open;
        let mut close_requested = false;

        // 1. 获取语言包纯净文本
        let title_txt = {
            let s = t("menu_config_did_mgr");
            if s == "menu_config_did_mgr" { "DID Items Config".to_string() } else { s }
        };
        let tab_rw_raw = {
            let s = t("did_tab_rw");
            if s == "did_tab_rw" { "Read/Write DID (RW)".to_string() } else { s }
        };
        let tab_ro_raw = {
            let s = t("did_tab_ro");
            if s == "did_tab_ro" { "Read-Only DID (RO)".to_string() } else { s }
        };
        let btn_clear_new_raw = {
            let s = t("did_btn_clear_new");
            if s == "did_btn_clear_new" { "Reset Form for New".to_string() } else { s }
        };

        let th_key_txt = {
            let s = t("did_th_key");
            if s == "did_th_key" { "Key".to_string() } else { s }
        };
        let th_did_txt = {
            let s = t("did_th_did");
            if s == "did_th_did" { "DID (Hex)".to_string() } else { s }
        };
        let th_zh_name_txt = {
            let s = t("did_th_zh_name");
            if s == "did_th_zh_name" { "ZH Name".to_string() } else { s }
        };
        let th_en_name_txt = {
            let s = t("did_th_en_name");
            if s == "did_th_en_name" { "EN Name".to_string() } else { s }
        };
        let th_fmt_txt = {
            let s = t("did_th_fmt");
            if s == "did_th_fmt" { "Format".to_string() } else { s }
        };
        let th_len_txt = {
            let s = t("did_th_len");
            if s == "did_th_len" { "Length".to_string() } else { s }
        };

        let form_edit_txt = {
            let s = t("did_form_edit");
            if s == "did_form_edit" { "Edit Selected Item:".to_string() } else { s }
        };
        let form_new_txt = {
            let s = t("did_form_new");
            if s == "did_form_new" { "New Item Form:".to_string() } else { s }
        };
        let lbl_fmt_txt = {
            let s = t("did_lbl_fmt");
            if s == "did_lbl_fmt" { "Format:".to_string() } else { s }
        };
        let lbl_len_txt = {
            let s = t("did_lbl_len");
            if s == "did_lbl_len" { "Length:".to_string() } else { s }
        };
        let btn_save_raw = {
            let s = t("did_btn_save");
            if s == "did_btn_save" { "Save/Update Config".to_string() } else { s }
        };
        let btn_del_raw = {
            let s = t("did_btn_del");
            if s == "did_btn_del" { "Delete Item".to_string() } else { s }
        };
        let btn_close_txt = {
            let s = t("dlg_close");
            if s == "dlg_close" {
                let s2 = t("pm_close_btn");
                if s2 == "pm_close_btn" { "Close".to_string() } else { s2 }
            } else {
                s
            }
        };

        // 2. 在 Rust 层动态拼接 Icon 呈现
        let tab_rw_txt = format!("✏️ {}", tab_rw_raw);
        let tab_ro_txt = format!("🔒 {}", tab_ro_raw);
        let btn_clear_new_txt = format!("＋ {}", btn_clear_new_raw);
        let btn_save_txt = format!("💾 {}", btn_save_raw);
        let btn_del_txt = format!("🗑 {}", btn_del_raw);

        Window::new(title_txt)
            .open(&mut is_open)
            .resizable(true)
            .collapsible(false)
            .min_width(860.0)
            .min_height(500.0)
            .default_size([880.0, 520.0])
            .default_pos([260.0, 160.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui
                        .selectable_label(self.active_tab == 0, tab_rw_txt)
                        .clicked()
                    {
                        self.active_tab = 0;
                        self.selected_key = None;
                    }
                    if ui
                        .selectable_label(self.active_tab == 1, tab_ro_txt)
                        .clicked()
                    {
                        self.active_tab = 1;
                        self.selected_key = None;
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(btn_clear_new_txt).clicked() {
                            self.selected_key = None;
                            self.edit_key = "new_did".to_string();
                            self.edit_did = "0xFD30".to_string();
                            self.edit_zh = "新数据项".to_string();
                            self.edit_en = "New Item".to_string();
                            self.edit_fmt = "hex".to_string();
                            self.edit_len = 4;
                        }
                    });
                });
                ui.separator();

                let target_rw = if self.active_tab == 0 { "RW" } else { "RO" };

                // 排序显示表格
                let mut did_keys: Vec<String> = cfg.dids.keys().cloned().collect();
                did_keys.sort_by_key(|k| cfg.dids.get(k).map(|d| d.did).unwrap_or(0));

                ScrollArea::vertical()
                    .id_source("did_mgr_table_scroll")
                    .max_height(280.0)
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        egui::Grid::new("did_mgr_table")
                            .striped(true)
                            .num_columns(6)
                            .spacing([14.0, 6.0])
                            .show(ui, |ui| {
                                ui.label(RichText::new(th_key_txt).strong());
                                ui.label(RichText::new(th_did_txt).strong());
                                ui.label(RichText::new(th_zh_name_txt).strong());
                                ui.label(RichText::new(th_en_name_txt).strong());
                                ui.label(RichText::new(th_fmt_txt).strong());
                                ui.label(RichText::new(th_len_txt).strong());
                                ui.end_row();

                                for k in did_keys {
                                    if let Some(v) = cfg.dids.get(&k) {
                                        if v.rw.to_uppercase() == target_rw {
                                            let is_sel = self.selected_key.as_ref() == Some(&k);
                                            let zh = v.name_i18n.get("zh").cloned().unwrap_or_else(|| v.name.clone());
                                            let en = v.name_i18n.get("en").cloned().unwrap_or_else(|| v.name.clone());

                                            if ui.selectable_label(is_sel, &k).clicked() {
                                                self.selected_key = Some(k.clone());
                                                self.edit_key = k.clone();
                                                self.edit_did = format!("0x{:04X}", v.did);
                                                self.edit_zh = zh.clone();
                                                self.edit_en = en.clone();
                                                self.edit_fmt = v.fmt.clone();
                                                self.edit_len = v.len;
                                            }
                                            ui.label(format!("0x{:04X}", v.did));
                                            ui.label(&zh);
                                            ui.label(&en);
                                            ui.label(&v.fmt);
                                            ui.label(v.len.to_string());
                                            ui.end_row();
                                        }
                                    }
                                }
                            });
                    });

                ui.separator();

                // 底部编辑/新增栏
                ui.group(|ui| {
                    ui.set_width(ui.available_width());
                    let form_header = if self.selected_key.is_some() { form_edit_txt } else { form_new_txt };
                    ui.label(RichText::new(form_header).strong().color(Color32::from_rgb(0, 188, 212)));
                    ui.add_space(4.0);

                    ui.horizontal(|ui| {
                        ui.label("Key:");
                        ui.add(egui::TextEdit::singleline(&mut self.edit_key).desired_width(110.0));
                        ui.label("DID:");
                        ui.add(egui::TextEdit::singleline(&mut self.edit_did).desired_width(70.0));
                        ui.label("ZH:");
                        ui.add(egui::TextEdit::singleline(&mut self.edit_zh).desired_width(110.0));
                        ui.label("EN:");
                        ui.add(egui::TextEdit::singleline(&mut self.edit_en).desired_width(110.0));

                        ui.label(lbl_fmt_txt);
                        egui::ComboBox::from_id_source("did_fmt_cb")
                            .selected_text(&self.edit_fmt)
                            .width(80.0)
                            .show_ui(ui, |ui| {
                                for f in &["ascii", "hex", "version", "utc", "date", "baud", "dec"] {
                                    ui.selectable_value(&mut self.edit_fmt, f.to_string(), *f);
                                }
                            });

                        ui.label(lbl_len_txt);
                        ui.add(egui::DragValue::new(&mut self.edit_len).clamp_range(1..=64));
                    });

                    ui.add_space(8.0);

                    ui.horizontal(|ui| {
                        let save_btn = egui::Button::new(
                            RichText::new(btn_save_txt).strong().color(Color32::WHITE),
                        ).fill(Color32::from_rgb(0, 122, 204));

                        if ui.add(save_btn).clicked() {
                            let did_val = if self.edit_did.to_lowercase().starts_with("0x") {
                                u16::from_str_radix(&self.edit_did[2..], 16).unwrap_or(0)
                            } else {
                                self.edit_did.parse::<u16>().unwrap_or(0)
                            };

                            if !self.edit_key.trim().is_empty() && did_val > 0 {
                                let mut i18n_map = HashMap::new();
                                i18n_map.insert("zh".to_string(), self.edit_zh.clone());
                                i18n_map.insert("en".to_string(), self.edit_en.clone());

                                // 1. 保存到配置字典中
                                cfg.dids.insert(
                                    self.edit_key.clone(),
                                    DidConfig {
                                        did: did_val,
                                        name: self.edit_zh.clone(),
                                        name_i18n: i18n_map,
                                        fmt: self.edit_fmt.clone(),
                                        len: self.edit_len,
                                        rw: target_rw.to_string(),
                                    },
                                );

                                // 2. 同步写回 i18n 语言文件
                                let did_i18n_key = format!("did_{}", self.edit_key);
                                sync_did_i18n(&did_i18n_key, &format!("{}:", self.edit_zh), &format!("{}:", self.edit_en));

                                // 3. 持久化到 config.dat
                                save_config_dat(cfg);
                                self.selected_key = Some(self.edit_key.clone());
                            }
                        }

                        let del_btn = egui::Button::new(
                            RichText::new(btn_del_txt).strong().color(Color32::WHITE),
                        ).fill(Color32::from_rgb(217, 83, 79));

                        if ui.add(del_btn).clicked() {
                            if let Some(sel) = &self.selected_key {
                                cfg.dids.remove(sel);
                                let did_i18n_key = format!("did_{}", sel);
                                remove_did_i18n(&did_i18n_key);

                                save_config_dat(cfg);
                                self.selected_key = None;
                                self.edit_key.clear();
                            }
                        }

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button(btn_close_txt).clicked() {
                                close_requested = true;
                            }
                        });
                    });
                });
            });

        if close_requested || !is_open {
            self.open = false;
        }
    }
}