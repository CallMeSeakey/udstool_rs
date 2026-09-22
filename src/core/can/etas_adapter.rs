// src/core/can/etas_adapter.rs
use super::{CanAdapter, CanFrame};
use libloading::Library;
use std::time::Duration;

// =========================================================================
// 1. SAE J2534-1 标准 API 常量与结构体定义 (ETAS ES582 原生支持)
// =========================================================================
const STATUS_NOERROR: i32 = 0;
const CAN: u32 = 0x05;
const ISO15765: u32 = 0x06;

// 通道标志
const CAN_29BIT_ID: u32 = 0x00000100;
const CAN_ID_BOTH: u32 = 0x00000800;

// Filter 类型
const PASS_FILTER: u32 = 0x01;

// Ioctl 标志
const CLEAR_RX_BUFFER: u32 = 0x01;
const CLEAR_TX_BUFFER: u32 = 0x02;

#[repr(C)]
#[derive(Copy, Clone)]
pub struct PassThruMsg {
    pub protocol_id: u32,
    pub rx_status: u32,
    pub tx_flags: u32,
    pub timestamp: u32,
    pub data_size: u32,
    pub extra_data_index: u32,
    pub data: [u8; 4128],
}

impl Default for PassThruMsg {
    fn default() -> Self {
        Self {
            protocol_id: CAN,
            rx_status: 0,
            tx_flags: 0,
            timestamp: 0,
            data_size: 0,
            extra_data_index: 0,
            data: [0u8; 4128],
        }
    }
}

// J2534 函数指针
type PtOpenFn = unsafe extern "system" fn(*const std::ffi::c_void, *mut u32) -> i32;
type PtCloseFn = unsafe extern "system" fn(u32) -> i32;
type PtConnectFn = unsafe extern "system" fn(u32, u32, u32, u32, *mut u32) -> i32;
type PtDisconnectFn = unsafe extern "system" fn(u32) -> i32;
type PtReadMsgsFn = unsafe extern "system" fn(u32, *mut PassThruMsg, *mut u32, u32) -> i32;
type PtWriteMsgsFn = unsafe extern "system" fn(u32, *mut PassThruMsg, *mut u32, u32) -> i32;
type PtStartMsgFilterFn = unsafe extern "system" fn(u32, u32, *const PassThruMsg, *const PassThruMsg, *const PassThruMsg, *mut u32) -> i32;
type PtIoctlFn = unsafe extern "system" fn(u32, u32, *mut std::ffi::c_void, *mut std::ffi::c_void) -> i32;
type PtGetLastErrorFn = unsafe extern "system" fn(*mut u8) -> i32;

pub fn get_channels() -> Vec<String> {
    vec![
        "Channel 1 (CAN1)".to_string(),
        "Channel 2 (CAN2)".to_string(),
    ]
}

pub struct EtasAdapter {
    _lib: Library,
    device_id: u32,
    channel_id: u32,
    filter_id: u32,
    channel_name: String,
    opened: bool,

    fn_open: PtOpenFn,
    fn_close: PtCloseFn,
    fn_connect: PtConnectFn,
    fn_disconnect: PtDisconnectFn,
    fn_read: PtReadMsgsFn,
    fn_write: PtWriteMsgsFn,
    fn_start_filter: PtStartMsgFilterFn,
    fn_ioctl: PtIoctlFn,
    fn_get_err: Option<PtGetLastErrorFn>,
}

