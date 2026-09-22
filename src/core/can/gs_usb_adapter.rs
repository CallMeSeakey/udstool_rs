// src/core/can/gs_usb_adapter.rs
use super::{CanAdapter, CanFrame};
use rusb::{Context, DeviceHandle, Direction, TransferType, UsbContext};
use std::time::Duration;

const GS_USB_BREQ_HOST_FORMAT: u8 = 0;
const GS_USB_BREQ_BITTIMING: u8 = 1;
const GS_USB_BREQ_MODE: u8 = 2;

const GS_CAN_MODE_START: u32 = 0x00000001;
const GS_CAN_MODE_RESET: u32 = 0x00000000;

// gs_usb 官方帧标记
const GS_CAN_FLAG_OVERFLOW: u8 = 1 << 0;
const GS_CAN_FLAG_FD: u8 = 1 << 1;
const GS_CAN_FLAG_BRS: u8 = 1 << 2;
const GS_CAN_FLAG_ESI: u8 = 1 << 3;

// 特殊 Echo 标记 (0xFFFFFFFF 代表真实的 RX 帧，非 echo)
const GS_USB_NONE_ECHO_ID: u32 = 0xFFFFFFFF;

const GS_USB_DEVICES: &[(u16, u16)] = &[
    (0x1d50, 0x606f), // candleLight open hardware
    (0x1209, 0x2323), // Geschwister Schneider USB/CAN
    (0x1cd5, 0x0001), // Cando
];

#[repr(C, packed)]
#[derive(Copy, Clone, Default)]
struct GsDeviceMode {
    mode: u32,
    flags: u32,
}

#[repr(C, packed)]
#[derive(Copy, Clone, Default)]
struct GsDeviceBittiming {
    prop_seg: u32,
    phase_seg1: u32,
    phase_seg2: u32,
    sjw: u32,
    brp: u32,
}

#[repr(C, packed)]
#[derive(Copy, Clone, Default)]
struct GsHostFormat {
    byte_order: u32,
}

pub fn get_channels() -> Vec<String> {
    vec!["Channel 0".to_string(), "Channel 1".to_string()]
}

pub struct GsUsbAdapter {
    handle: Option<DeviceHandle<Context>>,
    ep_in: u8,
    ep_out: u8,
    channel_name: String,
    opened: bool,
    echo_counter: u32,
}

impl GsUsbAdapter {
    pub fn try_load() -> Result<Self, String> {
        let context = Context::new().map_err(|e| format!("初始化 USB 上下文失败: {}", e))?;
        let devices = context.devices().map_err(|e| format!("枚举 USB 设备失败: {}", e))?;

        let mut found_dev = None;
        for dev in devices.iter() {
            if let Ok(desc) = dev.device_descriptor() {
                for &(vid, pid) in GS_USB_DEVICES {
                    if desc.vendor_id() == vid && desc.product_id() == pid {
                        found_dev = Some(dev);
                        break;
                    }
                }
            }
            if found_dev.is_some() {
                break;
            }
        }

        let dev = found_dev.ok_or_else(|| "未检测到 gs_usb / candleLight 硬件设备".to_string())?;
        let handle = dev.open().map_err(|e| format!("打开 gs_usb 设备失败: {}", e))?;

        let _ = handle.set_auto_detach_kernel_driver(true);
        let config = dev.active_config_descriptor().map_err(|e| e.to_string())?;

        let mut ep_in = 0x81;
        let mut ep_out = 0x02;

        if let Some(interface) = config.interfaces().next() {
            if let Some(desc) = interface.descriptors().next() {
                let _ = handle.claim_interface(desc.interface_number());
                for ep in desc.endpoint_descriptors() {
                    if ep.transfer_type() == TransferType::Bulk {
                        if ep.direction() == Direction::In {
                            ep_in = ep.address();
                        } else {
                            ep_out = ep.address();
                        }
                    }
                }
            }
        }

        Ok(Self {
            handle: Some(handle),
            ep_in,
            ep_out,
            channel_name: "Channel 0".to_string(),
            opened: false,
            echo_counter: 0,
        })
    }

    /// 适配常见 48MHz / 80MHz 时钟的标准 75%~80% 采样点
    fn baudrate_to_timing(baudrate: u32) -> GsDeviceBittiming {
        match baudrate {
            1000000 => GsDeviceBittiming { prop_seg: 1, phase_seg1: 4, phase_seg2: 3, sjw: 1, brp: 6 },
            500000  => GsDeviceBittiming { prop_seg: 1, phase_seg1: 6, phase_seg2: 5, sjw: 1, brp: 6 },
            250000  => GsDeviceBittiming { prop_seg: 1, phase_seg1: 6, phase_seg2: 5, sjw: 1, brp: 12 },
            125000  => GsDeviceBittiming { prop_seg: 1, phase_seg1: 6, phase_seg2: 5, sjw: 1, brp: 24 },
            _       => GsDeviceBittiming { prop_seg: 1, phase_seg1: 4, phase_seg2: 3, sjw: 1, brp: 6 },
        }
    }
}

