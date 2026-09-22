// src/ui/right_panel.rs
use crate::core::crypto::sha256_hex;
use crate::core::exporter::CanLogItem;
use crate::i18n::t;
use crate::ui::components::draw_vscode_h_splitter;
use crate::ui::theme::{hex_to_color, CURRENT_THEME};
use egui::{vec2, Align2, Color32, FontId, Rect, RichText, ScrollArea, Sense, Stroke, Ui};
use std::fs;
use std::path::Path;
use std::time::Instant;

#[derive(Clone, Debug)]
pub struct RightPanelState {
    pub bin_path: String,
    pub cached_path: String,
    pub cached_crc: String,
    pub cached_sha256: String,
    pub sig_mode: usize,
    pub sig_path: String,
    pub auto_reboot: bool,
    pub progress_percent: f32,
    pub progress_text: String,
    pub flash_logs: Vec<(String, String)>,
    pub can_logs: Vec<CanLogItem>,
    pub filter_mode_idx: usize,
    pub can_monitoring_paused: bool,
    pub firmware_card_height: f32,
    pub flash_log_height: f32,
    pub flash_start_time: Option<Instant>,
    pub total_elapsed_secs: Option<f32>,
}

impl Default for RightPanelState {
    fn default() -> Self {
        Self {
            bin_path: String::new(),
            cached_path: String::new(),
            cached_crc: "-".to_string(),
            cached_sha256: "-".to_string(),
            sig_mode: 0,
            sig_path: String::new(),
            auto_reboot: true,
            progress_percent: 0.0,
            progress_text: String::new(),
            flash_logs: Vec::new(),
            can_logs: Vec::new(),
            filter_mode_idx: 0,
            can_monitoring_paused: false,
            firmware_card_height: 195.0,
            flash_log_height: 220.0,
            flash_start_time: None,
            total_elapsed_secs: None,
        }
    }
}