impl EtasAdapter {
    pub fn try_load() -> Result<Self, String> {
        // ETAS ES582 官方驱动常见 DLL 名称与路径候选
        let candidates = [
            "ES582_J2534.dll",
            "etas_j2534.dll",
            "ETAS_PT32.dll",
            "ETAS_PT64.dll",
            "C:\\Program Files\\ETAS\\J2534\\etas_j2534.dll",
            "C:\\Program Files (x86)\\ETAS\\J2534\\etas_j2534.dll",
            "C:\\Windows\\System32\\etas_j2534.dll",
        ];

        let mut loaded_lib = None;
        let mut err_msgs = Vec::new();

        for path in &candidates {
            match unsafe { Library::new(path) } {
                Ok(lib) => {
                    loaded_lib = Some(lib);
                    break;
                }
                Err(e) => {
                    err_msgs.push(format!("{}: {}", path, e));
                }
            }
        }

        let lib = loaded_lib.ok_or_else(|| {
            format!(
                "未找到 ETAS ES582 驱动库 (请确认已安装 ETAS ES582 驱动/HSP): {}",
                err_msgs.join(" | ")
            )
        })?;

        unsafe {
            let fn_open = *lib.get::<PtOpenFn>(b"PassThruOpen").map_err(|e| e.to_string())?.into_raw();
            let fn_close = *lib.get::<PtCloseFn>(b"PassThruClose").map_err(|e| e.to_string())?.into_raw();
            let fn_connect = *lib.get::<PtConnectFn>(b"PassThruConnect").map_err(|e| e.to_string())?.into_raw();
            let fn_disconnect = *lib.get::<PtDisconnectFn>(b"PassThruDisconnect").map_err(|e| e.to_string())?.into_raw();
            let fn_read = *lib.get::<PtReadMsgsFn>(b"PassThruReadMsgs").map_err(|e| e.to_string())?.into_raw();
            let fn_write = *lib.get::<PtWriteMsgsFn>(b"PassThruWriteMsgs").map_err(|e| e.to_string())?.into_raw();
            let fn_start_filter = *lib.get::<PtStartMsgFilterFn>(b"PassThruStartMsgFilter").map_err(|e| e.to_string())?.into_raw();
            let fn_ioctl = *lib.get::<PtIoctlFn>(b"PassThruIoctl").map_err(|e| e.to_string())?.into_raw();
            let fn_get_err = lib.get::<PtGetLastErrorFn>(b"PassThruGetLastError").ok().map(|f| *f.into_raw());

            Ok(Self {
                _lib: lib,
                device_id: 0,
                channel_id: 0,
                filter_id: 0,
                channel_name: "Channel 1 (CAN1)".to_string(),
                opened: false,
                fn_open,
                fn_close,
                fn_connect,
                fn_disconnect,
                fn_read,
                fn_write,
                fn_start_filter,
                fn_ioctl,
                fn_get_err,
            })
        }
    }

    fn get_last_error_string(&self) -> String {
        if let Some(get_err) = self.fn_get_err {
            let mut buf = [0u8; 256];
            let ret = unsafe { get_err(buf.as_mut_ptr()) };
            if ret == STATUS_NOERROR {
                return String::from_utf8_lossy(&buf).trim_matches('\0').trim().to_string();
            }
        }
        "Unknown J2534 error".to_string()
    }
}

impl CanAdapter for EtasAdapter {
    fn open(&mut self, channel: &str, bitrate: u32) -> Result<(), String> {
        self.close();

        // 1. 打开设备
        let mut dev_id = 0u32;
        let ret_open = unsafe { (self.fn_open)(std::ptr::null(), &mut dev_id) };
        if ret_open != STATUS_NOERROR {
            return Err(format!("ETAS ES582 打开失败: {}", self.get_last_error_string()));
        }

        // 2. 连接通道与设定波特率 (协议: CAN, 标志: 支持29位ID)
        let mut ch_id = 0u32;
        let connect_flags = CAN_ID_BOTH;
        let ret_conn = unsafe { (self.fn_connect)(dev_id, CAN, connect_flags, bitrate, &mut ch_id) };
        if ret_conn != STATUS_NOERROR {
            unsafe { (self.fn_close)(dev_id) };
            return Err(format!("ETAS ES582 通道连接/波特率设定失败 ({} bps): {}", bitrate, self.get_last_error_string()));
        }

        // 3. 设置放行所有报文的全通滤波器 (PASS_FILTER)
        let mut mask_msg = PassThruMsg::default();
        mask_msg.data_size = 4;

        let mut pattern_msg = PassThruMsg::default();
        pattern_msg.data_size = 4;

        let mut flt_id = 0u32;
        let ret_flt = unsafe {
            (self.fn_start_filter)(
                ch_id,
                PASS_FILTER,
                &mask_msg,
                &pattern_msg,
                std::ptr::null(),
                &mut flt_id,
            )
        };

        if ret_flt != STATUS_NOERROR {
            unsafe {
                (self.fn_disconnect)(ch_id);
                (self.fn_close)(dev_id);
            }
            return Err(format!("ETAS ES582 设置过滤器失败: {}", self.get_last_error_string()));
        }

        // 清空缓存队列
        unsafe {
            (self.fn_ioctl)(ch_id, CLEAR_RX_BUFFER, std::ptr::null_mut(), std::ptr::null_mut());
            (self.fn_ioctl)(ch_id, CLEAR_TX_BUFFER, std::ptr::null_mut(), std::ptr::null_mut());
        }

        self.device_id = dev_id;
        self.channel_id = ch_id;
        self.filter_id = flt_id;
        self.channel_name = channel.to_string();
        self.opened = true;
        Ok(())
    }

