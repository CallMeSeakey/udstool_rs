// src/core/isotp.rs
use crate::core::can::{CanFrame, CanAdapter};
use std::time::{Duration, Instant};

pub struct IsoTpHandler<'a> {
    adapter: &'a mut dyn CanAdapter,
    tx_id: u32,
    rx_id: u32,
    is_extended: bool,
    tx_padding: u8,
    pub block_size: u8,
    pub st_min: Duration,
    monitor_cb: Option<Box<dyn FnMut(&CanFrame, bool) + 'a>>,
}

impl<'a> IsoTpHandler<'a> {
    pub fn new(
        adapter: &'a mut dyn CanAdapter,
        tx_id: u32,
        rx_id: u32,
        is_extended: bool,
        tx_padding: u8,
    ) -> Self {
        Self {
            adapter,
            tx_id,
            rx_id,
            is_extended,
            tx_padding,
            block_size: 0,
            st_min: Duration::from_micros(500),
            monitor_cb: None,
        }
    }

    pub fn set_monitor_callback(&mut self, cb: impl FnMut(&CanFrame, bool) + 'a) {
        self.monitor_cb = Some(Box::new(cb));
    }

    fn send_frame(&mut self, frame: &CanFrame) -> Result<(), String> {
        if let Some(ref mut cb) = self.monitor_cb {
            cb(frame, false);
        }
        self.adapter.send(frame)
    }

    pub fn send_payload(&mut self, payload: &[u8]) -> Result<(), String> {
        let len = payload.len();
        if len <= 7 {
            let mut data = vec![len as u8];
            data.extend_from_slice(payload);
            while data.len() < 8 {
                data.push(self.tx_padding);
            }
            self.send_frame(&CanFrame {
                id: self.tx_id,
                is_extended: self.is_extended,
                data,
                is_rx: false,
                timestamp_us: 0,
            })?;
        } else {
            // 1. 发送首帧 (First Frame)
            let mut data = vec![0x10 | ((len >> 8) & 0x0F) as u8, (len & 0xFF) as u8];
            data.extend_from_slice(&payload[..6]);
            self.send_frame(&CanFrame {
                id: self.tx_id,
                is_extended: self.is_extended,
                data,
                is_rx: false,
                timestamp_us: 0,
            })?;

            // 2. 等待 ECU 返回首次流控，动态解析 ECU 指定的 BS 和 STmin
            self.wait_flow_control(Duration::from_millis(1500))?;

            let mut seq = 1u8;
            let mut offset = 6;
            let mut frames_sent_in_block = 0u8;

            while offset < len {
                // 若 ECU 返回的 BS > 0，且发满该 block 数量，则必须停下来等待 ECU 再次反馈流控
                if self.block_size > 0 && frames_sent_in_block >= self.block_size {
                    self.wait_flow_control(Duration::from_millis(1500))?;
                    frames_sent_in_block = 0;
                }

                let chunk_size = (len - offset).min(7);
                let mut cf_data = vec![0x20 | (seq & 0x0F)];
                cf_data.extend_from_slice(&payload[offset..offset + chunk_size]);
                while cf_data.len() < 8 {
                    cf_data.push(self.tx_padding);
                }

                self.send_frame(&CanFrame {
                    id: self.tx_id,
                    is_extended: self.is_extended,
                    data: cf_data,
                    is_rx: false,
                    timestamp_us: 0,
                })?;

                seq = (seq + 1) % 16;
                offset += chunk_size;
                frames_sent_in_block += 1;

                // 严格执行帧间隔延时（STmin=0 时保底 500us，其余严格按 ECU 要求）
                if self.st_min.as_micros() > 0 {
                    std::thread::sleep(self.st_min);
                }
            }
        }
        Ok(())
    }

