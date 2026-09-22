// src/core/can/zlg_adapter.rs
use super::{CanAdapter, CanFrame};
use libloading::Library;
use std::time::Duration;

// =========================================================================
// 1. ZCAN 常量与结构体定义 (对齐 zlgcan.dll 官方标准)
// =========================================================================
pub const ZCAN_DEVICE_TYPE_USBCAN2: u32 = 4; // 兼容 USBCAN-2A/2C/2E-U/II 等型号
pub const ZCAN_STATUS_OK: u32 = 1;
pub const INVALID_DEVICE_HANDLE: usize = 0;
pub const INVALID_CHANNEL_HANDLE: usize = 0;

/// 通道初始化参数配置
#[repr(C)]
#[derive(Copy, Clone)]
pub struct ZcanInitConfig {
    pub can_type: u8, // 0: CAN, 1: CANFD
    pub reserved: [u8; 3],
    pub acc_code: u32,
    pub acc_mask: u32,
    pub reserved1: u32,
    pub filter: u8,
    pub timing0: u8,
    pub timing1: u8,
    pub mode: u8, // 0: 正常模式, 1: 只听模式
}

impl Default for ZcanInitConfig {
    fn default() -> Self {
        Self {
            can_type: 0,
            reserved: [0; 3],
            acc_code: 0x00000000,
            acc_mask: 0xFFFFFFFF,
            reserved1: 0,
            filter: 1,
            timing0: 0x00,
            timing1: 0x1C,
            mode: 0,
        }
    }
}

/// ZCAN 报文对象
#[repr(C)]
#[derive(Copy, Clone)]
pub struct ZcanCanFrame {
    pub can_id: u32,
    pub eff_rtr_pad: u32, // 包含扩展帧与远程帧标识
    pub can_dlc: u8,
    pub pad: [u8; 3],
    pub data: [u8; 8],
}

/// 发送/接收报文载荷结构体
#[repr(C)]
#[derive(Copy, Clone)]
pub struct ZcanTransmitData {
    pub frame: ZcanCanFrame,
    pub timestamp: u32,
}

// ZCAN 函数指针定义
type ZcanOpenDeviceFn = unsafe extern "system" fn(u32, u32, u32) -> usize;
type ZcanCloseDeviceFn = unsafe extern "system" fn(usize) -> u32;
type ZcanInitCanFn = unsafe extern "system" fn(usize, u32, *const ZcanInitConfig) -> usize;
type ZcanStartCanFn = unsafe extern "system" fn(usize) -> u32;
type ZcanResetCanFn = unsafe extern "system" fn(usize) -> u32;
type ZcanTransmitDataFn = unsafe extern "system" fn(usize, *const ZcanTransmitData, u32) -> u32;
type ZcanReceiveDataFn = unsafe extern "system" fn(usize, *mut ZcanTransmitData, u32, i32) -> u32;

// =========================================================================
// 2. 通道列表枚举
// =========================================================================
pub fn get_channels() -> Vec<String> {
    vec![
        "Channel 0 (CAN1)".to_string(),
        "Channel 1 (CAN2)".to_string(),
    ]
}

// =========================================================================
// 3. 适配器结构体与实现
// =========================================================================
pub struct ZlgAdapter {
    _lib: Library,
    dev_handle: usize,
    chn_handle: usize,
    channel_name: String,
    opened: bool,

    fn_open_device: ZcanOpenDeviceFn,
    fn_close_device: ZcanCloseDeviceFn,
    fn_init_can: ZcanInitCanFn,
    fn_start_can: ZcanStartCanFn,
    fn_reset_can: Option<ZcanResetCanFn>,
    fn_transmit_data: ZcanTransmitDataFn,
    fn_receive_data: ZcanReceiveDataFn,
}

