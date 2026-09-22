use chrono::Local;
use crossbeam_channel::Sender;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::core::can::{create_adapter, CanAdapter};
use crate::i18n::t_fmt;
use crate::BusEvent;

pub struct CanController;

impl CanController {
    pub fn connect(
        can_driver: Arc<Mutex<Option<Box<dyn CanAdapter>>>>,
        bus_running: Arc<AtomicBool>,
        bus_tx: Sender<BusEvent>,
        interface: &str,
        channel: &str,
        baudrate_str: &str,
    ) -> Result<String, String> {
        let baud: u32 = baudrate_str.parse().unwrap_or(250000);
        let mut adapter = create_adapter(interface)?;

        adapter.open(channel, baud)?;
        let actual_ch = adapter.get_active_channel_name();

        {
            let mut lock = can_driver.lock().unwrap();
            *lock = Some(adapter);
        }

        bus_running.store(true, Ordering::SeqCst);
        let running_flag = bus_running.clone();
        let driver_ref = can_driver.clone();
        let bus_sender = bus_tx.clone();

        thread::spawn(move || {
            while running_flag.load(Ordering::SeqCst) {
                let frame_opt = {
                    if let Ok(mut drv_lock) = driver_ref.try_lock() {
                        if let Some(ref mut drv) = *drv_lock {
                            drv.receive(Duration::from_millis(5)).ok().flatten()
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                };

                if let Some(f) = frame_opt {
                    let time_now = Local::now().format("%H:%M:%S%.3f").to_string();
                    let id_str = if f.is_extended {
                        format!("0x{:08X}", f.id)
                    } else {
                        format!("0x{:03X}", f.id)
                    };
                    let data_hex: Vec<String> = f.data.iter().map(|b| format!("{:02X}", b)).collect();
                    let formatted = format!(
                        "{} RX {} [{}] {}",
                        time_now,
                        id_str,
                        f.data.len(),
                        data_hex.join(" ")
                    );

                    let _ = bus_sender.send(BusEvent::CanMsg {
                        id: f.id,
                        is_ext: f.is_extended,
                        is_rx: true,
                        formatted,
                    });
                }
                thread::sleep(Duration::from_millis(2));
            }
        });

        let time_str = Local::now().format("%H:%M:%S").to_string();
        let log_text = t_fmt(
            "log_conn_success",
            &[
                ("time", &time_str),
                ("hw", interface),
                ("ch", &actual_ch),
                ("baud", &baud.to_string()),
            ],
        );
        let _ = bus_tx.send(BusEvent::FlashLog(log_text, "info".to_string()));

        Ok(actual_ch)
    }

    pub fn disconnect(
        can_driver: Arc<Mutex<Option<Box<dyn CanAdapter>>>>,
        bus_running: Arc<AtomicBool>,
        bus_tx: Sender<BusEvent>,
    ) {
        bus_running.store(false, Ordering::SeqCst);
        if let Ok(mut lock) = can_driver.lock() {
            if let Some(ref mut drv) = *lock {
                drv.close();
            }
            *lock = None;
        }

        let time_str = Local::now().format("%H:%M:%S").to_string();
        let log_text = t_fmt("log_conn_disconnect", &[("time", &time_str)]);
        let _ = bus_tx.send(BusEvent::FlashLog(log_text, "info".to_string()));
    }
}
