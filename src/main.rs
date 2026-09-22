// src/main.rs
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod config;
mod core;
mod i18n;
mod ui;

use app::{CanController, DidController, FlashController};
use chrono::Local;
use config::{load_config_dat, save_config_dat, AppConfig, APP_VERSION, CUSTOM_PRODUCT_ID};
use core::can::CanAdapter;
use core::exporter::{CanLogItem, LogExporter};
use core::flash_engine::{FlashEngine, FlashEvent};
use crossbeam_channel::{unbounded, Receiver, Sender};
use eframe::egui::{
    self, Align2, CentralPanel, Color32, RichText, SidePanel, TopBottomPanel, Window,
};
use i18n::{get_language, load_locales, set_language, t, t_fmt};
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use ui::components::draw_vscode_v_splitter;
use ui::dialogs::{
    about::AboutDialog, algo_mgr::AlgoManagerDialog, did_mgr::DidManagerDialog,
    filter_dialog::CustomFilterDialog, product_mgr::ProductManagerDialog,
    security_cfg::SecurityConfigDialog,
};
use ui::left_panel::{render as render_left, LeftPanelState};
use ui::right_panel::{render as render_right, RightPanelState};
use ui::theme::{hex_to_color, CURRENT_THEME};

pub enum BusEvent {
    CanMsg {
        id: u32,
        is_ext: bool,
        is_rx: bool,
        formatted: String,
    },
    FlashLog(String, String),
    DidResult(String, String),
    DidQueryFinished,
    WriteNotice {
        success: bool,
        msg: String,
    },
}

pub struct UdsApp {
    cfg: AppConfig,
    saved_cfg: AppConfig,
    show_unsaved_dialog: bool,
    force_close: bool,
    show_saved_toast_timer: f32,
    is_querying_dids: bool,
    notice_dialog: Option<(bool, String)>,
    init_titlebar_ticks: u8,

    theme_mode: usize,
    is_connected: bool,
    conn_start_time: Option<Instant>,
    can_driver: Arc<Mutex<Option<Box<dyn CanAdapter>>>>,
    bus_running: Arc<AtomicBool>,
    bus_tx: Sender<BusEvent>,
    bus_rx: Receiver<BusEvent>,

    left_panel_width: f32,
    active_filter_ids: HashSet<u32>,
    left_state: LeftPanelState,
    right_state: RightPanelState,
    flash_engine: FlashEngine,
    flash_tx: Sender<FlashEvent>,
    flash_rx: Receiver<FlashEvent>,

    about_dlg: AboutDialog,
    sec_dlg: SecurityConfigDialog,
    prod_dlg: ProductManagerDialog,
    algo_dlg: AlgoManagerDialog,
    did_dlg: DidManagerDialog,
    filter_dlg: CustomFilterDialog,
}

pub fn get_app_title() -> String {
    let base_title = t("window_title");
    if base_title.is_empty() || base_title == "window_title" {
        format!("UDS Flashing Tool (Rust Edition) V{}", APP_VERSION)
    } else {
        format!("{} V{}", base_title, APP_VERSION)
    }
}

fn load_app_icon() -> Option<egui::IconData> {
    let ico_bytes = include_bytes!("../udstools.ico");
    if let Ok(img) = image::load_from_memory(ico_bytes) {
        let rgba = img.to_rgba8();
        let (width, height) = rgba.dimensions();
        Some(egui::IconData {
            rgba: rgba.into_raw(),
            width,
            height,
        })
    } else {
        None
    }
}