pub fn render(
    ui: &mut Ui,
    state: &mut RightPanelState,
    is_flashing: bool,
    is_connected: bool,
    is_querying_dids: bool,
    verify_method: &str,
    on_start_flash: impl FnOnce(),
    on_stop_flash: impl FnOnce(),
    on_manual_reboot: impl FnOnce(),
    on_open_filter_dialog: impl FnOnce(),
    on_export_flash_log: impl FnOnce(),
    on_export_can_log: impl FnOnce(),
) {
    let theme = CURRENT_THEME.read().unwrap().clone();
    let border_col = hex_to_color(&theme.border);
    let panel_bg = hex_to_color(&theme.window_bg);
    let log_embed_bg = hex_to_color(&theme.log_embed_bg);
    let accent_col = hex_to_color(&theme.accent);
    let text_normal_col = hex_to_color(&theme.text_normal);

    // 拖放文件检测
    ui.ctx().input(|i| {
        if !i.raw.dropped_files.is_empty() {
            for file in &i.raw.dropped_files {
                if let Some(ref path) = file.path {
                    let ext = path
                        .extension()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_lowercase();
                    if ext == "bin" {
                        state.bin_path = path.to_string_lossy().to_string();
                    } else if ext == "sig" {
                        state.sig_path = path.to_string_lossy().to_string();
                        state.sig_mode = 1;
                    }
                }
            }
        }
    });

    // 固件哈希缓存
    let cur_trimmed = state.bin_path.trim().to_string();
    if cur_trimmed != state.cached_path {
        state.cached_path = cur_trimmed.clone();
        if !cur_trimmed.is_empty() {
            if let Ok(bytes) = fs::read(&cur_trimmed) {
                let mut hasher = crc32fast::Hasher::new();
                hasher.update(&bytes);
                state.cached_crc = format!("0x{:08X}", hasher.finalize());
                state.cached_sha256 = sha256_hex(&bytes);
            } else {
                state.cached_crc = "-".to_string();
                state.cached_sha256 = "-".to_string();
            }
        } else {
            state.cached_crc = "-".to_string();
            state.cached_sha256 = "-".to_string();
        }
    }

    // 刷写完成统计总耗时（仅当处于刷写中且进度达到100%时固化）
    if is_flashing && state.progress_percent >= 99.99 {
        if state.total_elapsed_secs.is_none() {
            if let Some(st) = state.flash_start_time {
                state.total_elapsed_secs = Some(st.elapsed().as_secs_f32());
            }
        }
    }

    ui.spacing_mut().item_spacing.y = 1.0_f32;

    ui.vertical(|ui| {
        // ==================== 1. 上部：固件与刷写控制卡片 ====================
        egui::Frame::none()
            .fill(panel_bg)
            .stroke(Stroke::new(1.0_f32, border_col))
            .rounding(4.0)
            .inner_margin(egui::Margin::same(8.0))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.set_height(state.firmware_card_height);

                ScrollArea::vertical()
                    .id_source("firmware_card_scroll_area")
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(t("firmware_file_title"))
                                .strong()
                                .size(13.0)
                                .color(accent_col),
                        );
                        ui.add_space(3.0);

                        ui.horizontal(|ui| {
                            let text_edit_width = ui.available_width() - 85.0;
                            ui.add_sized(
                                [text_edit_width, 24.0],
                                egui::TextEdit::singleline(&mut state.bin_path)
                                    .hint_text(t("drag_bin_hint")),
                            );

                            if ui
                                .add_sized(
                                    [75.0, 24.0],
                                    egui::Button::new(format!("📁 {}", t("browse"))),
                                )
                                .clicked()
                            {
                                if let Some(path) = rfd::FileDialog::new()
                                    .add_filter("Binary (*.bin)", &["bin"])
                                    .pick_file()
                                {
                                    state.bin_path = path.to_string_lossy().to_string();
                                }
                            }
                        });

                        ui.add_space(3.0);

                        let (size_str, ctime_str, mtime_str) = if !state.bin_path.trim().is_empty()
                        {
                            let p = Path::new(state.bin_path.trim());
                            if let Ok(meta) = fs::metadata(p) {
                                let sz = meta.len();
                                let sz_disp = if sz > 1024 * 1024 {
                                    format!("{:.2} MB ({} B)", sz as f64 / 1048576.0, sz)
                                } else {
                                    format!("{:.2} KB ({} B)", sz as f64 / 1024.0, sz)
                                };
                                let ctime = meta
                                    .created()
                                    .ok()
                                    .and_then(|t| {
                                        chrono::DateTime::<chrono::Local>::from(t)
                                            .format("%Y-%m-%d %H:%M:%S")
                                            .to_string()
                                            .into()
                                    })
                                    .unwrap_or_else(|| "-".to_string());
                                let mtime = meta
                                    .modified()
                                    .ok()
                                    .and_then(|t| {
                                        chrono::DateTime::<chrono::Local>::from(t)
                                            .format("%Y-%m-%d %H:%M:%S")
                                            .to_string()
                                            .into()
                                    })
                                    .unwrap_or_else(|| "-".to_string());
                                (sz_disp, ctime, mtime)
                            } else {
                                ("-".into(), "-".into(), "-".into())
                            }
                        } else {
                            ("-".into(), "-".into(), "-".into())
                        };

                        egui::Grid::new("firmware_meta_grid")
                            .num_columns(3)
                            .spacing([20.0, 4.0])
                            .show(ui, |ui| {
                                ui.label(format!("{}: {}", t("firmware_size"), size_str));
                                ui.label(format!("CRC32: {}", state.cached_crc));
                                ui.label(format!("SHA256: {}", state.cached_sha256));
                                ui.end_row();

                                ui.label(format!("{}: {}", t("firmware_create_time"), ctime_str));
                                ui.label(format!("{}: {}", t("firmware_modify_time"), mtime_str));
                                ui.label("");
                                ui.end_row();
                            });

                        let ver_lower = verify_method.to_lowercase();
                        if ver_lower.contains("signature")
                            || ver_lower.contains("sign")
                            || ver_lower.contains("签名")
                        {
                            ui.add_space(3.0);
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(format!("🔏 {}", t("sig_mode_label")))
                                        .strong()
                                        .color(accent_col),
                                );
                                ui.radio_value(&mut state.sig_mode, 0, t("sig_mode_embedded"));
                                ui.radio_value(&mut state.sig_mode, 1, t("sig_mode_detached"));
                            });
                            if state.sig_mode == 1 {
                                ui.add_space(2.0);
                                ui.horizontal(|ui| {
                                    let text_w = ui.available_width() - 95.0;
                                    ui.add_sized(
                                        [text_w, 24.0],
                                        egui::TextEdit::singleline(&mut state.sig_path)
                                            .hint_text(t("sig_file_hint")),
                                    );
                                    if ui
                                        .add_sized(
                                            [85.0, 24.0],
                                            egui::Button::new(format!(
                                                "🔏 {}",
                                                t("sig_browse_btn")
                                            )),
                                        )
                                        .clicked()
                                    {
                                        if let Some(path) = rfd::FileDialog::new()
                                            .add_filter("Signature", &["sig", "bin"])
                                            .pick_file()
                                        {
                                            state.sig_path = path.to_string_lossy().to_string();
                                        }
                                    }
                                });
                            }
                        }

                        ui.add_space(4.0);
                        ui.separator();
                        ui.add_space(3.0);

                        ui.checkbox(&mut state.auto_reboot, t("auto_reboot_tip"));
                        ui.add_space(4.0);

                        ui.horizontal(|ui| {
                            let btn_w = (ui.available_width() - 10.0) / 2.0;
                            let btn_h = 28.0;

                            if !is_flashing {
                                let has_bin = !state.bin_path.trim().is_empty();
                                let has_sig =
                                    state.sig_mode == 0 || !state.sig_path.trim().is_empty();
                                let can_start =
                                    is_connected && !is_querying_dids && has_bin && has_sig;

                                let mut start_btn = egui::Button::new(
                                    if can_start {
                                        RichText::new(format!("▶ {}", t("start_flash")))
                                            .strong()
                                            .color(text_normal_col)
                                    } else {
                                        RichText::new(format!("▶ {}", t("start_flash")))
                                    },
                                )
                                .min_size(vec2(btn_w, btn_h))
                                .rounding(4.0);

                                if can_start {
                                    start_btn = start_btn.fill(hex_to_color(&theme.primary_btn));
                                }

                                let start_resp = ui.add_enabled(can_start, start_btn);

                                if !can_start {
                                    let hint = if !is_connected {
                                        "Connect hardware first"
                                    } else if is_querying_dids {
                                        "Querying DIDs..."
                                    } else if !has_bin {
                                        "Select firmware file first"
                                    } else {
                                        "Select signature file first"
                                    };
                                    start_resp.on_disabled_hover_text(hint);
                                } else if start_resp.clicked() {
                                    state.flash_start_time = Some(Instant::now());
                                    state.total_elapsed_secs = None;
                                    state.progress_percent = 0.0;
                                    state.progress_text.clear();
                                    on_start_flash();
                                }
                            } else {
                                let stop_btn = egui::Button::new(
                                    RichText::new(format!("⏹ {}", t("stop_flash")))
                                        .strong()
                                        .color(text_normal_col),
                                )
                                .fill(hex_to_color(&theme.danger_btn))
                                .rounding(4.0);
                                if ui.add_sized([btn_w, btn_h], stop_btn).clicked() {
                                    on_stop_flash();
                                }
                            }

                            let can_reboot = is_connected && !is_flashing && !state.auto_reboot;
                            let reboot_btn =
                                egui::Button::new(format!("🔄 {}", t("manual_reboot")))
                                    .min_size(vec2(btn_w, btn_h));
                            let reboot_resp = ui.add_enabled(can_reboot, reboot_btn);
                            if !can_reboot {
                                let hint = if !is_connected {
                                    "Connect hardware first"
                                } else if is_flashing {
                                    "Flashing"
                                } else {
                                    "Auto reboot is enabled"
                                };
                                reboot_resp.on_disabled_hover_text(hint);
                            } else if reboot_resp.clicked() {
                                on_manual_reboot();
                            }
                        });

                        ui.add_space(5.0);

                        let clean_text = state
                            .progress_text
                            .trim_start_matches("100.0%")
                            .trim_start_matches("100%")
                            .trim();

                        let progress_display = if state.progress_percent >= 99.99 {
                            let duration_str = if let Some(secs) = state.total_elapsed_secs {
                                format!(" in {:.1}s", secs)
                            } else if let Some(st) = state.flash_start_time {
                                format!(" in {:.1}s", st.elapsed().as_secs_f32())
                            } else {
                                String::new()
                            };
                            let base_desc = if clean_text.is_empty() {
                                "Done"
                            } else {
                                clean_text
                            };
                            format!("100% {}{}", base_desc, duration_str)
                        } else if clean_text.is_empty() {
                            format!("{:.0}% 00:00 Sent 0 Bytes", state.progress_percent)
                        } else {
                            format!("{:.0}% {}", state.progress_percent, clean_text)
                        };

                        if state.progress_percent > 0.001 || is_flashing {
                            let (rect, _) = ui.allocate_exact_size(
                                vec2(ui.available_width(), 20.0_f32),
                                Sense::hover(),
                            );
                            let font_id = FontId::monospace(11.5_f32);
                            let text_pos = rect.center();

                            let track_color = hex_to_color(&theme.progress_track_bg);
                            let fill_color = hex_to_color(&theme.progress_fill);
                            ui.painter().rect_filled(rect, 4.0_f32, track_color);

                            let fill_w =
                                rect.width() * (state.progress_percent / 100.0).clamp(0.0, 1.0);
                            if fill_w > 0.0 {
                                let fill_rect =
                                    Rect::from_min_size(rect.min, vec2(fill_w, rect.height()));
                                ui.painter().rect_filled(fill_rect, 4.0_f32, fill_color);
                            }

                            let unfill_rect = Rect::from_min_size(
                                rect.min + vec2(fill_w, 0.0),
                                vec2(rect.width() - fill_w, rect.height()),
                            );
                            if unfill_rect.width() > 0.0 {
                                let mut painter_unfill = ui.painter().clone();
                                painter_unfill.set_clip_rect(unfill_rect);
                                painter_unfill.text(
                                    text_pos,
                                    Align2::CENTER_CENTER,
                                    &progress_display,
                                    font_id.clone(),
                                    text_normal_col,
                                );
                            }

                            if fill_w > 0.0 {
                                let fill_rect =
                                    Rect::from_min_size(rect.min, vec2(fill_w, rect.height()));
                                let mut painter_fill = ui.painter().clone();
                                painter_fill.set_clip_rect(fill_rect);
                                painter_fill.text(
                                    text_pos,
                                    Align2::CENTER_CENTER,
                                    &progress_display,
                                    font_id,
                                    Color32::WHITE,
                                );
                            }
                        } else {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(progress_display)
                                        .size(12.0)
                                        .color(hex_to_color(&theme.text_subtle)),
                                );
                            });
                        }
                    });
            });

        // 垂直分割线 1
        let split_1 = draw_vscode_h_splitter(ui, 3.0);
        if split_1.dragged() {
            let dy = split_1.drag_delta().y;
            state.firmware_card_height = (state.firmware_card_height + dy).clamp(120.0, 400.0);
        }

        // ==================== 2. 中部：刷写日志卡片 ====================
        egui::Frame::none()
            .fill(panel_bg)
            .stroke(Stroke::new(1.0_f32, border_col))
            .rounding(4.0)
            .inner_margin(egui::Margin::same(8.0))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("📝 {}", t("flash_log_title")))
                            .strong()
                            .color(accent_col),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(format!("🗑 {}", t("clear_log"))).clicked() {
                            state.flash_logs.clear();
                        }
                        if ui.button(format!("💾 {}", t("export_log"))).clicked() {
                            on_export_flash_log();
                        }
                    });
                });
                ui.add_space(3.0);

                egui::Frame::none()
                    .fill(log_embed_bg)
                    .stroke(Stroke::new(1.0_f32, border_col))
                    .rounding(3.0)
                    .inner_margin(egui::Margin::same(4.0))
                    .show(ui, |ui| {
                        ScrollArea::vertical()
                            .id_source("flash_log_scroll_unique")
                            .max_height(state.flash_log_height)
                            .min_scrolled_height(state.flash_log_height)
                            .auto_shrink([false; 2])
                            .stick_to_bottom(true)
                            .show(ui, |ui| {
                                ui.set_clip_rect(ui.clip_rect());
                                ui.set_width(ui.available_width());
                                for (line, level) in &state.flash_logs {
                                    let col = match level.as_str() {
                                        "error" => hex_to_color(&theme.error_text),
                                        "warn" => hex_to_color(&theme.warn_text),
                                        _ => hex_to_color(&theme.info_text),
                                    };
                                    ui.label(RichText::new(line).color(col).monospace().size(12.0));
                                }
                            });
                    });
            });

        // 垂直分割线 2
        let split_2 = draw_vscode_h_splitter(ui, 3.0);
        if split_2.dragged() {
            let dy = split_2.drag_delta().y;
            state.flash_log_height = (state.flash_log_height + dy).clamp(80.0, 520.0);
        }

        // ==================== 3. 底部：CAN 报文监控卡片 ====================
        let remain_card_h = ui.available_height().max(100.0);

        egui::Frame::none()
            .fill(panel_bg)
            .stroke(Stroke::new(1.0_f32, border_col))
            .rounding(4.0)
            .inner_margin(egui::Margin::same(8.0))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.set_min_height(remain_card_h - 16.0);

                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("📡 {}", t("can_monitor_title")))
                            .strong()
                            .color(accent_col),
                    );
                    ui.label(t("filter"));

                    let filter_modes = [t("filter_none"), t("filter_uds_only"), t("filter_custom")];
                    egui::ComboBox::from_id_source("cb_can_filter_mode_unique")
                        .selected_text(&filter_modes[state.filter_mode_idx.min(2)])
                        .show_ui(ui, |ui| {
                            for (idx, name) in filter_modes.iter().enumerate() {
                                ui.selectable_value(&mut state.filter_mode_idx, idx, name);
                            }
                        });

                    if state.filter_mode_idx == 2 && ui.button(t("filter_settings")).clicked() {
                        on_open_filter_dialog();
                    }

                    let sniff_txt = if state.can_monitoring_paused {
                        format!("▶ {}", t("resume_sniff"))
                    } else {
                        format!("⏸ {}", t("pause_sniff"))
                    };
                    if ui.button(sniff_txt).clicked() {
                        state.can_monitoring_paused = !state.can_monitoring_paused;
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(format!("🗑 {}", t("clear"))).clicked() {
                            state.can_logs.clear();
                        }
                        if ui.button(format!("💾 {}", t("export"))).clicked() {
                            on_export_can_log();
                        }
                    });
                });
                ui.add_space(3.0);

                let scroll_h = (ui.available_height() - 4.0).max(60.0);
                egui::Frame::none()
                    .fill(log_embed_bg)
                    .stroke(Stroke::new(1.0_f32, border_col))
                    .rounding(3.0)
                    .inner_margin(egui::Margin::same(4.0))
                    .show(ui, |ui| {
                        ScrollArea::vertical()
                            .id_source("can_log_scroll_unique")
                            .max_height(scroll_h)
                            .min_scrolled_height(scroll_h)
                            .auto_shrink([false; 2])
                            .stick_to_bottom(true)
                            .show(ui, |ui| {
                                ui.set_clip_rect(ui.clip_rect());
                                ui.set_width(ui.available_width());
                                for item in &state.can_logs {
                                    let col = if item.is_rx {
                                        hex_to_color(&theme.can_rx_text)
                                    } else {
                                        hex_to_color(&theme.can_tx_text)
                                    };
                                    ui.label(
                                        RichText::new(&item.formatted)
                                            .color(col)
                                            .monospace()
                                            .size(12.0),
                                    );
                                }
                            });
                    });
            });
    });
}