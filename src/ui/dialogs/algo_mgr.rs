// src/ui/dialogs/algo_mgr.rs
use std::env;
use std::path::PathBuf;

use eframe::egui::{self, Align2, Color32, RichText, Stroke, Window};

use crate::config::AppConfig;
use crate::i18n::t;
use crate::ui::theme::{hex_to_color, CURRENT_THEME};

pub struct AlgoManagerDialog {
    pub open: bool,
    selected_algo: String,
    edit_name: String,
    edit_path: String,
    is_editing: bool,
}

impl Default for AlgoManagerDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl AlgoManagerDialog {
    pub fn new() -> Self {
        Self {
            open: false,
            selected_algo: String::new(),
            edit_name: String::new(),
            edit_path: String::new(),
            is_editing: false,
        }
    }

    fn sync_selection(&mut self, cfg: &AppConfig) {
        if !cfg.security_algos.contains_key(&self.selected_algo) {
            if let Some(first_key) = cfg.security_algos.keys().next() {
                self.selected_algo = first_key.clone();
            } else {
                self.selected_algo.clear();
            }
        }

        if let Some(path) = cfg.security_algos.get(&self.selected_algo) {
            self.edit_name = self.selected_algo.clone();
            self.edit_path = path.clone();
        } else {
            self.edit_name.clear();
            self.edit_path.clear();
        }
    }