    fn close(&mut self) {
        if self.opened {
            unsafe {
                if self.channel_id != 0 {
                    let _ = (self.fn_disconnect)(self.channel_id);
                }
                if self.device_id != 0 {
                    let _ = (self.fn_close)(self.device_id);
                }
            }
            self.opened = false;
            self.channel_id = 0;
            self.device_id = 0;
            self.filter_id = 0;
        }
    }

    fn is_open(&self) -> bool {
        self.opened
    }

    fn send(&mut self, frame: &CanFrame) -> Result<(), String> {
        if !self.opened || self.channel_id == 0 {
            return Err("ETAS ES582 设备未开启".to_string());
        }

        let mut pt_msg = PassThruMsg::default();
        let len = frame.data.len().min(8);

        // J2534 CAN 格式: 前 4 字节为 CAN ID (大端序)，后续为 Payload 数据
        pt_msg.data_size = (4 + len) as u32;

        if frame.is_extended {
            pt_msg.tx_flags = CAN_29BIT_ID;
            pt_msg.data[0] = ((frame.id >> 24) & 0xFF) as u8;
            pt_msg.data[1] = ((frame.id >> 16) & 0xFF) as u8;
            pt_msg.data[2] = ((frame.id >> 8) & 0xFF) as u8;
            pt_msg.data[3] = (frame.id & 0xFF) as u8;
        } else {
            pt_msg.tx_flags = 0;
            pt_msg.data[0] = 0;
            pt_msg.data[1] = 0;
            pt_msg.data[2] = ((frame.id >> 8) & 0xFF) as u8;
            pt_msg.data[3] = (frame.id & 0xFF) as u8;
        }

        pt_msg.data[4..4 + len].copy_from_slice(&frame.data[..len]);

        let mut num_msgs = 1u32;
        let timeout_ms = 100;
        let ret = unsafe { (self.fn_write)(self.channel_id, &mut pt_msg, &mut num_msgs, timeout_ms) };

        if ret == STATUS_NOERROR && num_msgs > 0 {
            Ok(())
        } else {
            Err(format!("ETAS ES582 发送失败: {}", self.get_last_error_string()))
        }
    }

    fn receive(&mut self, timeout: Duration) -> Result<Option<CanFrame>, String> {
        if !self.opened || self.channel_id == 0 {
            return Err("ETAS ES582 设备未开启".to_string());
        }

        let mut pt_msg = PassThruMsg::default();
        let mut num_msgs = 1u32;
        let wait_ms = timeout.as_millis().min(20) as u32;

        let ret = unsafe { (self.fn_read)(self.channel_id, &mut pt_msg, &mut num_msgs, wait_ms) };

        if ret == STATUS_NOERROR && num_msgs > 0 && pt_msg.data_size >= 4 {
            let is_extended = (pt_msg.rx_status & CAN_29BIT_ID) != 0;
            let can_id = if is_extended {
                ((pt_msg.data[0] as u32) << 24)
                    | ((pt_msg.data[1] as u32) << 16)
                    | ((pt_msg.data[2] as u32) << 8)
                    | (pt_msg.data[3] as u32)
            } else {
                ((pt_msg.data[2] as u32) << 8) | (pt_msg.data[3] as u32)
            };

            let payload_len = (pt_msg.data_size - 4).min(8) as usize;
            let payload = pt_msg.data[4..4 + payload_len].to_vec();

            Ok(Some(CanFrame {
                id: can_id,
                is_extended,
                data: payload,
                is_rx: true,
                timestamp_us: (pt_msg.timestamp as u64) * 1000,
            }))
        } else {
            Ok(None)
        }
    }

    fn get_active_channel_name(&self) -> String {
        self.channel_name.clone()
    }
}

impl Drop for EtasAdapter {
    fn drop(&mut self) {
        self.close();
    }
}