impl UdsApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        ui::theme::setup_custom_fonts(&cc.egui_ctx);
        load_locales();
        let cfg = load_config_dat();

        set_language(&cfg.language);

        let is_dark = if cfg.theme_mode == 2 {
            ui::theme::is_system_dark_mode()
        } else {
            cfg.theme_mode == 0
        };
        ui::theme::apply_theme(&cc.egui_ctx, is_dark);

        let (flash_tx, flash_rx) = unbounded();
        let (bus_tx, bus_rx) = unbounded();

        let mut left_state = LeftPanelState::new(&cfg);
        left_state.sync_from_product(&cfg);

        let mut right_state = RightPanelState::default();
        right_state.filter_mode_idx = cfg.default_filter_mode;
        right_state.auto_reboot = cfg.default_auto_reboot;

        Self {
            left_state,
            right_state,
            saved_cfg: cfg.clone(),
            theme_mode: cfg.theme_mode,
            cfg,
            show_unsaved_dialog: false,
            force_close: false,
            init_titlebar_ticks: 3,
            is_querying_dids: false,
            show_saved_toast_timer: 0.0,
            notice_dialog: None,
            is_connected: false,
            conn_start_time: None,
            can_driver: Arc::new(Mutex::new(None)),
            bus_running: Arc::new(AtomicBool::new(false)),
            bus_tx,
            bus_rx,
            left_panel_width: 420.0,
            active_filter_ids: HashSet::new(),
            flash_engine: FlashEngine::new(),
            flash_tx,
            flash_rx,
            about_dlg: AboutDialog::new(),
            sec_dlg: SecurityConfigDialog::new(),
            prod_dlg: ProductManagerDialog::new(),
            algo_dlg: AlgoManagerDialog::new(),
            did_dlg: DidManagerDialog::new(),
            filter_dlg: CustomFilterDialog::new(),
        }
    }

    fn try_save_config(&mut self) -> bool {
        self.cfg.language = get_language();
        self.cfg.default_product = self.left_state.selected_product.clone();
        self.cfg.default_interface = self.left_state.selected_interface.clone();
        self.cfg.default_channel = self.left_state.selected_channel.clone();
        self.cfg.default_baudrate = self.left_state.selected_baudrate.clone();
        self.cfg.default_flash_type = self.left_state.flash_type_key.clone();
        self.cfg.theme_mode = self.theme_mode;

        self.cfg.default_filter_mode = self.right_state.filter_mode_idx;
        self.cfg.default_auto_reboot = self.right_state.auto_reboot;

        if self.left_state.is_custom() {
            if let Some(custom_prod) = self.cfg.products.get_mut(CUSTOM_PRODUCT_ID) {
                custom_prod.txid = self.left_state.txid.clone();
                custom_prod.rxid = self.left_state.rxid.clone();
                custom_prod.baudrate = self.left_state.selected_baudrate.clone();
                if self.left_state.flash_type_key == "boot" {
                    custom_prod.boot_address = self.left_state.flash_address.clone();
                } else {
                    custom_prod.app_address = self.left_state.flash_address.clone();
                }
            }
        }

        if save_config_dat(&self.cfg) {
            self.saved_cfg = self.cfg.clone();
            self.show_saved_toast_timer = 2.0;
            true
        } else {
            false
        }
    }

    fn has_unsaved_changes(&self) -> bool {
        if !std::path::Path::new("config.dat").exists() {
            return true;
        }

        let mut cur = self.cfg.clone();
        cur.language = get_language();
        cur.default_product = self.left_state.selected_product.clone();
        cur.default_interface = self.left_state.selected_interface.clone();
        cur.default_channel = self.left_state.selected_channel.clone();
        cur.default_baudrate = self.left_state.selected_baudrate.clone();
        cur.default_flash_type = self.left_state.flash_type_key.clone();
        cur.theme_mode = self.theme_mode;

        cur.default_filter_mode = self.right_state.filter_mode_idx;
        cur.default_auto_reboot = self.right_state.auto_reboot;

        if self.left_state.is_custom() {
            if let Some(saved_custom) = self.saved_cfg.products.get(CUSTOM_PRODUCT_ID) {
                if self.left_state.txid != saved_custom.txid
                    || self.left_state.rxid != saved_custom.rxid
                    || self.left_state.selected_baudrate != saved_custom.baudrate
                {
                    return true;
                }
                let target_addr = if self.left_state.flash_type_key == "boot" {
                    &saved_custom.boot_address
                } else {
                    &saved_custom.app_address
                };
                if &self.left_state.flash_address != target_addr {
                    return true;
                }
            } else {
                return true;
            }
        }

        cur != self.saved_cfg
    }

    fn toggle_connection(&mut self) {
        if self.is_connected {
            CanController::disconnect(
                self.can_driver.clone(),
                self.bus_running.clone(),
                self.bus_tx.clone(),
            );
            self.is_connected = false;
            self.conn_start_time = None;
            self.is_querying_dids = false;
        } else {
            match CanController::connect(
                self.can_driver.clone(),
                self.bus_running.clone(),
                self.bus_tx.clone(),
                &self.left_state.selected_interface,
                &self.left_state.selected_channel,
                &self.left_state.selected_baudrate,
            ) {
                Ok(actual_ch) => {
                    self.left_state.selected_channel = actual_ch;
                    self.is_connected = true;
                    self.conn_start_time = Some(Instant::now());
                }
                Err(err_msg) => {
                    let time_str = Local::now().format("%H:%M:%S").to_string();
                    let log_text =
                        t_fmt("log_conn_fail", &[("time", &time_str), ("err", &err_msg)]);
                    self.right_state
                        .flash_logs
                        .push((log_text, "error".to_string()));
                }
            }
        }
    }

    fn is_frame_accepted(&self, id: u32, is_ext: bool) -> bool {
        match self.right_state.filter_mode_idx {
            0 => true,
            1 => {
                let tx_id = u32::from_str_radix(
                    self.left_state
                        .txid
                        .trim_start_matches("0x")
                        .trim_start_matches("0X"),
                    16,
                )
                .unwrap_or(0);
                let rx_id = u32::from_str_radix(
                    self.left_state
                        .rxid
                        .trim_start_matches("0x")
                        .trim_start_matches("0X"),
                    16,
                )
                .unwrap_or(0);
                id == tx_id || id == rx_id
            }
            2 => {
                let std_ok = !is_ext && self.filter_dlg.filter_std;
                let ext_ok = is_ext && self.filter_dlg.filter_ext;
                if !std_ok && !ext_ok {
                    return false;
                }
                if self.active_filter_ids.is_empty() {
                    true
                } else {
                    self.active_filter_ids.contains(&id)
                }
            }
            _ => true,
        }
    }
}

