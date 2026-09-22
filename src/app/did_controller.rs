use chrono::Local;
use crossbeam_channel::Sender;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::config::AppConfig;
use crate::core::algo_loader::compute_key_with_fallback;
use crate::core::can::{CanAdapter, CanFrame};
use crate::core::isotp::IsoTpHandler;
use crate::core::uds_service::{parse_did_payload, validate_and_build_did_payload};
use crate::i18n::{get_language, t, t_fmt};
use crate::ui::left_panel::LeftPanelState;
use crate::BusEvent;

pub struct DidController;

impl DidController {
    pub fn query_all_dids(
        can_driver: Arc<Mutex<Option<Box<dyn CanAdapter>>>>,
        bus_tx: Sender<BusEvent>,
        cfg: &AppConfig,
        left_state: &LeftPanelState,
    ) {
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

        let cur_lang = get_language();
        let mut did_items: Vec<(String, u16, String, String)> = cfg
            .dids
            .iter()
            .map(|(k, v)| {
                let i18n_k = format!("did_{}", k);
                let tr = t(&i18n_k);
                let label = if tr != i18n_k {
                    tr.trim_end_matches(':').to_string()
                } else if let Some(lang_name) = v.name_i18n.get(&cur_lang) {
                    lang_name.clone()
                } else {
                    v.name.clone()
                };
                (k.clone(), v.did, label, v.fmt.clone())
            })
            .collect();
        did_items.sort_by_key(|item| item.1);

        thread::spawn(move || {
            for (key, did, name, fmt) in did_items {
                let req_payload = [0x22, (did >> 8) as u8, (did & 0xFF) as u8];

                let result = {
                    let mut lock = can_driver.lock().unwrap();
                    if let Some(ref mut drv) = *lock {
                        let mut isotp = IsoTpHandler::new(&mut **drv, tx_id, rx_id, true, pad_byte);

                        let sender_for_monitor = bus_tx.clone();
                        isotp.set_monitor_callback(move |frame: &CanFrame, is_rx: bool| {
                            let time_now = Local::now().format("%H:%M:%S%.3f").to_string();
                            let dir = if is_rx { "RX" } else { "TX" };
                            let id_str = if frame.is_extended {
                                format!("0x{:08X}", frame.id)
                            } else {
                                format!("0x{:03X}", frame.id)
                            };
                            let data_hex: Vec<String> =
                                frame.data.iter().map(|b| format!("{:02X}", b)).collect();
                            let formatted = format!(
                                "{} {} {} [{}] {}",
                                time_now,
                                dir,
                                id_str,
                                frame.data.len(),
                                data_hex.join(" ")
                            );
                            let _ = sender_for_monitor.send(BusEvent::CanMsg {
                                id: frame.id,
                                is_ext: frame.is_extended,
                                is_rx,
                                formatted,
                            });
                        });

                        if isotp.send_payload(&req_payload).is_ok() {
                            isotp.receive_payload(Duration::from_millis(300))
                        } else {
                            Err("Send error".to_string())
                        }
                    } else {
                        Err("CAN offline".to_string())
                    }
                };

                let time_tag = Local::now().format("%H:%M:%S").to_string();
                let did_hex = format!("{:04X}", did);

                match result {
                    Ok(resp) => {
                        if resp.len() >= 3 && resp[0] == 0x62 {
                            let parsed = parse_did_payload(&fmt, &resp[3..]);
                            let log_text = t_fmt(
                                "log_did_query_success",
                                &[("time", &time_tag), ("name", &name), ("val", &parsed)],
                            );
                            let _ = bus_tx.send(BusEvent::FlashLog(log_text, "info".to_string()));
                            let _ = bus_tx.send(BusEvent::DidResult(key, parsed));
                        } else if resp.len() >= 3 && resp[0] == 0x7F {
                            let nrc_str = format!("{:02X}", resp[2]);
                            let log_text = t_fmt(
                                "log_did_query_unsupported",
                                &[
                                    ("time", &time_tag),
                                    ("name", &name),
                                    ("did", &did_hex),
                                    ("nrc", &nrc_str),
                                ],
                            );
                            let _ = bus_tx.send(BusEvent::FlashLog(log_text, "warn".to_string()));
                            let _ =
                                bus_tx.send(BusEvent::DidResult(key, "unsupported".to_string()));
                        }
                    }
                    Err(_) => {
                        let log_text = t_fmt(
                            "log_did_query_timeout",
                            &[("time", &time_tag), ("name", &name), ("did", &did_hex)],
                        );
                        let _ = bus_tx.send(BusEvent::FlashLog(log_text, "warn".to_string()));
                    }
                }
                thread::sleep(Duration::from_millis(25));
            }
        });
    }

