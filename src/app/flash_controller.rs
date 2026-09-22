// src/app/flash_controller.rs
use crossbeam_channel::Sender;
use std::fs;
use std::sync::{Arc, Mutex};

use crate::config::AppConfig;
use crate::core::can::CanAdapter;
use crate::core::flash_engine::{FlashEngine, FlashEvent, FlashParams};
use crate::ui::left_panel::LeftPanelState;
use crate::ui::right_panel::RightPanelState;
use crate::BusEvent;

pub struct FlashController;

impl FlashController {
    pub fn start_flash(
        engine: FlashEngine,
        flash_tx: Sender<FlashEvent>,
        can_driver: Arc<Mutex<Option<Box<dyn CanAdapter>>>>,
        bus_tx: Sender<BusEvent>,
        cfg: &AppConfig,
        left_state: &LeftPanelState,
        right_state: &RightPanelState,
        effective_sec_service: &str,
        effective_verify_method: &str,
    ) {
        let bin_path = right_state.bin_path.trim();
        if bin_path.is_empty() {
            let _ = flash_tx.send(FlashEvent::Finished(
                false,
                "未指定固件文件路径".to_string(),
            ));
            return;
        }

        let bin_data = match fs::read(bin_path) {
            Ok(bytes) => bytes,
            Err(e) => {
                let _ = flash_tx.send(FlashEvent::Finished(false, format!("固件读取失败: {}", e)));
                return;
            }
        };

        let addr = u32::from_str_radix(
            left_state
                .flash_address
                .trim_start_matches("0x")
                .trim_start_matches("0X"),
            16,
        )
        .unwrap_or(0x08020000);

        let tx_id = u32::from_str_radix(
            left_state
                .txid
                .trim_start_matches("0x")
                .trim_start_matches("0X"),
            16,
        )
        .unwrap_or(0x18FFFE32);

        let rx_id = u32::from_str_radix(
            left_state
                .rxid
                .trim_start_matches("0x")
                .trim_start_matches("0X"),
            16,
        )
        .unwrap_or(0x18FF32FE);

        let pad_byte = u8::from_str_radix(
            cfg.isotp_tx_padding
                .trim_start_matches("0x")
                .trim_start_matches("0X"),
            16,
        )
        .unwrap_or(0x55);

        let cur_prod = cfg.products.get(&left_state.selected_product);
        let flash_type_label = match left_state.flash_type_key.as_str() {
            "app" => "主APP程序",
            "mcu" => "从MCU程序",
            "boot" => "BOOT程序",
            _ => "主RTS程序",
        };

        let (erase_rid, verify_rid) = cur_prod
            .and_then(|p| p.flash_rid_overrides.get(flash_type_label))
            .map(|r| {
                let e = r
                    .erase_rid
                    .as_deref()
                    .and_then(|s| u16::from_str_radix(s.trim_start_matches("0x"), 16).ok())
                    .unwrap_or(0x1011);
                let v = r
                    .verify_rid
                    .as_deref()
                    .and_then(|s| u16::from_str_radix(s.trim_start_matches("0x"), 16).ok())
                    .unwrap_or(0x1012);
                (e, v)
            })
            .unwrap_or((0x1011, 0x1012));

        let algo_dll_path = cur_prod
            .and_then(|p| p.security_algo_or_cert.clone())
            .unwrap_or_else(|| "algos/rc4.dll".to_string());

        let signature_data = if effective_verify_method.eq_ignore_ascii_case("signature")
            && right_state.sig_mode == 1
            && !right_state.sig_path.trim().is_empty()
        {
            fs::read(right_state.sig_path.trim()).ok()
        } else {
            None
        };

        let params = FlashParams {
            bin_data,
            address: addr,
            tx_id,
            rx_id,
            tx_padding: pad_byte,
            sec_service: effective_sec_service.to_string(),
            algo_dll_path,
            erase_rid,
            verify_rid,
            verify_method: effective_verify_method.to_string(),
            signature: signature_data,
            auto_reboot: right_state.auto_reboot,
            flash_type_name: flash_type_label.to_string(),
        };

        engine.start(can_driver, bus_tx, flash_tx, params);
    }

    pub fn stop_flash(engine: &FlashEngine) {
        engine.stop();
    }
}