impl ZlgAdapter {
    pub fn try_load() -> Result<Self, String> {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()));

        let mut candidate_paths = Vec::new();

        // 优先使用绝对路径加载
        if let Some(ref dir) = exe_dir {
            candidate_paths.push(dir.join("zlgcan.dll").to_string_lossy().to_string());
            candidate_paths.push(dir.join("ControlCAN.dll").to_string_lossy().to_string());
        }
        candidate_paths.push("zlgcan.dll".to_string());
        candidate_paths.push("ControlCAN.dll".to_string());
        candidate_paths.push("C:\\Windows\\System32\\zlgcan.dll".to_string());
        candidate_paths.push("C:\\Windows\\System32\\ControlCAN.dll".to_string());

        let mut loaded_lib = None;
        let mut err_msgs = Vec::new();

        for path in &candidate_paths {
            if std::path::Path::new(path).exists() || !path.contains('\\') {
                // 使用 Windows 标志 LOAD_WITH_ALTERED_SEARCH_PATH (0x00000008)
                // 确保 zlgcan.dll 会在它自身的目录下自动检索 zdbc.dll、NetClient.dll 等依赖
                #[cfg(windows)]
                let res = unsafe {
                    use libloading::os::windows::Library as WinLib;
                    WinLib::load_with_flags(path, 0x00000008).map(Library::from)
                };
                #[cfg(not(windows))]
                let res = unsafe { Library::new(path) };

                match res {
                    Ok(lib) => {
                        loaded_lib = Some(lib);
                        break;
                    }
                    Err(e) => {
                        err_msgs.push(format!("{}: {}", path, e));
                    }
                }
            }
        }

        let lib = loaded_lib.ok_or_else(|| {
            format!(
                "未找到周立功驱动库或缺少依赖 DLL: {}",
                err_msgs.join(" | ")
            )
        })?;

        unsafe {
            let fn_open_device = *lib
                .get::<ZcanOpenDeviceFn>(b"ZCAN_OpenDevice")
                .or_else(|_| lib.get::<ZcanOpenDeviceFn>(b"VCI_OpenDevice"))
                .map_err(|e| format!("未找到 OpenDevice 符号: {}", e))?
                .into_raw();

            let fn_close_device = *lib
                .get::<ZcanCloseDeviceFn>(b"ZCAN_CloseDevice")
                .or_else(|_| lib.get::<ZcanCloseDeviceFn>(b"VCI_CloseDevice"))
                .map_err(|e| format!("未找到 CloseDevice 符号: {}", e))?
                .into_raw();

            let fn_init_can = *lib
                .get::<ZcanInitCanFn>(b"ZCAN_InitCAN")
                .or_else(|_| lib.get::<ZcanInitCanFn>(b"VCI_InitCAN"))
                .map_err(|e| format!("未找到 InitCAN 符号: {}", e))?
                .into_raw();

            let fn_start_can = *lib
                .get::<ZcanStartCanFn>(b"ZCAN_StartCAN")
                .or_else(|_| lib.get::<ZcanStartCanFn>(b"VCI_StartCAN"))
                .map_err(|e| format!("未找到 StartCAN 符号: {}", e))?
                .into_raw();

            let fn_reset_can = lib
                .get::<ZcanResetCanFn>(b"ZCAN_ResetCAN")
                .or_else(|_| lib.get::<ZcanResetCanFn>(b"VCI_ResetCAN"))
                .ok()
                .map(|f| *f.into_raw());

            let fn_transmit_data = *lib
                .get::<ZcanTransmitDataFn>(b"ZCAN_TransmitData")
                .or_else(|_| lib.get::<ZcanTransmitDataFn>(b"VCI_Transmit"))
                .map_err(|e| format!("未找到 Transmit 符号: {}", e))?
                .into_raw();

            let fn_receive_data = *lib
                .get::<ZcanReceiveDataFn>(b"ZCAN_ReceiveData")
                .or_else(|_| lib.get::<ZcanReceiveDataFn>(b"VCI_Receive"))
                .map_err(|e| format!("未找到 Receive 符号: {}", e))?
                .into_raw();

            Ok(Self {
                _lib: lib,
                dev_handle: INVALID_DEVICE_HANDLE,
                chn_handle: INVALID_CHANNEL_HANDLE,
                channel_name: "Channel 0 (CAN1)".to_string(),
                opened: false,
                fn_open_device,
                fn_close_device,
                fn_init_can,
                fn_start_can,
                fn_reset_can,
                fn_transmit_data,
                fn_receive_data,
            })
        }
    }

    fn parse_channel_index(ch_str: &str) -> u32 {
        if ch_str.contains('1') || ch_str.contains("CAN2") {
            1
        } else {
            0
        }
    }

    /// 波特率对应 SJA1000 典型定时器参数
    fn baudrate_to_timing(baudrate: u32) -> (u8, u8) {
        match baudrate {
            1000000 => (0x00, 0x14),
            500000 => (0x00, 0x1C),
            250000 => (0x01, 0x1C),
            125000 => (0x03, 0x1C),
            100000 => (0x04, 0x1C),
            _ => (0x01, 0x1C),
        }
    }
}

