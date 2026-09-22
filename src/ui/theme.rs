// src/ui/theme.rs
use eframe::egui::{
    self, epaint::Shadow, Color32, FontFamily, Stroke, Visuals,
};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::sync::RwLock;

/// 严格对应 dark.json / light.json 定义的 32 个主题颜色键
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ThemeColors {
    pub panel_bg: String,
    pub window_bg: String,
    pub faint_bg: String,
    pub extreme_bg: String,
    pub border: String,
    pub accent: String,
    pub text_normal: String,
    pub text_placeholder: String,
    pub readonly_bg: String,
    pub readonly_border: String,
    pub readonly_text: String,
    pub btn_inactive: String,
    pub btn_hover: String,
    pub btn_active: String,
    pub primary_btn: String,
    pub danger_btn: String,
    pub success_btn: String,
    pub warn_text: String,
    pub error_text: String,
    pub info_text: String,
    pub can_tx_text: String,
    pub can_rx_text: String,
    pub splitter_border: String,
    pub splitter_dots: String,
    pub splitter_hover: String,
    pub log_embed_bg: String,
    pub query_btn_bg: String,
    pub selection_bg: String,
    pub selection_border: String,
    pub text_subtle: String,
    pub progress_track_bg: String,
    pub progress_fill: String,
}

// 编译期内嵌默认主题 JSON 内容
const EMBEDDED_DARK_JSON: &str = include_str!("../../themes/dark.json");
const EMBEDDED_LIGHT_JSON: &str = include_str!("../../themes/light.json");

/// 从本地文件或内嵌二进制加载主题
fn load_theme_from_json(file_name: &str, embedded_fallback: &str) -> ThemeColors {
    // 1. 获取当前可执行文件 (.exe) 所在的真实绝对目录
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()));

    // 2. 候选路径列表：优先支持复数 `themes/` 以及单数 `theme/`
    let mut search_paths = vec![
        format!("themes/{}", file_name),
        format!("theme/{}", file_name),
        format!("src/theme/{}", file_name),
        format!("src/themes/{}", file_name),
        file_name.to_string(),
    ];

    // 3. 将基于 exe 目录的绝对路径也加入候选
    if let Some(ref dir) = exe_dir {
        search_paths.insert(0, dir.join("themes").join(file_name).to_string_lossy().to_string());
        search_paths.insert(1, dir.join("theme").join(file_name).to_string_lossy().to_string());
        search_paths.insert(2, dir.join(file_name).to_string_lossy().to_string());
    }

    // 4. 遍历检测并加载外部 JSON
    for path_str in &search_paths {
        let p = Path::new(path_str);
        if p.exists() {
            if let Ok(content) = fs::read_to_string(p) {
                if let Ok(colors) = serde_json::from_str::<ThemeColors>(&content) {
                    return colors;
                }
            }
        }
    }

    // 5. 外部无匹配或解析失败时，回退到内嵌默认配置
    serde_json::from_str::<ThemeColors>(embedded_fallback)
        .unwrap_or_else(|e| panic!("解析主题 JSON {} 失败: {}", file_name, e))
}

pub fn dark_theme() -> ThemeColors {
    load_theme_from_json("dark.json", EMBEDDED_DARK_JSON)
}

pub fn light_theme() -> ThemeColors {
    load_theme_from_json("light.json", EMBEDDED_LIGHT_JSON)
}

lazy_static! {
    pub static ref CURRENT_THEME: RwLock<ThemeColors> = RwLock::new(dark_theme());
}

pub fn hex_to_color(hex: &str) -> Color32 {
    let clean = hex.trim_start_matches('#');
    if clean.len() == 6 {
        let r = u8::from_str_radix(&clean[0..2], 16).unwrap_or(0);
        let g = u8::from_str_radix(&clean[2..4], 16).unwrap_or(0);
        let b = u8::from_str_radix(&clean[4..6], 16).unwrap_or(0);
        Color32::from_rgb(r, g, b)
    } else {
        Color32::WHITE
    }
}