    pub fn receive_payload(&mut self, timeout: Duration) -> Result<Vec<u8>, String> {
        let start = Instant::now();
        let mut expected_len = 0usize;
        let mut buffer = Vec::new();
        let mut next_seq = 1u8;

        while start.elapsed() < timeout {
            if let Some(frame) = self.adapter.receive(Duration::from_millis(10))? {
                if let Some(ref mut cb) = self.monitor_cb {
                    cb(&frame, true);
                }

                if frame.id != self.rx_id || frame.data.is_empty() {
                    continue;
                }

                let pci = frame.data[0] >> 4;
                match pci {
                    0x0 => {
                        let len = (frame.data[0] & 0x0F) as usize;
                        if frame.data.len() >= 1 + len {
                            return Ok(frame.data[1..=len].to_vec());
                        }
                    }
                    0x1 => {
                        expected_len =
                            (((frame.data[0] & 0x0F) as usize) << 8) | (frame.data[1] as usize);
                        buffer.extend_from_slice(&frame.data[2..8.min(frame.data.len())]);
                        let fc_data = vec![
                            0x30,
                            0x00,
                            0x00,
                            self.tx_padding,
                            self.tx_padding,
                            self.tx_padding,
                            self.tx_padding,
                            self.tx_padding,
                        ];
                        self.send_frame(&CanFrame {
                            id: self.tx_id,
                            is_extended: self.is_extended,
                            data: fc_data,
                            is_rx: false,
                            timestamp_us: 0,
                        })?;
                    }
                    0x2 => {
                        let seq = frame.data[0] & 0x0F;
                        if seq == next_seq {
                            let remain = expected_len - buffer.len();
                            let take_len = remain.min(7).min(frame.data.len() - 1);
                            buffer.extend_from_slice(&frame.data[1..1 + take_len]);
                            next_seq = (next_seq + 1) % 16;

                            if buffer.len() >= expected_len {
                                return Ok(buffer);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        Err("ISO-TP response timeout".to_string())
    }

    /// 等待流控帧，严格按 ECU 实际报文解析 BS 和 STmin，并在 STmin=0 时启用 200us 保底
    fn wait_flow_control(&mut self, timeout: Duration) -> Result<(), String> {
        let start = Instant::now();
        let stmin_default:u64 = 200;

        while start.elapsed() < timeout {
            if let Some(frame) = self.adapter.receive(Duration::from_millis(10))? {
                if let Some(ref mut cb) = self.monitor_cb {
                    cb(&frame, true);
                }

                if frame.id == self.rx_id && !frame.data.is_empty() && (frame.data[0] >> 4) == 0x3 {
                    let flow_status = frame.data[0] & 0x0F;
                    if flow_status == 0 {
                        // 1. 动态按 ECU 给出的块大小（data[1]）决定
                        self.block_size = if frame.data.len() > 1 {
                            frame.data[1]
                        } else {
                            0
                        };

                        // 2. 动态按 ECU 给出的时间间隔（data[2]）决定
                        let st_raw = if frame.data.len() > 2 {
                            frame.data[2]
                        } else {
                            0
                        };

                        self.st_min = if st_raw == 0 {
                            // 当 ECU 设定 STmin == 0 时，上位机保底插入 200us 延时
                            Duration::from_micros(stmin_default)
                        } else if (1..=127).contains(&st_raw) {
                            // 1ms ~ 127ms
                            Duration::from_millis(st_raw as u64)
                        } else if (0xF1..=0xF9).contains(&st_raw) {
                            // 100us ~ 900us
                            Duration::from_micros(((st_raw - 0xF0) as u64) * 100)
                        } else {
                            Duration::from_micros(stmin_default)
                        };

                        return Ok(());
                    } else if flow_status == 1 {
                        // Wait 帧 (0x31)，下位机繁忙，继续等待下一个 CTS 帧
                        continue;
                    } else {
                        // Overflow / 拒绝
                        return Err("Flow control overflow or aborted by ECU".to_string());
                    }
                }
            }
        }
        Err("Wait flow control timeout".to_string())
    }
}