    pub fn write_did(
        can_driver: Arc<Mutex<Option<Box<dyn CanAdapter>>>>,
        bus_tx: Sender<BusEvent>,
        cfg: &AppConfig,
        left_state: &LeftPanelState,
        key: String,
        val: String,
    ) -> Result<(), String> {
        let did_cfg = cfg
            .dids
            .get(&key)
            .ok_or_else(|| t("err_did_not_found"))?
            .clone();

        let payload = validate_and_build_did_payload(&key, &did_cfg.fmt, did_cfg.len, &val)
            .map_err(|err_key| t(err_key))?;

        let cur_lang = get_language();
        let display_name = {
            let i18n_k = format!("did_{}", key);
            let tr = t(&i18n_k);
            if tr != i18n_k {
                tr.trim_end_matches(':').to_string()
            } else if let Some(lang_name) = did_cfg.name_i18n.get(&cur_lang) {
                lang_name.clone()
            } else {
                did_cfg.name.clone()
            }
        };

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

        let algo_dll_path = cfg
            .products
            .get(&left_state.selected_product)
            .and_then(|p| p.security_algo_or_cert.clone())
            .unwrap_or_else(|| "algos/rc4.dll".to_string());

        let fmt_copy = did_cfg.fmt.clone();

        thread::spawn(move || {
            let mut lock = can_driver.lock().unwrap();
            let drv = match *lock {
                Some(ref mut d) => d,
                None => {
                    let _ = bus_tx.send(BusEvent::WriteNotice {
                        success: false,
                        msg: t("err_can_offline_write"),
                    });
                    return;
                }
            };

            let mut isotp = IsoTpHandler::new(&mut **drv, tx_id, rx_id, true, pad_byte);

            let sender_for_monitor = bus_tx.clone();
            isotp.set_monitor_callback(move |frame: &CanFrame, is_rx: bool| {
                let time_now = Local::now().format("%H:%M:%S%.3f").to_string();
                let dir = if is_rx { "RX" } else { "TX" };
                let id_str = if frame.is_extended {
                    format!("0x{:08X}", frame.id)
                } else {
                    format!("0x{:03X}", frame.id)
                };
                let data_hex: Vec<String> =
                    frame.data.iter().map(|b| format!("{:02X}", b)).collect();
                let formatted = format!(
                    "{} {} {} [{}] {}",
                    time_now,
                    dir,
                    id_str,
                    frame.data.len(),
                    data_hex.join(" ")
                );
                let _ = sender_for_monitor.send(BusEvent::CanMsg {
                    id: frame.id,
                    is_ext: frame.is_extended,
                    is_rx,
                    formatted,
                });
            });

            let time_tag = Local::now().format("%H:%M:%S").to_string();
            let log_start = format!(
                "[{}] INFO | {}",
                time_tag,
                t_fmt("log_did_write_preparing", &[("name", &display_name)])
            );
            let _ = bus_tx.send(BusEvent::FlashLog(log_start, "info".to_string()));

            // ================= 步骤 1: 扩展诊断会话 (0x10 0x03) =================
            if isotp.send_payload(&[0x10, 0x03]).is_err() {
                let _ = bus_tx.send(BusEvent::WriteNotice {
                    success: false,
                    msg: t_fmt("err_send_ext_session", &[("name", &display_name)]),
                });
                return;
            }
            match isotp.receive_payload(Duration::from_millis(600)) {
                Ok(resp) => {
                    if resp.is_empty() || resp[0] != 0x50 {
                        let resp_str = format!("{:02X?}", resp);
                        let _ = bus_tx.send(BusEvent::WriteNotice {
                            success: false,
                            msg: t_fmt(
                                "err_ext_session_rejected",
                                &[("name", &display_name), ("resp", &resp_str)],
                            ),
                        });
                        return;
                    }
                }
                Err(e) => {
                    let _ = bus_tx.send(BusEvent::WriteNotice {
                        success: false,
                        msg: t_fmt(
                            "err_ext_session_timeout",
                            &[("name", &display_name), ("err", &e)],
                        ),
                    });
                    return;
                }
            }

            // ================= 步骤 2: 安全访问 - 请求种子 (0x27 0x01) =================
            if isotp.send_payload(&[0x27, 0x01]).is_err() {
                let _ = bus_tx.send(BusEvent::WriteNotice {
                    success: false,
                    msg: t_fmt("err_send_seed_req", &[("name", &display_name)]),
                });
                let _ = isotp.send_payload(&[0x10, 0x01]);
                return;
            }

            let seed = match isotp.receive_payload(Duration::from_millis(600)) {
                Ok(resp) => {
                    if resp.len() >= 3 && resp[0] == 0x67 && resp[1] == 0x01 {
                        resp[2..].to_vec()
                    } else {
                        let detail = if resp.len() >= 3 && resp[0] == 0x7F {
                            format!("NRC: 0x{:02X}", resp[2])
                        } else {
                            format!("{:02X?}", resp)
                        };
                        let _ = bus_tx.send(BusEvent::WriteNotice {
                            success: false,
                            msg: t_fmt(
                                "err_seed_req_failed",
                                &[("name", &display_name), ("detail", &detail)],
                            ),
                        });
                        let _ = isotp.send_payload(&[0x10, 0x01]);
                        return;
                    }
                }
                Err(e) => {
                    let _ = bus_tx.send(BusEvent::WriteNotice {
                        success: false,
                        msg: t_fmt(
                            "err_seed_req_timeout",
                            &[("name", &display_name), ("err", &e)],
                        ),
                    });
                    let _ = isotp.send_payload(&[0x10, 0x01]);
                    return;
                }
            };

            let is_already_unlocked = seed.iter().all(|&b| b == 0);
            if !is_already_unlocked {
                let (key_bytes, warn_msg) = compute_key_with_fallback(&algo_dll_path, 1, &seed, 4);
                if let Some(warn) = warn_msg {
                    let _ = bus_tx.send(BusEvent::FlashLog(
                        format!("[{}] WARN | {}", time_tag, warn),
                        "warn".to_string(),
                    ));
                }

                // ================= 步骤 3: 安全访问 - 发送密钥 (0x27 0x02) =================
                let mut send_key_payload = vec![0x27, 0x02];
                send_key_payload.extend_from_slice(&key_bytes);
                if isotp.send_payload(&send_key_payload).is_err() {
                    let _ = bus_tx.send(BusEvent::WriteNotice {
                        success: false,
                        msg: t_fmt("err_send_key_failed", &[("name", &display_name)]),
                    });
                    let _ = isotp.send_payload(&[0x10, 0x01]);
                    return;
                }

                match isotp.receive_payload(Duration::from_millis(600)) {
                    Ok(resp) => {
                        if resp.len() < 2 || resp[0] != 0x67 || resp[1] != 0x02 {
                            let nrc = if resp.len() >= 3 && resp[0] == 0x7F {
                                format!("0x{:02X}", resp[2])
                            } else {
                                format!("{:02X?}", resp)
                            };
                            let _ = bus_tx.send(BusEvent::WriteNotice {
                                success: false,
                                msg: t_fmt(
                                    "err_key_verify_failed",
                                    &[("name", &display_name), ("nrc", &nrc)],
                                ),
                            });
                            let _ = isotp.send_payload(&[0x10, 0x01]);
                            return;
                        }
                    }
                    Err(e) => {
                        let _ = bus_tx.send(BusEvent::WriteNotice {
                            success: false,
                            msg: t_fmt(
                                "err_key_verify_timeout",
                                &[("name", &display_name), ("err", &e)],
                            ),
                        });
                        let _ = isotp.send_payload(&[0x10, 0x01]);
                        return;
                    }
                }
            }

            // ================= 步骤 4: 执行 DID 写入 (0x2E <DID> <Payload>) =================
            let mut write_req = vec![0x2E, (did_cfg.did >> 8) as u8, (did_cfg.did & 0xFF) as u8];
            write_req.extend_from_slice(&payload);

            if isotp.send_payload(&write_req).is_err() {
                let _ = bus_tx.send(BusEvent::WriteNotice {
                    success: false,
                    msg: t_fmt("err_send_write_req", &[("name", &display_name)]),
                });
                let _ = isotp.send_payload(&[0x10, 0x01]);
                return;
            }

            let write_success = match isotp.receive_payload(Duration::from_millis(800)) {
                Ok(resp) => {
                    if resp.len() >= 3 && resp[0] == 0x6E {
                        true
                    } else {
                        let nrc = if resp.len() >= 3 && resp[0] == 0x7F {
                            format!("0x{:02X}", resp[2])
                        } else {
                            format!("{:02X?}", resp)
                        };
                        let _ = bus_tx.send(BusEvent::WriteNotice {
                            success: false,
                            msg: t_fmt("err_write_nrc", &[("name", &display_name), ("nrc", &nrc)]),
                        });
                        false
                    }
                }
                Err(e) => {
                    let _ = bus_tx.send(BusEvent::WriteNotice {
                        success: false,
                        msg: t_fmt("err_write_timeout", &[("name", &display_name), ("err", &e)]),
                    });
                    false
                }
            };

            // ================= 步骤 5: 切回默认会话 (0x10 0x01) =================
            let _ = isotp.send_payload(&[0x10, 0x01]);
            let _ = isotp.receive_payload(Duration::from_millis(300));

            // ================= 步骤 6: 写入成功后回读并弹窗 =================
            if write_success {
                let log_ok = format!(
                    "[{}] INFO | {}",
                    time_tag,
                    t_fmt("log_did_write_ok", &[("name", &display_name)])
                );
                let _ = bus_tx.send(BusEvent::FlashLog(log_ok, "info".to_string()));

                thread::sleep(Duration::from_millis(40));
                let read_req = [0x22, (did_cfg.did >> 8) as u8, (did_cfg.did & 0xFF) as u8];
                if isotp.send_payload(&read_req).is_ok() {
                    if let Ok(resp) = isotp.receive_payload(Duration::from_millis(400)) {
                        if resp.len() >= 3 && resp[0] == 0x62 {
                            let verified_val = parse_did_payload(&fmt_copy, &resp[3..]);
                            let _ = bus_tx.send(BusEvent::DidResult(key.clone(), verified_val));
                        }
                    }
                }

                let _ = bus_tx.send(BusEvent::WriteNotice {
                    success: true,
                    msg: t_fmt("log_did_write_ok", &[("name", &display_name)]),
                });
            }
        });

        Ok(())
    }
}