pub fn is_system_dark_mode() -> bool {
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        if let Ok(output) = Command::new("reg")
            .args(&[
                "query",
                "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize",
                "/v",
                "AppsUseLightTheme",
            ])
            .output()
        {
            let text = String::from_utf8_lossy(&output.stdout);
            if text.contains("0x0") {
                return true;
            } else if text.contains("0x1") {
                return false;
            }
        }
    }
    true
}

/// 将 JSON 中配置的全部颜色深度同步到 egui 的 Visuals 中
pub fn apply_theme(ctx: &egui::Context, is_dark: bool) {
    let theme = if is_dark { dark_theme() } else { light_theme() };

    let mut visuals = if is_dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };

    // 1. 同步背景颜色与边框
    visuals.panel_fill = hex_to_color(&theme.panel_bg);
    visuals.window_fill = hex_to_color(&theme.window_bg);
    visuals.faint_bg_color = hex_to_color(&theme.faint_bg);
    visuals.extreme_bg_color = hex_to_color(&theme.extreme_bg);
    visuals.window_stroke = Stroke::new(1.0_f32, hex_to_color(&theme.border));

    // 2. 同步文本颜色
    visuals.override_text_color = Some(hex_to_color(&theme.text_normal));
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, hex_to_color(&theme.text_normal));
    visuals.widgets.inactive.fg_stroke = Stroke::new(1.0_f32, hex_to_color(&theme.text_normal));
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.0_f32, hex_to_color(&theme.text_normal));
    visuals.widgets.active.fg_stroke = Stroke::new(1.0_f32, hex_to_color(&theme.text_normal));

    // 3. 同步控件背景与交互状态
    visuals.widgets.inactive.bg_fill = hex_to_color(&theme.btn_inactive);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, hex_to_color(&theme.border));

    visuals.widgets.hovered.bg_fill = hex_to_color(&theme.btn_hover);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, hex_to_color(&theme.border));

    visuals.widgets.active.bg_fill = hex_to_color(&theme.btn_active);
    visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, hex_to_color(&theme.accent));

    visuals.widgets.noninteractive.bg_fill = hex_to_color(&theme.panel_bg);
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, hex_to_color(&theme.border));

    // 4. 同步选中高亮与阴影
    visuals.selection.bg_fill = hex_to_color(&theme.selection_bg);
    visuals.selection.stroke = Stroke::new(1.0_f32, hex_to_color(&theme.selection_border));
    visuals.window_shadow = Shadow::NONE;
    visuals.popup_shadow = Shadow::NONE;

    ctx.set_visuals(visuals);

    if let Ok(mut lock) = CURRENT_THEME.write() {
        *lock = theme;
    }

    update_native_titlebar(ctx, is_dark);
}

