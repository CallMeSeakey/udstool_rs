#![allow(dead_code)]
use crate::ui::theme::{hex_to_color, CURRENT_THEME};
use egui::{CursorIcon, Pos2, Response, Sense, Ui, Vec2};

pub const ICO_FILE: &str = "📁";
pub const ICO_OPEN: &str = "📂";
pub const ICO_SAVE: &str = "💾";
pub const ICO_EXIT: &str = "❌";
pub const ICO_CFG: &str = "⚙";
pub const ICO_SEC: &str = "🛡️";
pub const ICO_PROD: &str = "📦";
pub const ICO_ALGO: &str = "🔐";
pub const ICO_THEME: &str = "🎨";
pub const ICO_LANG: &str = "🌐";
pub const ICO_HELP: &str = "❓";
pub const ICO_INFO: &str = "ℹ️";
pub const ICO_CONNECT: &str = "⚡";
pub const ICO_DISCONN: &str = "⏹";
pub const ICO_SEARCH: &str = "🔍";
pub const ICO_START: &str = "▶";
pub const ICO_REBOOT: &str = "🔄";
pub const ICO_CLEAR: &str = "🗑";
pub const ICO_MONITOR: &str = "📡";
pub const ICO_PAUSE: &str = "⏸";
pub const ICO_ADD: &str = "＋";
pub const ICO_DEL: &str = "－";

pub fn draw_vscode_v_splitter(ui: &mut Ui, width: f32) -> Response {
    let height = ui.available_height();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::drag());

    if response.hovered() || response.dragged() {
        ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
    }

    let is_active = response.hovered() || response.dragged();
    let painter = ui.painter();
    let theme = CURRENT_THEME.read().unwrap().clone();

    let dot_color = if is_active {
        hex_to_color(&theme.splitter_hover)
    } else {
        hex_to_color(&theme.splitter_dots)
    };

    let center = rect.center();
    for &offset_y in &[-4.0, 0.0, 4.0] {
        painter.circle_filled(Pos2::new(center.x, center.y + offset_y), 1.0, dot_color);
    }

    response
}

pub fn draw_vscode_h_splitter(ui: &mut Ui, height: f32) -> Response {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::drag());

    if response.hovered() || response.dragged() {
        ui.ctx().set_cursor_icon(CursorIcon::ResizeVertical);
    }

    let is_active = response.hovered() || response.dragged();
    let painter = ui.painter();
    let theme = CURRENT_THEME.read().unwrap().clone();

    let dot_color = if is_active {
        hex_to_color(&theme.splitter_hover)
    } else {
        hex_to_color(&theme.splitter_dots)
    };

    let center = rect.center();
    for &offset_x in &[-4.0, 0.0, 4.0] {
        painter.circle_filled(Pos2::new(center.x + offset_x, center.y), 1.0, dot_color);
    }

    response
}
