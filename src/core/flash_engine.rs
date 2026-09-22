use chrono::Local;
use crossbeam_channel::Sender;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::core::algo_loader::compute_key_with_fallback;
use crate::core::can::{CanAdapter, CanFrame};
use crate::core::isotp::IsoTpHandler;
use crate::i18n::{t, t_fmt};
use crate::BusEvent;

const MAX_TRANSFER_RETRY: usize = 10;
const RETRY_DELAY_MS: u64 = 50;

pub enum FlashEvent {
    Log(String, String),
    Progress(f32, String),
    Finished(bool, String),
}

#[derive(Clone)]
pub struct FlashEngine {
    pub running: Arc<AtomicBool>,
}

impl Default for FlashEngine {
    fn default() -> Self {
        Self::new()
    }
}

pub struct FlashParams {
    pub bin_data: Vec<u8>,
    pub address: u32,
    pub tx_id: u32,
    pub rx_id: u32,
    pub tx_padding: u8,
    pub sec_service: String,
    pub algo_dll_path: String,
    pub erase_rid: u16,
    pub verify_rid: u16,
    pub verify_method: String,
    pub signature: Option<Vec<u8>>,
    pub auto_reboot: bool,
    pub flash_type_name: String,
}

impl FlashEngine {
    pub fn new() -> Self {
        Self {
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn start(
        &self,
        can_driver: Arc<Mutex<Option<Box<dyn CanAdapter>>>>,
        bus_tx: Sender<BusEvent>,
        flash_tx: Sender<FlashEvent>,
        params: FlashParams,
    ) {
        self.running.store(true, Ordering::SeqCst);
        let running_flag = self.running.clone();

        thread::spawn(move || {
            let log = |msg: String, lvl: &str| {
                let time_str = Local::now().format("%H:%M:%S").to_string();
                let full_msg = format!("[{}] {} | {}", time_str, lvl.to_uppercase(), msg);
                let _ = flash_tx.send(FlashEvent::Log(full_msg, lvl.to_string()));
            };

            log(t("fl_start"), "info");
            let addr_str = format!("0x{:08X}", params.address);
            log(
                t_fmt(
                    "fl_addr_switched",
                    &[("addr", &addr_str), ("type", &params.flash_type_name)],
                ),
                "info",
            );

            let mut drv_guard = can_driver.lock().unwrap();
            let drv = match *drv_guard {
                Some(ref mut d) => d,
                None => {
                    let err_msg = t("err_can_offline_write");
                    log(err_msg.clone(), "error");
                    let _ = flash_tx.send(FlashEvent::Finished(false, err_msg));
                    running_flag.store(false, Ordering::SeqCst);
                    return;
                }
            };

            let mut isotp = IsoTpHandler::new(
                &mut **drv,
                params.tx_id,
                params.rx_id,
                true,
                params.tx_padding,
            );

            let bus_sender = bus_tx.clone();
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
                let _ = bus_sender.send(BusEvent::CanMsg {
                    id: frame.id,
                    is_ext: frame.is_extended,
                    is_rx,
                    formatted,
                });
            });

            // 1. 切换到扩展会话 (0x10 0x03)
            log(t("fl_req_ext_mode"), "info");
            log(t("fl_step1_ext"), "info");
            if isotp.send_payload(&[0x10, 0x03]).is_err() {
                log(t("fl_err_ext"), "error");
                let _ = flash_tx.send(FlashEvent::Finished(false, t("fl_err_ext")));
                running_flag.store(false, Ordering::SeqCst);
                return;
            }
            match isotp.receive_payload(Duration::from_millis(800)) {
                Ok(resp) if resp.first() == Some(&0x50) => {}
                _ => {
                    log(t("fl_err_ext"), "error");
                    let _ = flash_tx.send(FlashEvent::Finished(false, t("fl_err_ext")));
                    running_flag.store(false, Ordering::SeqCst);
                    return;
                }
            }

            // 2. 关闭正常报文发送 (0x28 0x03 0x01)
            log(t("fl_step2_comm_ctrl"), "info");
            let _ = isotp.send_payload(&[0x28, 0x03, 0x01]);
            let _ = isotp.receive_payload(Duration::from_millis(300));

            // 3. 切换到编程会话 (0x10 0x02)
            log(t("fl_step3_prog"), "info");
            if isotp.send_payload(&[0x10, 0x02]).is_err() {
                log(t("fl_err_prog"), "error");
                let _ = flash_tx.send(FlashEvent::Finished(false, t("fl_err_prog")));
                running_flag.store(false, Ordering::SeqCst);
                return;
            }
            match isotp.receive_payload(Duration::from_millis(800)) {
                Ok(resp) if resp.first() == Some(&0x50) => {}
                _ => {
                    log(t("fl_err_prog"), "error");
                    let _ = flash_tx.send(FlashEvent::Finished(false, t("fl_err_prog")));
                    running_flag.store(false, Ordering::SeqCst);
                    return;
                }
            }

            // 4. 请求安全访问 (0x27)
            log(t("fl_step4_sec"), "info");
            if isotp.send_payload(&[0x27, 0x01]).is_err() {
                log(t("fl_err_seed"), "error");
                let _ = flash_tx.send(FlashEvent::Finished(false, t("fl_err_seed")));
                running_flag.store(false, Ordering::SeqCst);
                return;
            }

            let seed = match isotp.receive_payload(Duration::from_millis(800)) {
                Ok(resp) if resp.len() >= 3 && resp[0] == 0x67 && resp[1] == 0x01 => {
                    resp[2..].to_vec()
                }
                _ => {
                    log(t("fl_err_seed"), "error");
                    let _ = flash_tx.send(FlashEvent::Finished(false, t("fl_err_seed")));
                    running_flag.store(false, Ordering::SeqCst);
                    return;
                }
            };

            if !seed.iter().all(|&b| b == 0) {
                let (key, warn) = compute_key_with_fallback(&params.algo_dll_path, 1, &seed, 4);
                if let Some(w) = warn {
                    log(w, "warn");
                }
                let mut send_key = vec![0x27, 0x02];
                send_key.extend_from_slice(&key);
                if isotp.send_payload(&send_key).is_err() {
                    log(t("fl_err_key"), "error");
                    let _ = flash_tx.send(FlashEvent::Finished(false, t("fl_err_key")));
                    running_flag.store(false, Ordering::SeqCst);
                    return;
                }
                match isotp.receive_payload(Duration::from_millis(800)) {
                    Ok(resp) if resp.len() >= 2 && resp[0] == 0x67 && resp[1] == 0x02 => {}
                    _ => {
                        log(t("fl_err_key"), "error");
                        let _ = flash_tx.send(FlashEvent::Finished(false, t("fl_err_key")));
                        running_flag.store(false, Ordering::SeqCst);
                        return;
                    }
                }
            }

            // 5. 正在擦除 Flash
            let rid_hex = format!("{:04X}", params.erase_rid);
            log(t_fmt("fl_step5_erase", &[("rid", &rid_hex)]), "info");

            let total_size = params.bin_data.len() as u32;
            let mut erase_req = vec![
                0x31,
                0x01,
                (params.erase_rid >> 8) as u8,
                (params.erase_rid & 0xFF) as u8,
                0x44,
            ];
            erase_req.extend_from_slice(&params.address.to_be_bytes());
            erase_req.extend_from_slice(&total_size.to_be_bytes());

            if isotp.send_payload(&erase_req).is_err() {
                log(t("fl_err_erase"), "error");
                let _ = flash_tx.send(FlashEvent::Finished(false, t("fl_err_erase")));
                running_flag.store(false, Ordering::SeqCst);
                return;
            }

            match isotp.receive_payload(Duration::from_millis(5000)) {
                Ok(resp) if resp.len() >= 4 && resp[0] == 0x71 => {}
                _ => {
                    log(t("fl_err_erase"), "error");
                    let _ = flash_tx.send(FlashEvent::Finished(false, t("fl_err_erase")));
                    running_flag.store(false, Ordering::SeqCst);
                    return;
                }
            }

            // 6. 请求下载 (0x34)
            log(t("fl_step6_req_dl"), "info");
            let mut req_dl = vec![0x34, 0x00, 0x44];
            req_dl.extend_from_slice(&params.address.to_be_bytes());
            req_dl.extend_from_slice(&total_size.to_be_bytes());

            if isotp.send_payload(&req_dl).is_err() {
                log(t("fl_err_dl"), "error");
                let _ = flash_tx.send(FlashEvent::Finished(false, t("fl_err_dl")));
                running_flag.store(false, Ordering::SeqCst);
                return;
            }

            let max_block_size = match isotp.receive_payload(Duration::from_millis(1000)) {
                Ok(resp) if resp.first() == Some(&0x74) => {
                    if resp.len() >= 4 {
                        let len = ((resp[resp.len() - 2] as usize) << 8)
                            | (resp[resp.len() - 1] as usize);
                        if len > 2 {
                            (len - 2).min(1024)
                        } else {
                            1024
                        }
                    } else {
                        1024
                    }
                }
                _ => {
                    log(t("fl_err_dl"), "error");
                    let _ = flash_tx.send(FlashEvent::Finished(false, t("fl_err_dl")));
                    running_flag.store(false, Ordering::SeqCst);
                    return;
                }
            };

            // 7. 开始传输数据 (0x36, 支持最大 10 次重试机制)
            let block_size = max_block_size;
            let total_chunks = (total_size as usize + block_size - 1) / block_size;
            log(t("fl_step7_transfer"), "info");
            log(
                t_fmt(
                    "fl_transfer_info",
                    &[
                        ("total", &total_chunks.to_string()),
                        ("size", &block_size.to_string()),
                    ],
                ),
                "info",
            );

            let start_time = Instant::now();
            let mut block_counter = 1u8;

            for (chunk_idx, offset) in (0..params.bin_data.len()).step_by(block_size).enumerate() {
                if !running_flag.load(Ordering::SeqCst) {
                    log(t("fl_abort_user"), "warn");
                    let _ = flash_tx.send(FlashEvent::Finished(false, t("fl_abort_user")));
                    return;
                }

                let end = (offset + block_size).min(params.bin_data.len());
                let chunk_data = &params.bin_data[offset..end];

                let mut transfer_payload = vec![0x36, block_counter];
                transfer_payload.extend_from_slice(chunk_data);

                let curr_pkg = (chunk_idx + 1).to_string();
                let mut success = false;

                for retry in 0..MAX_TRANSFER_RETRY {
                    if retry > 0 {
                        log(
                            t_fmt(
                                "fl_err_chunk_retry",
                                &[
                                    ("curr", &curr_pkg),
                                    ("total", &total_chunks.to_string()),
                                    ("retry", &retry.to_string()),
                                    ("max", &MAX_TRANSFER_RETRY.to_string()),
                                ],
                            ),
                            "warn",
                        );
                        thread::sleep(Duration::from_millis(RETRY_DELAY_MS));
                    }

                    if isotp.send_payload(&transfer_payload).is_err() {
                        continue;
                    }

                    match isotp.receive_payload(Duration::from_millis(1500)) {
                        Ok(resp) if resp.len() >= 2 && resp[0] == 0x76 && resp[1] == block_counter => {
                            success = true;
                            break;
                        }
                        _ => {}
                    }
                }

                if !success {
                    let err_msg = t_fmt(
                        "fl_err_chunk_max_retry",
                        &[
                            ("curr", &curr_pkg),
                            ("max", &MAX_TRANSFER_RETRY.to_string()),
                        ],
                    );
                    log(err_msg.clone(), "error");
                    let _ = flash_tx.send(FlashEvent::Finished(false, err_msg));
                    running_flag.store(false, Ordering::SeqCst);
                    return;
                }

                block_counter = block_counter.wrapping_add(1);

                let percent = ((chunk_idx + 1) as f32 / total_chunks as f32) * 100.0;
                let elapsed = start_time.elapsed().as_secs();
                let status = format!("{:02}:{:02} Sent {} Bytes", elapsed / 60, elapsed % 60, end);
                let _ = flash_tx.send(FlashEvent::Progress(percent, status));

                let pct_str = format!("{:.0}", percent);
                log(
                    t_fmt(
                        "fl_chunk_done",
                        &[
                            ("curr", &curr_pkg),
                            ("total", &total_chunks.to_string()),
                            ("pct", &pct_str),
                        ],
                    ),
                    "info",
                );
            }

            log(
                t_fmt(
                    "fl_all_chunks_done",
                    &[("total", &total_chunks.to_string())],
                ),
                "info",
            );

            // 退出传输 (0x37)
            let _ = isotp.send_payload(&[0x37]);
            let _ = isotp.receive_payload(Duration::from_millis(500));

            // 8. 校验 Flash
            let verify_rid_hex = format!("{:04X}", params.verify_rid);
            log(
                t_fmt("fl_step8_verify", &[("rid", &verify_rid_hex)]),
                "info",
            );

            let mut verify_req = vec![
                0x31,
                0x01,
                (params.verify_rid >> 8) as u8,
                (params.verify_rid & 0xFF) as u8,
            ];

            if params.verify_method.eq_ignore_ascii_case("signature") {
                if let Some(ref sig) = params.signature {
                    verify_req.extend_from_slice(sig);
                }
            } else {
                let mut hasher = crc32fast::Hasher::new();
                hasher.update(&params.bin_data);
                verify_req.extend_from_slice(&hasher.finalize().to_be_bytes());
            }

            if isotp.send_payload(&verify_req).is_err() {
                log(t("fl_err_verify"), "error");
                let _ = flash_tx.send(FlashEvent::Finished(false, t("fl_err_verify")));
                running_flag.store(false, Ordering::SeqCst);
                return;
            }

            match isotp.receive_payload(Duration::from_millis(4000)) {
                Ok(resp) if resp.first() == Some(&0x71) => {}
                _ => {
                    log(t("fl_err_verify"), "error");
                    let _ = flash_tx.send(FlashEvent::Finished(false, t("fl_err_verify")));
                    running_flag.store(false, Ordering::SeqCst);
                    return;
                }
            }

            // 9. 恢复正常通信
            log(t("fl_step9_restore_comm"), "info");
            let _ = isotp.send_payload(&[0x28, 0x00, 0x01]);
            let _ = isotp.receive_payload(Duration::from_millis(300));

            // 10. 重启或切回默认会话
            if params.auto_reboot {
                log(t("fl_step10_reboot"), "info");
                let _ = isotp.send_payload(&[0x11, 0x01]);
                let _ = isotp.receive_payload(Duration::from_millis(500));
            } else {
                log(t("fl_step10_default_sess"), "info");
                let _ = isotp.send_payload(&[0x10, 0x01]);
                let _ = isotp.receive_payload(Duration::from_millis(300));
            }

            log(t("fl_success"), "info");
            let _ = flash_tx.send(FlashEvent::Progress(100.0, "Done".into()));
            let _ = flash_tx.send(FlashEvent::Finished(true, t("fl_success")));
            running_flag.store(false, Ordering::SeqCst);
        });
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }
}