pub fn update_native_titlebar(ctx: &egui::Context, dark_mode: bool) {
    let sys_theme = if dark_mode {
        egui::SystemTheme::Dark
    } else {
        egui::SystemTheme::Light
    };
    ctx.send_viewport_cmd(egui::ViewportCommand::SetTheme(sys_theme));

    #[cfg(target_os = "windows")]
    {
        use libloading::{Library, Symbol};

        type DwmSetWindowAttributeFn =
            unsafe extern "system" fn(isize, u32, *const i32, u32) -> i32;
        type GetCurrentProcessIdFn = unsafe extern "system" fn() -> u32;
        type GetWindowThreadProcessIdFn = unsafe extern "system" fn(isize, *mut u32) -> u32;
        type EnumWindowsFn = unsafe extern "system" fn(
            unsafe extern "system" fn(isize, isize) -> i32,
            isize,
        ) -> i32;
        type SetWindowPosFn =
            unsafe extern "system" fn(isize, isize, i32, i32, i32, i32, u32) -> i32;
        type SendMessageWFn = unsafe extern "system" fn(isize, u32, usize, isize) -> isize;
        type RedrawWindowFn =
            unsafe extern "system" fn(isize, *const std::ffi::c_void, isize, u32) -> i32;

        unsafe {
            let kernel32 = match Library::new("kernel32.dll") {
                Ok(lib) => lib,
                Err(_) => return,
            };
            let user32 = match Library::new("user32.dll") {
                Ok(lib) => lib,
                Err(_) => return,
            };
            let dwmapi = match Library::new("dwmapi.dll") {
                Ok(lib) => lib,
                Err(_) => return,
            };

            let get_pid: Symbol<GetCurrentProcessIdFn> = match kernel32.get(b"GetCurrentProcessId") {
                Ok(f) => f,
                Err(_) => return,
            };
            let enum_windows: Symbol<EnumWindowsFn> = match user32.get(b"EnumWindows") {
                Ok(f) => f,
                Err(_) => return,
            };
            let set_attr: Symbol<DwmSetWindowAttributeFn> =
                match dwmapi.get(b"DwmSetWindowAttribute") {
                    Ok(f) => f,
                    Err(_) => return,
                };
            let set_pos: Symbol<SetWindowPosFn> = match user32.get(b"SetWindowPos") {
                Ok(f) => f,
                Err(_) => return,
            };
            let send_msg: Symbol<SendMessageWFn> = match user32.get(b"SendMessageW") {
                Ok(f) => f,
                Err(_) => return,
            };
            let redraw_win: Symbol<RedrawWindowFn> = match user32.get(b"RedrawWindow") {
                Ok(f) => f,
                Err(_) => return,
            };

            struct EnumCtx {
                target_pid: u32,
                hwnd: isize,
            }

            static mut ENUM_CTX: EnumCtx = EnumCtx {
                target_pid: 0,
                hwnd: 0,
            };
            ENUM_CTX.target_pid = get_pid();
            ENUM_CTX.hwnd = 0;

            unsafe extern "system" fn enum_proc(hwnd: isize, _lparam: isize) -> i32 {
                if let Ok(user32) = Library::new("user32.dll") {
                    if let Ok(get_win_pid) =
                        user32.get::<Symbol<GetWindowThreadProcessIdFn>>(b"GetWindowThreadProcessId")
                    {
                        let mut w_pid = 0u32;
                        get_win_pid(hwnd, &mut w_pid);
                        if w_pid == ENUM_CTX.target_pid {
                            ENUM_CTX.hwnd = hwnd;
                            return 0;
                        }
                    }
                }
                1
            }

            enum_windows(enum_proc, 0);
            let hwnd = ENUM_CTX.hwnd;

            if hwnd != 0 {
                let dark_val: i32 = if dark_mode { 1 } else { 0 };
                let _ = set_attr(hwnd, 19, &dark_val, 4);
                let _ = set_attr(hwnd, 20, &dark_val, 4);

                const SWP_NOSIZE: u32 = 0x0001;
                const SWP_NOMOVE: u32 = 0x0002;
                const SWP_NOZORDER: u32 = 0x0004;
                const SWP_FRAMECHANGED: u32 = 0x0020;
                let _ = set_pos(
                    hwnd,
                    0,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED,
                );

                const WM_NCACTIVATE: u32 = 0x0086;
                let _ = send_msg(hwnd, WM_NCACTIVATE, 0, 0);
                let _ = send_msg(hwnd, WM_NCACTIVATE, 1, 0);

                const RDW_FRAME: u32 = 0x0400;
                const RDW_INVALIDATE: u32 = 0x0001;
                const RDW_UPDATENOW: u32 = 0x0100;
                let _ = redraw_win(
                    hwnd,
                    std::ptr::null(),
                    0,
                    RDW_FRAME | RDW_INVALIDATE | RDW_UPDATENOW,
                );
            }
        }
    }
}

pub fn setup_custom_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let font_path = "C:\\Windows\\Fonts\\msyh.ttc";
    if let Ok(font_bytes) = std::fs::read(font_path) {
        fonts.font_data.insert(
            "msyh".to_owned(),
            egui::FontData::from_owned(font_bytes),
        );
        fonts
            .families
            .entry(FontFamily::Proportional)
            .or_default()
            .insert(0, "msyh".to_owned());
        fonts
            .families
            .entry(FontFamily::Monospace)
            .or_default()
            .insert(0, "msyh".to_owned());
        ctx.set_fonts(fonts);
    }
}