impl CanAdapter for GsUsbAdapter {
    fn open(&mut self, channel: &str, bitrate: u32) -> Result<(), String> {
        self.close();

        let handle = self.handle.as_mut().ok_or("设备句柄不存在")?;

        // 1. 设置字节序 (Host Format)
        let host_format = GsHostFormat { byte_order: 0xEFBE0000 };
        let buf_host = unsafe {
            std::slice::from_raw_parts(&host_format as *const _ as *const u8, std::mem::size_of::<GsHostFormat>())
        };
        let _ = handle.write_control(0x41, GS_USB_BREQ_HOST_FORMAT, 0, 0, buf_host, Duration::from_millis(500));

        // 2. 发送波特率配置
        let timing = Self::baudrate_to_timing(bitrate);
        let buf_timing = unsafe {
            std::slice::from_raw_parts(&timing as *const _ as *const u8, std::mem::size_of::<GsDeviceBittiming>())
        };
        handle.write_control(0x41, GS_USB_BREQ_BITTIMING, 0, 0, buf_timing, Duration::from_millis(500))
            .map_err(|e| format!("设置波特率失败: {}", e))?;

        // 3. 启动设备 (Mode Start)
        let mode = GsDeviceMode {
            mode: GS_CAN_MODE_START,
            flags: 0,
        };
        let buf_mode = unsafe {
            std::slice::from_raw_parts(&mode as *const _ as *const u8, std::mem::size_of::<GsDeviceMode>())
        };
        handle.write_control(0x41, GS_USB_BREQ_MODE, 0, 0, buf_mode, Duration::from_millis(500))
            .map_err(|e| format!("启动 gs_usb 失败: {}", e))?;

        self.channel_name = channel.to_string();
        self.opened = true;
        self.echo_counter = 0;
        Ok(())
    }

    fn close(&mut self) {
        if self.opened {
            if let Some(ref mut handle) = self.handle {
                let mode = GsDeviceMode {
                    mode: GS_CAN_MODE_RESET,
                    flags: 0,
                };
                let buf_mode = unsafe {
                    std::slice::from_raw_parts(&mode as *const _ as *const u8, std::mem::size_of::<GsDeviceMode>())
                };
                let _ = handle.write_control(0x41, GS_USB_BREQ_MODE, 0, 0, buf_mode, Duration::from_millis(300));
            }
            self.opened = false;
        }
    }

    fn is_open(&self) -> bool {
        self.opened
    }

    fn send(&mut self, frame: &CanFrame) -> Result<(), String> {
        let handle = self.handle.as_mut().ok_or("gs_usb 未打开")?;

        let mut buf = [0u8; 32];
        // 关键修复：标准扩展帧标志最高位置位 (CAN_EFF_FLAG = 0x80000000)
        let can_id = if frame.is_extended {
            (frame.id & 0x1FFFFFFF) | 0x80000000
        } else {
            frame.id & 0x000007FF
        };

        let echo_id = self.echo_counter;
        self.echo_counter = self.echo_counter.wrapping_add(1);

        buf[0..4].copy_from_slice(&echo_id.to_le_bytes());
        buf[4..8].copy_from_slice(&can_id.to_le_bytes());
        buf[8] = frame.data.len().min(8) as u8;
        buf[9] = 0;  // channel
        buf[10] = 0; // flags
        buf[11] = 0;

        let len = frame.data.len().min(8);
        buf[12..12 + len].copy_from_slice(&frame.data[..len]);

        handle.write_bulk(self.ep_out, &buf, Duration::from_millis(100))
            .map_err(|e| format!("gs_usb 发送失败: {}", e))?;

        Ok(())
    }

    fn receive(&mut self, timeout: Duration) -> Result<Option<CanFrame>, String> {
        let handle = self.handle.as_mut().ok_or("gs_usb 未打开")?;

        let mut buf = [0u8; 64];
        let start = std::time::Instant::now();

        // 持续读取直到读取到真实 RX 报文或超时，过滤掉硬件自身的回显帧 (Echo Frame)
        while start.elapsed() < timeout {
            let remain = timeout.saturating_sub(start.elapsed()).max(Duration::from_millis(2));

            match handle.read_bulk(self.ep_in, &mut buf, remain) {
                Ok(read_bytes) if read_bytes >= 20 => {
                    let echo_id = u32::from_le_bytes(buf[0..4].try_into().unwrap());
                    
                    // 关键修复：gs_usb 协议规定，echo_id == 0xFFFFFFFF 时才是总线上接收到的 ECU 报文
                    // 如果 echo_id != 0xFFFFFFFF，则是本机刚才发送成功的 echo 回显通知，需忽略并继续读取
                    if echo_id != GS_USB_NONE_ECHO_ID {
                        continue;
                    }

                    let raw_id = u32::from_le_bytes(buf[4..8].try_into().unwrap());
                    let is_extended = (raw_id & 0x80000000) != 0;
                    let id = if is_extended { raw_id & 0x1FFFFFFF } else { raw_id & 0x7FF };
                    let dlc = (buf[8] as usize).min(8);

                    let data = buf[12..12 + dlc].to_vec();

                    return Ok(Some(CanFrame {
                        id,
                        is_extended,
                        data,
                        is_rx: true,
                        timestamp_us: 0,
                    }));
                }
                Ok(_) => {}
                Err(rusb::Error::Timeout) => return Ok(None),
                Err(e) => return Err(format!("gs_usb 接收错误: {}", e)),
            }
        }
        Ok(None)
    }

    fn get_active_channel_name(&self) -> String {
        self.channel_name.clone()
    }
}

impl Drop for GsUsbAdapter {
    fn drop(&mut self) {
        self.close();
    }
}