impl eframe::App for UdsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.init_titlebar_ticks > 0 {
            self.init_titlebar_ticks -= 1;
            let is_dark = if self.theme_mode == 2 {
                ui::theme::is_system_dark_mode()
            } else {
                self.theme_mode == 0
            };
            ui::theme::update_native_titlebar(ctx, is_dark);
        }

        if self.theme_mode == 2 {
            let is_sys_dark = ui::theme::is_system_dark_mode();
            ui::theme::apply_theme(ctx, is_sys_dark);
        }

        if ctx.input(|i| i.viewport().close_requested()) {
            if !self.force_close && self.has_unsaved_changes() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.show_unsaved_dialog = true;
            }
        }

        while let Ok(event) = self.bus_rx.try_recv() {
            match event {
                BusEvent::CanMsg {
                    id,
                    is_ext,
                    is_rx,
                    formatted,
                } => {
                    if !self.right_state.can_monitoring_paused && self.is_frame_accepted(id, is_ext)
                    {
                        let now_us = self
                            .conn_start_time
                            .map(|st| st.elapsed().as_micros() as u64)
                            .unwrap_or_else(|| {
                                std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_micros() as u64
                            });

                        let parts: Vec<&str> = formatted.split_whitespace().collect();
                        let mut data_bytes = Vec::new();
                        let mut dlc = 8u8;

                        for part in &parts {
                            if part.starts_with('[') && part.ends_with(']') {
                                if let Ok(d) =
                                    part.trim_matches('[').trim_matches(']').parse::<u8>()
                                {
                                    dlc = d;
                                }
                            } else if part.len() == 2 {
                                if let Ok(b) = u8::from_str_radix(part, 16) {
                                    data_bytes.push(b);
                                }
                            }
                        }

                        self.right_state.can_logs.push(CanLogItem {
                            timestamp_us: now_us,
                            channel: 1,
                            id,
                            is_ext,
                            is_rx,
                            dlc,
                            data: data_bytes,
                            formatted,
                        });

                        if self.right_state.can_logs.len() > 1000 {
                            self.right_state.can_logs.remove(0);
                        }
                    }
                }
                BusEvent::FlashLog(msg, lvl) => self.right_state.flash_logs.push((msg, lvl)),
                BusEvent::DidResult(k, v) => {
                    self.left_state
                        .did_original_values
                        .insert(k.clone(), v.clone());
                    self.left_state.did_values.insert(k, v);
                }
                BusEvent::DidQueryFinished => {
                    self.is_querying_dids = false;
                }
                BusEvent::WriteNotice { success, msg } => {
                    self.notice_dialog = Some((success, msg.clone()));
                    let time_tag = Local::now().format("%H:%M:%S").to_string();
                    let lvl = if success { "info" } else { "error" };
                    self.right_state.flash_logs.push((
                        format!("[{}] {} | {}", time_tag, lvl.to_uppercase(), msg),
                        lvl.to_string(),
                    ));
                }
            }
        }

        while let Ok(event) = self.flash_rx.try_recv() {
            match event {
                FlashEvent::Log(msg, lvl) => self.right_state.flash_logs.push((msg, lvl)),
                FlashEvent::Progress(p, text) => {
                    self.right_state.progress_percent = p;
                    self.right_state.progress_text = text;
                }
                FlashEvent::Finished(success, msg) => {
                    self.flash_engine.stop();
                    if self.right_state.total_elapsed_secs.is_none() {
                        if let Some(st) = self.right_state.flash_start_time {
                            self.right_state.total_elapsed_secs = Some(st.elapsed().as_secs_f32());
                        }
                    }
                    self.right_state
                        .flash_logs
                        .push((msg, if success { "info" } else { "error" }.to_string()));
                }
            }
        }

        // ==================== 顶部菜单栏 ====================
        TopBottomPanel::top("top_menubar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button(format!("📁 {}", t("menu_file")), |ui| {
                    if ui.button(t("menu_file_open")).clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("bin", &["bin"])
                            .pick_file()
                        {
                            self.right_state.bin_path = path.to_string_lossy().to_string();
                        }
                        ui.close_menu();
                    }
                    if ui
                        .button(format!("💾 {}", t("menu_file_save_cfg")))
                        .clicked()
                    {
                        self.try_save_config();
                        ui.close_menu();
                    }
                    if ui.button(t("menu_file_exp_flash")).clicked() {
                        let _ = LogExporter::export_flash_log(&self.right_state.flash_logs);
                        ui.close_menu();
                    }
                    if ui.button(t("menu_file_exp_can")).clicked() {
                        let _ = LogExporter::export_can_log(&self.right_state.can_logs);
                        ui.close_menu();
                    }
                    if ui.button(t("menu_file_exit")).clicked() {
                        if self.has_unsaved_changes() {
                            self.show_unsaved_dialog = true;
                        } else {
                            self.force_close = true;
                            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        ui.close_menu();
                    }
                });

                ui.menu_button(format!("⚙ {}", t("menu_config")), |ui| {
                    if ui.button(t("menu_config_security")).clicked() {
                        self.sec_dlg.open = true;
                        ui.close_menu();
                    }
                    if ui.button(t("menu_config_prod_mgr")).clicked() {
                        self.prod_dlg.open = true;
                        ui.close_menu();
                    }
                    if ui.button(t("menu_config_algo_mgr")).clicked() {
                        self.algo_dlg.open = true;
                        ui.close_menu();
                    }
                    if ui.button(t("menu_config_did_mgr")).clicked() {
                        self.did_dlg.open = true;
                        ui.close_menu();
                    }
                });

                ui.menu_button(format!("🌐 {}", t("menu_lang")), |ui| {
                    if ui.button(t("menu_lang_zh")).clicked() {
                        set_language("zh");
                        self.cfg.language = "zh".to_string();
                        ctx.send_viewport_cmd(egui::ViewportCommand::Title(get_app_title()));
                        ui.ctx().request_repaint();
                        ui.close_menu();
                    }
                    if ui.button(t("menu_lang_en")).clicked() {
                        set_language("en");
                        self.cfg.language = "en".to_string();
                        ctx.send_viewport_cmd(egui::ViewportCommand::Title(get_app_title()));
                        ui.ctx().request_repaint();
                        ui.close_menu();
                    }
                });

                ui.menu_button(format!("🎨 {}", t("menu_theme")), |ui| {
                    if ui.button(t("menu_theme_dark")).clicked() {
                        self.theme_mode = 0;
                        ui::theme::apply_theme(ctx, true);
                        ui.close_menu();
                    }
                    if ui.button(t("menu_theme_light")).clicked() {
                        self.theme_mode = 1;
                        ui::theme::apply_theme(ctx, false);
                        ui.close_menu();
                    }
                    if ui.button(t("menu_theme_auto")).clicked() {
                        self.theme_mode = 2;
                        let is_sys_dark = ui::theme::is_system_dark_mode();
                        ui::theme::apply_theme(ctx, is_sys_dark);
                        ui.close_menu();
                    }
                });

                ui.menu_button(format!("❓ {}", t("menu_help")), |ui| {
                    if ui.button(t("menu_help_about")).clicked() {
                        self.about_dlg.open = true;
                        ui.close_menu();
                    }
                });

                if self.show_saved_toast_timer > 0.0 {
                    self.show_saved_toast_timer -= ui.input(|i| i.stable_dt);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!("✔ {}", t("cfg_saved_notice")))
                                .color(Color32::from_rgb(82, 196, 26))
                                .small(),
                        );
                    });
                }
            });
        });

        // ==================== 底部状态栏 ====================
        let current_prod = self.cfg.products.get(&self.left_state.selected_product);
        let effective_sec_service = current_prod
            .and_then(|p| {
                if p.override_security {
                    p.security_service.clone()
                } else {
                    None
                }
            })
            .unwrap_or_else(|| self.cfg.security_service.clone());

        let effective_verify_method = current_prod
            .and_then(|p| {
                if p.override_security {
                    p.verify_method.clone()
                } else {
                    None
                }
            })
            .unwrap_or_else(|| self.cfg.verify_method.clone());

        let effective_algo_or_cert = current_prod
            .and_then(|p| {
                if p.override_security {
                    p.security_algo_or_cert.clone()
                } else {
                    None
                }
            })
            .unwrap_or_else(|| {
                if effective_sec_service.contains("29") {
                    self.cfg.auth_0x29_cert_path.clone()
                } else {
                    "SonnePower".to_string()
                }
            });

        let theme_snapshot = CURRENT_THEME.read().unwrap().clone();

        TopBottomPanel::bottom("bottom_status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let baud_k = if let Ok(b) = self.left_state.selected_baudrate.parse::<u32>() {
                    format!("{}k", b / 1000)
                } else {
                    "--".to_string()
                };

                ui.label(format!(
                    "{}: {} ({}) {}: {}",
                    t("status_hw"),
                    self.left_state.selected_interface,
                    self.left_state.selected_channel,
                    t("status_baud"),
                    baud_k
                ));
                ui.separator();
                ui.label(format!(
                    "{}: TX: {} RX: {}",
                    t("status_addr"),
                    self.left_state.txid,
                    self.left_state.rxid
                ));
                ui.separator();

                let is_0x29 = effective_sec_service.contains("29");
                let auth_info = if is_0x29 {
                    let cert = if effective_algo_or_cert.is_empty() {
                        "None/Default"
                    } else {
                        "Custom Cert"
                    };
                    format!("{}: {}", t("status_cred"), cert)
                } else {
                    format!("{}: {}", t("status_algo"), effective_algo_or_cert)
                };

                let proto_text = format!(
                    "{}: {} {}: {} {} {}: {}",
                    t("status_proto"),
                    effective_sec_service,
                    t("status_verify"),
                    effective_verify_method.to_uppercase(),
                    auth_info,
                    t("status_pad"),
                    self.cfg.isotp_tx_padding
                );
                ui.label(RichText::new(proto_text).color(hex_to_color(&theme_snapshot.accent)));

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let badge = if self.is_connected {
                        (
                            t("status_connected"),
                            hex_to_color(&theme_snapshot.success_btn),
                        )
                    } else {
                        (
                            t("status_disconnected"),
                            hex_to_color(&theme_snapshot.danger_btn),
                        )
                    };
                    ui.label(
                        RichText::new(format!("● {}", badge.0))
                            .color(badge.1)
                            .strong(),
                    );
                });
            });
        });

        let mut do_toggle_conn = false;
        let mut do_query_dids = false;
        let mut do_write_did: Option<(String, String)> = None;
        let mut log_cfg_changed: Option<(String, String)> = None;
        let mut do_start_flash = false;
        let mut do_stop_flash = false;
        let mut do_manual_reboot = false;
        let mut do_open_filter_dlg = false;

        // 1. 左侧面板
        SidePanel::left("main_left_side_panel")
            .resizable(false)
            .exact_width(self.left_panel_width)
            .show_separator_line(false)
            .frame(
                egui::Frame::none()
                    .fill(ctx.style().visuals.panel_fill)
                    .inner_margin(egui::Margin {
                        left: 6.0,
                        right: 0.0,
                        top: 4.0,
                        bottom: 4.0,
                    }),
            )
            .show(ctx, |ui| {
                let connected = self.is_connected;
                let is_flashing = self.flash_engine.running.load(Ordering::SeqCst);

                render_left(
                    ui,
                    &mut self.left_state,
                    &self.cfg,
                    connected,
                    is_flashing,
                    self.is_querying_dids,
                    || do_toggle_conn = true,
                    || do_query_dids = true,
                    |key, val| do_write_did = Some((key, val)),
                    |item, val| log_cfg_changed = Some((item, val)),
                );
            });

        // 2. 垂直分割条
        SidePanel::left("main_v_splitter_panel")
            .resizable(false)
            .exact_width(5.0)
            .show_separator_line(false)
            .frame(egui::Frame::none().fill(ctx.style().visuals.panel_fill))
            .show(ctx, |ui| {
                let v_split_resp = draw_vscode_v_splitter(ui, 5.0);
                if v_split_resp.dragged() {
                    let dx = v_split_resp.drag_delta().x;
                    self.left_panel_width = (self.left_panel_width + dx).clamp(340.0, 680.0);
                }
            });

        // 3. 右侧主面板
        CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(ctx.style().visuals.panel_fill)
                    .inner_margin(egui::Margin {
                        left: 0.0,
                        right: 6.0,
                        top: 4.0,
                        bottom: 4.0,
                    }),
            )
            .show(ctx, |ui| {
                let is_flashing = self.flash_engine.running.load(Ordering::SeqCst);
                let flash_logs_ref = self.right_state.flash_logs.clone();
                let can_logs_ref = self.right_state.can_logs.clone();

                render_right(
                    ui,
                    &mut self.right_state,
                    is_flashing,
                    self.is_connected,
                    self.is_querying_dids,
                    &effective_verify_method,
                    || do_start_flash = true,
                    || do_stop_flash = true,
                    || do_manual_reboot = true,
                    || do_open_filter_dlg = true,
                    move || {
                        let _ = LogExporter::export_flash_log(&flash_logs_ref);
                    },
                    move || {
                        let _ = LogExporter::export_can_log(&can_logs_ref);
                    },
                );
            });

        // ==================== 统一动作分发 ====================
        if do_open_filter_dlg {
            self.filter_dlg.open = true;
        }

        if do_start_flash && !self.is_querying_dids {
            FlashController::start_flash(
                self.flash_engine.clone(),
                self.flash_tx.clone(),
                self.can_driver.clone(),
                self.bus_tx.clone(),
                &self.cfg,
                &self.left_state,
                &self.right_state,
                &effective_sec_service,
                &effective_verify_method,
            );
        }

        if do_stop_flash {
            FlashController::stop_flash(&self.flash_engine);
        }

        if do_manual_reboot {
            let can_driver = self.can_driver.clone();
            let bus_tx = self.bus_tx.clone();
            let tx_id = u32::from_str_radix(
                self.left_state
                    .txid
                    .trim_start_matches("0x")
                    .trim_start_matches("0X"),
                16,
            )
            .unwrap_or(0x18FFFE32);
            let rx_id = u32::from_str_radix(
                self.left_state
                    .rxid
                    .trim_start_matches("0x")
                    .trim_start_matches("0X"),
                16,
            )
            .unwrap_or(0x18FF32FE);
            let pad_byte = u8::from_str_radix(
                self.cfg
                    .isotp_tx_padding
                    .trim_start_matches("0x")
                    .trim_start_matches("0X"),
                16,
            )
            .unwrap_or(0x55);

            std::thread::spawn(move || {
                let time_str = Local::now().format("%H:%M:%S").to_string();
                let mut guard = can_driver.lock().unwrap();
                if let Some(ref mut drv) = *guard {
                    let mut isotp = crate::core::isotp::IsoTpHandler::new(
                        &mut **drv, tx_id, rx_id, true, pad_byte,
                    );
                    let _ = isotp.send_payload(&[0x11, 0x01]);
                    let _ = bus_tx.send(BusEvent::FlashLog(
                        format!("[{}] INFO | 手动发送复位指令 (0x11 0x01)...", time_str),
                        "info".to_string(),
                    ));
                }
            });
        }

        if let Some((item, val)) = log_cfg_changed {
            if let Some(custom_prod) = self.cfg.products.get_mut(CUSTOM_PRODUCT_ID) {
                match item.as_str() {
                    "custom_txid" => custom_prod.txid = val.clone(),
                    "custom_rxid" => custom_prod.rxid = val.clone(),
                    "custom_flash_address" => {
                        if self.left_state.flash_type_key == "boot" {
                            custom_prod.boot_address = val.clone();
                        } else {
                            custom_prod.app_address = val.clone();
                        }
                    }
                    _ => {}
                }
            }

            if item == t("product_model") {
                self.cfg.default_product = val.clone();
            } else if item == t("can_baudrate") {
                self.cfg.default_baudrate = val.clone();
                if self.left_state.is_custom() {
                    if let Some(custom_prod) = self.cfg.products.get_mut(CUSTOM_PRODUCT_ID) {
                        custom_prod.baudrate = val.clone();
                    }
                }
            } else if item == t("flash_type") {
                self.cfg.default_flash_type = self.left_state.flash_type_key.clone();
            } else if item == t("can_hardware") {
                self.cfg.default_interface = val.clone();
            } else if item == t("can_channel") {
                self.cfg.default_channel = val.clone();
            }

            let time_str = Local::now().format("%H:%M:%S").to_string();
            let log_text = t_fmt(
                "log_cfg_changed",
                &[("time", &time_str), ("item", &item), ("val", &val)],
            );
            self.right_state
                .flash_logs
                .push((log_text, "info".to_string()));
        }

        if do_toggle_conn {
            self.toggle_connection();
        }

        if do_query_dids
            && !self.is_querying_dids
            && !self.flash_engine.running.load(Ordering::SeqCst)
        {
            self.is_querying_dids = true;
            let can_driver = self.can_driver.clone();
            let bus_tx = self.bus_tx.clone();
            let cfg = self.cfg.clone();
            let left_state_snap = self.left_state.clone();

            std::thread::spawn(move || {
                DidController::query_all_dids(can_driver, bus_tx.clone(), &cfg, &left_state_snap);
                // 仅在所有项目完整查询并回写完成后，才派发 Finished 事件解除禁用
                let _ = bus_tx.send(BusEvent::DidQueryFinished);
            });
        }

        if let Some((k, v)) = do_write_did {
            if let Err(err_msg) = DidController::write_did(
                self.can_driver.clone(),
                self.bus_tx.clone(),
                &self.cfg,
                &self.left_state,
                k,
                v,
            ) {
                self.notice_dialog = Some((false, err_msg.clone()));
                let time_tag = Local::now().format("%H:%M:%S").to_string();
                self.right_state.flash_logs.push((
                    format!("[{}] ERROR | {}", time_tag, err_msg),
                    "error".to_string(),
                ));
            }
        }

        // ==================== 对话框处理 ====================
        self.about_dlg.show(ctx);
        self.sec_dlg.show(ctx, &mut self.cfg);

        let before_cfg = self.cfg.clone();
        self.prod_dlg.show(ctx, &mut self.cfg);
        if self.cfg != before_cfg {
            self.left_state.sync_from_product(&self.cfg);
        }

        self.algo_dlg.show(ctx, &mut self.cfg);
        self.did_dlg.show(ctx, &mut self.cfg);
        self.filter_dlg.show(ctx, &mut self.active_filter_ids);

        if let Some((success, msg)) = self.notice_dialog.clone() {
            let title = if success {
                format!("✔ {}", t("dlg_op_success"))
            } else {
                format!("❌ {}", t("dlg_op_fail"))
            };

            let screen_rect = ctx.screen_rect();
            let mut is_open = true;

            Window::new(title)
                .id(egui::Id::new("uds_notice_dialog_modal_window"))
                .default_pos([
                    screen_rect.center().x - 160.0,
                    screen_rect.center().y - 70.0,
                ])
                .collapsible(false)
                .resizable(false)
                .movable(true)
                .open(&mut is_open)
                .show(ctx, |ui| {
                    ui.set_width(320.0);
                    ui.add_space(8.0);

                    let text_color = if success {
                        Color32::from_rgb(82, 196, 26)
                    } else {
                        Color32::from_rgb(255, 77, 79)
                    };

                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new(&msg).color(text_color).size(13.5).strong());
                    });

                    ui.add_space(14.0);

                    ui.vertical_centered(|ui| {
                        let btn_txt = t("dlg_confirm");
                        let label = if btn_txt.is_empty() || btn_txt == "dlg_confirm" {
                            "确定".to_string()
                        } else {
                            btn_txt
                        };

                        let ok_btn =
                            egui::Button::new(RichText::new(label).strong().color(Color32::WHITE))
                                .fill(Color32::from_rgb(0, 122, 204))
                                .min_size(egui::vec2(80.0, 24.0));

                        if ui.add(ok_btn).clicked() {
                            self.notice_dialog = None;
                        }
                    });

                    ui.add_space(4.0);
                });

            if !is_open {
                self.notice_dialog = None;
            }
        }

        if self.show_unsaved_dialog {
            Window::new(t("unsaved_prompt_title"))
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.add_space(4.0);
                    ui.label(RichText::new(t("unsaved_prompt_msg")).size(14.0));
                    ui.add_space(14.0);

                    ui.horizontal(|ui| {
                        let save_btn = egui::Button::new(
                            RichText::new(t("unsaved_save_and_exit"))
                                .strong()
                                .color(Color32::WHITE),
                        )
                        .fill(Color32::from_rgb(0, 122, 204));

                        if ui.add(save_btn).clicked() {
                            self.try_save_config();
                            self.show_unsaved_dialog = false;
                            self.force_close = true;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }

                        let discard_btn = egui::Button::new(
                            RichText::new(t("unsaved_discard_and_exit"))
                                .strong()
                                .color(Color32::WHITE),
                        )
                        .fill(Color32::from_rgb(217, 83, 79));

                        if ui.add(discard_btn).clicked() {
                            self.show_unsaved_dialog = false;
                            self.force_close = true;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }

                        if ui.button(t("dlg_cancel")).clicked() {
                            self.show_unsaved_dialog = false;
                        }
                    });
                });
        }

        ctx.request_repaint_after(std::time::Duration::from_millis(50));
    }
}

fn main() -> eframe::Result<()> {
    load_locales();
    let cfg = load_config_dat();
    set_language(&cfg.language);

    let window_title = get_app_title();
    let app_icon = load_app_icon();

    let mut viewport_builder = egui::ViewportBuilder::default()
        .with_title(window_title.clone())
        .with_inner_size([1420.0, 920.0])
        .with_min_inner_size([1100.0, 720.0]);

    if let Some(icon) = app_icon {
        viewport_builder = viewport_builder.with_icon(icon);
    }

    let native_options = eframe::NativeOptions {
        viewport: viewport_builder,
        ..Default::default()
    };

    eframe::run_native(
        &window_title,
        native_options,
        Box::new(|cc| Box::new(UdsApp::new(cc))),
    )
}