    pub fn show(&mut self, ctx: &egui::Context, cfg: &mut AppConfig) {
        if !self.open {
            self.is_editing = false;
            return;
        }

        if self.selected_algo.is_empty() && !cfg.security_algos.is_empty() {
            self.sync_selection(cfg);
        }

        let theme = CURRENT_THEME.read().unwrap().clone();

        // 统一从 i18n 获取
        let title_txt = {
            let s = t("menu_config_algo_mgr");
            if s == "menu_config_algo_mgr" { "Security Algorithm Manager".to_string() } else { s }
        };
        let algo_list_txt = {
            let s = t("algo_list_title");
            if s == "algo_list_title" { "Algorithms:".to_string() } else { s }
        };
        let algo_params_txt = {
            let s = t("algo_params_title");
            if s == "algo_params_title" { "Parameters".to_string() } else { s }
        };
        let algo_name_txt = {
            let s = t("algo_name");
            if s == "algo_name" { "Name".to_string() } else { s }
        };
        let algo_path_txt = {
            let s = t("algo_file_path");
            if s == "algo_file_path" { "File Path".to_string() } else { s }
        };
        let hint_fmt_txt = {
            let s = t("algo_hint_fmt");
            if s == "algo_hint_fmt" { "Hint: Supports external .dll security plugins".to_string() } else { s }
        };

        // 与 product_mgr 统一使用 btn_new 与 btn_del
        let add_txt = {
            let s = t("btn_new");
            if s == "btn_new" {
                let fallback = t("dlg_add");
                if fallback == "dlg_add" { "New".to_string() } else { fallback }
            } else {
                s
            }
        };
        let del_txt = {
            let s = t("btn_del");
            if s == "btn_del" {
                let fallback = t("dlg_del");
                if fallback == "dlg_del" { "Delete".to_string() } else { fallback }
            } else {
                s
            }
        };

        let save_txt = {
            let s = t("dlg_save");
            if s == "dlg_save" { "Save".to_string() } else { s }
        };
        let edit_txt = {
            let s = t("dlg_edit");
            if s == "dlg_edit" { "Edit".to_string() } else { s }
        };
        let cancel_txt = {
            let s = t("dlg_cancel");
            if s == "dlg_cancel" { "Cancel".to_string() } else { s }
        };
        let close_txt = {
            let s = t("dlg_close");
            if s == "dlg_close" { "Close".to_string() } else { s }
        };
        let browse_txt = {
            let s = t("browse_btn");
            if s == "browse_btn" { "Browse...".to_string() } else { s }
        };

        // 局部标记，解耦借用
        let mut close_dialog = false;
        let mut need_sync_after_closure = false;
        let mut window_still_open = true;

        Window::new(title_txt)
            .open(&mut window_still_open)
            .collapsible(false)
            .resizable(false)
            .movable(true)
            .default_size([580.0, 340.0])
            .pivot(Align2::CENTER_CENTER)
            .default_pos(ctx.screen_rect().center())
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    // ==================== 左侧：算法列表 ====================
                    ui.vertical(|ui| {
                        ui.set_width(170.0);
                        ui.label(RichText::new(algo_list_txt).strong());
                        ui.add_space(4.0);

                        egui::Frame::none()
                            .fill(ui.visuals().extreme_bg_color)
                            .stroke(Stroke::new(1.0_f32, hex_to_color(&theme.border)))
                            .rounding(3.0)
                            .show(ui, |ui| {
                                egui::ScrollArea::vertical()
                                    .max_height(180.0)
                                    .auto_shrink([false; 2])
                                    .show(ui, |ui| {
                                        let mut keys: Vec<String> = cfg.security_algos.keys().cloned().collect();
                                        keys.sort();

                                        for k in keys {
                                            let is_selected = self.selected_algo == k;
                                            let resp = ui.selectable_label(is_selected, &k);
                                            if resp.clicked() {
                                                self.selected_algo = k.clone();
                                                self.is_editing = false;
                                                if let Some(path) = cfg.security_algos.get(&k) {
                                                    self.edit_name = k;
                                                    self.edit_path = path.clone();
                                                }
                                            }
                                        }
                                    });
                            });

                        ui.add_space(8.0);

                        // 与产品管理器严格对齐
                        ui.horizontal(|ui| {
                            let btn_w = 72.0;
                            if ui.add_sized([btn_w, 24.0], egui::Button::new(format!("+ {}", add_txt))).clicked() {
                                let mut idx = 1;
                                let mut new_name = format!("Custom_Algo_{}", idx);
                                while cfg.security_algos.contains_key(&new_name) {
                                    idx += 1;
                                    new_name = format!("Custom_Algo_{}", idx);
                                }
                                cfg.security_algos.insert(new_name.clone(), "algos/rc4.dll".to_string());
                                self.selected_algo = new_name.clone();
                                self.edit_name = new_name;
                                self.edit_path = "algos/rc4.dll".to_string();
                                self.is_editing = true;
                            }

                            let can_delete = !self.selected_algo.is_empty() && cfg.security_algos.len() > 1;
                            if ui.add_enabled(can_delete, egui::Button::new(format!("- {}", del_txt)).min_size(egui::vec2(btn_w, 24.0))).clicked() {
                                cfg.security_algos.remove(&self.selected_algo);
                                self.selected_algo.clear();
                                self.is_editing = false;
                                need_sync_after_closure = true;
                            }
                        });
                    });

                    ui.separator();

                    // ==================== 右侧：参数与编辑 ====================
                    ui.vertical(|ui| {
                        ui.set_width(ui.available_width());

                        ui.label(RichText::new(algo_params_txt).strong().size(14.0));
                        ui.add_space(8.0);

                        let label_w = 64.0;
                        let input_w = 220.0;
                        let row_h = 24.0;

                        // 1. 算法名称
                        ui.horizontal(|ui| {
                            ui.set_min_width(label_w);
                            ui.label(format!("{}:", algo_name_txt));

                            if self.is_editing {
                                ui.add_sized([input_w, row_h], egui::TextEdit::singleline(&mut self.edit_name));
                            } else {
                                render_readonly_box(ui, &self.edit_name, input_w, row_h);
                            }
                        });

                        ui.add_space(6.0);

                        // 2. 文件路径及浏览
                        ui.horizontal(|ui| {
                            ui.set_min_width(label_w);
                            ui.label(format!("{}:", algo_path_txt));

                            if self.is_editing {
                                ui.add_sized([input_w - 60.0, row_h], egui::TextEdit::singleline(&mut self.edit_path));

                                if ui.add_sized([52.0, row_h], egui::Button::new(browse_txt)).clicked() {
                                    let start_dir = env::current_dir()
                                        .map(|p| p.join("algos"))
                                        .unwrap_or_else(|_| PathBuf::from("algos"));

                                    if let Some(path) = rfd::FileDialog::new()
                                        .set_directory(&start_dir)
                                        .add_filter("Algo Plugin (*.dll; *.py)", &["dll", "py"])
                                        .pick_file()
                                    {
                                        let rel_path = if let Ok(cur_dir) = env::current_dir() {
                                            match path.strip_prefix(&cur_dir) {
                                                Ok(rel) => rel.to_string_lossy().replace('\\', "/"),
                                                Err(_) => path.to_string_lossy().replace('\\', "/"),
                                            }
                                        } else {
                                            path.to_string_lossy().replace('\\', "/")
                                        };
                                        self.edit_path = rel_path;
                                    }
                                }
                            } else {
                                render_readonly_box(ui, &self.edit_path, input_w, row_h);
                            }
                        });

                        ui.add_space(10.0);
                        ui.label(
                            RichText::new(hint_fmt_txt)
                                .size(11.0)
                                .color(Color32::from_rgb(140, 140, 140)),
                        );

                        ui.add_space(20.0);

                        // ==================== 底部动作按钮 ====================
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let btn_size = egui::vec2(64.0, 25.0);

                            // 编辑态显示 Cancel，非编辑态显示 Close
                            let exit_label = if self.is_editing { cancel_txt } else { close_txt };
                            let exit_btn = egui::Button::new(exit_label).min_size(btn_size);

                            if ui.add(exit_btn).clicked() {
                                if self.is_editing {
                                    self.is_editing = false;
                                    need_sync_after_closure = true;
                                } else {
                                    close_dialog = true;
                                }
                            }

                            ui.add_space(8.0);

                            if self.is_editing {
                                let save_btn = egui::Button::new(
                                    RichText::new(save_txt)
                                        .strong()
                                        .color(Color32::WHITE),
                                )
                                .fill(hex_to_color(&theme.primary_btn))
                                .min_size(btn_size);

                                if ui.add(save_btn).clicked() {
                                    let new_k = self.edit_name.trim().to_string();
                                    let new_v = self.edit_path.trim().to_string();

                                    if !new_k.is_empty() && !new_v.is_empty() {
                                        if new_k != self.selected_algo {
                                            cfg.security_algos.remove(&self.selected_algo);
                                        }
                                        cfg.security_algos.insert(new_k.clone(), new_v);
                                        self.selected_algo = new_k;
                                        self.is_editing = false;
                                    }
                                }
                            } else {
                                let edit_btn = egui::Button::new(edit_txt).min_size(btn_size);
                                let can_edit = !self.selected_algo.is_empty();
                                if ui.add_enabled(can_edit, edit_btn).clicked() {
                                    self.is_editing = true;
                                }
                            }
                        });
                    });
                });
            });

        // 闭包执行结束，借用完全释放，在此集中处理状态与关闭
        if !window_still_open || close_dialog {
            self.open = false;
        }

        if need_sync_after_closure {
            self.sync_selection(cfg);
        }
    }
}

fn render_readonly_box(ui: &mut egui::Ui, text: &str, width: f32, height: f32) {
    let theme = CURRENT_THEME.read().unwrap().clone();
    egui::Frame::none()
        .fill(hex_to_color(&theme.readonly_bg))
        .stroke(Stroke::new(1.0_f32, hex_to_color(&theme.readonly_border)))
        .rounding(3.0)
        .inner_margin(egui::Margin::symmetric(6.0, 2.0))
        .show(ui, |ui| {
            ui.set_min_size(egui::vec2(width - 12.0, height - 4.0));
            ui.label(
                RichText::new(text)
                    .color(hex_to_color(&theme.readonly_text))
                    .size(12.5),
            );
        });
}