impl CanAdapter for ZlgAdapter {
    fn open(&mut self, channel: &str, bitrate: u32) -> Result<(), String> {
        self.close();

        let chn_idx = Self::parse_channel_index(channel);

        // 1. 打开设备
        let d_handle = unsafe { (self.fn_open_device)(ZCAN_DEVICE_TYPE_USBCAN2, 0, 0) };
        if d_handle == INVALID_DEVICE_HANDLE {
            return Err("周立功设备打开失败: 请确认设备已插入且驱动安装正确".to_string());
        }

        // 2. 初始化 CAN 通道
        let (t0, t1) = Self::baudrate_to_timing(bitrate);
        let mut cfg = ZcanInitConfig::default();
        cfg.timing0 = t0;
        cfg.timing1 = t1;

        let c_handle = unsafe { (self.fn_init_can)(d_handle, chn_idx, &cfg) };
        if c_handle == INVALID_CHANNEL_HANDLE {
            unsafe { (self.fn_close_device)(d_handle) };
            return Err(format!("ZLG 通道 {} 初始化失败", chn_idx));
        }

        // 3. 启动通道
        let start_res = unsafe { (self.fn_start_can)(c_handle) };
        if start_res != ZCAN_STATUS_OK {
            unsafe {
                (self.fn_close_device)(d_handle);
            }
            return Err(format!("ZLG 通道 {} 启动 (StartCAN) 失败", chn_idx));
        }

        self.dev_handle = d_handle;
        self.chn_handle = c_handle;
        self.channel_name = channel.to_string();
        self.opened = true;
        Ok(())
    }

    fn close(&mut self) {
        if self.opened {
            unsafe {
                if self.chn_handle != INVALID_CHANNEL_HANDLE {
                    if let Some(reset_fn) = self.fn_reset_can {
                        let _ = reset_fn(self.chn_handle);
                    }
                }
                if self.dev_handle != INVALID_DEVICE_HANDLE {
                    let _ = (self.fn_close_device)(self.dev_handle);
                }
            }
            self.opened = false;
            self.chn_handle = INVALID_CHANNEL_HANDLE;
            self.dev_handle = INVALID_DEVICE_HANDLE;
        }
    }

    fn is_open(&self) -> bool {
        self.opened
    }

    fn send(&mut self, frame: &CanFrame) -> Result<(), String> {
        if !self.opened || self.chn_handle == INVALID_CHANNEL_HANDLE {
            return Err("ZLG 硬件设备未连接".to_string());
        }

        let mut data_frame = ZcanTransmitData {
            frame: ZcanCanFrame {
                can_id: frame.id,
                eff_rtr_pad: if frame.is_extended { 1 } else { 0 },
                can_dlc: frame.data.len().min(8) as u8,
                pad: [0; 3],
                data: [0u8; 8],
            },
            timestamp: 0,
        };

        let copy_len = frame.data.len().min(8);
        data_frame.frame.data[..copy_len].copy_from_slice(&frame.data[..copy_len]);

        let sent = unsafe { (self.fn_transmit_data)(self.chn_handle, &data_frame, 1) };
        if sent == 1 {
            Ok(())
        } else {
            Err(format!("ZLG 发送帧失败: 实际发送数量 {}", sent))
        }
    }

    fn receive(&mut self, timeout: Duration) -> Result<Option<CanFrame>, String> {
        if !self.opened || self.chn_handle == INVALID_CHANNEL_HANDLE {
            return Err("ZLG 硬件设备未连接".to_string());
        }

        let mut rx_data: ZcanTransmitData = unsafe { std::mem::zeroed() };
        let wait_ms = timeout.as_millis().min(20) as i32;

        let count = unsafe { (self.fn_receive_data)(self.chn_handle, &mut rx_data, 1, wait_ms) };

        if count > 0 {
            let len = (rx_data.frame.can_dlc as usize).min(8);
            let is_ext = (rx_data.frame.eff_rtr_pad & 0x01) != 0;

            Ok(Some(CanFrame {
                id: rx_data.frame.can_id,
                is_extended: is_ext,
                data: rx_data.frame.data[..len].to_vec(),
                is_rx: true,
                timestamp_us: rx_data.timestamp as u64,
            }))
        } else {
            Ok(None)
        }
    }

    fn get_active_channel_name(&self) -> String {
        self.channel_name.clone()
    }
}

impl Drop for ZlgAdapter {
    fn drop(&mut self) {
        self.close();
    }
}