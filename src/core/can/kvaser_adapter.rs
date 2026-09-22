// src/core/can/kvaser_adapter.rs
use super::{CanAdapter, CanFrame};
use libloading::Library;
use std::time::Duration;

pub type CanHandle = i32;
pub type CanStatus = i32;

pub const CAN_OK: CanStatus = 0;
pub const CAN_ERR_NOMSG: CanStatus = -2;
pub const CAN_OPEN_ACCEPT_VIRTUAL: i32 = 0x0020;
pub const CAN_MSG_EXT: u32 = 0x0004;

type CanInitializeLibraryFn = unsafe extern "system" fn();
type CanOpenChannelFn = unsafe extern "system" fn(i32, i32) -> CanHandle;
type CanCloseFn = unsafe extern "system" fn(CanHandle) -> CanStatus;
type CanSetBusParamsFn = unsafe extern "system" fn(CanHandle, i64, u32, u32, u32, u32, u32) -> CanStatus;
type CanBusOnFn = unsafe extern "system" fn(CanHandle) -> CanStatus;
type CanBusOffFn = unsafe extern "system" fn(CanHandle) -> CanStatus;
type CanWriteFn = unsafe extern "system" fn(CanHandle, i64, *const u8, u32, u32) -> CanStatus;
type CanReadFn = unsafe extern "system" fn(CanHandle, *mut i64, *mut u8, *mut u32, *mut u32, *mut u64) -> CanStatus;
type CanGetNumberOfChannelsFn = unsafe extern "system" fn(*mut i32) -> CanStatus;

pub fn get_channels() -> Vec<String> {
    if let Ok(lib) = unsafe {
        Library::new("canlib32.dll")
            .or_else(|_| Library::new("C:\\Windows\\System32\\canlib32.dll"))
    } {
        unsafe {
            if let Ok(get_count_fn) = lib.get::<CanGetNumberOfChannelsFn>(b"canGetNumberOfChannels") {
                let mut count: i32 = 0;
                if get_count_fn(&mut count) == CAN_OK && count > 0 {
                    return (0..count).map(|i| format!("Channel {}", i)).collect();
                }
            }
        }
    }
    vec![
        "Channel 0".to_string(),
        "Channel 1".to_string(),
        "Channel 2".to_string(),
        "Channel 3".to_string(),
    ]
}

pub struct KvaserAdapter {
    _lib: Library,
    handle: CanHandle,
    channel_name: String,
    opened: bool,
    fn_open_channel: CanOpenChannelFn,
    fn_close: CanCloseFn,
    fn_set_bus_params: CanSetBusParamsFn,
    fn_bus_on: CanBusOnFn,
    fn_bus_off: CanBusOffFn,
    fn_write: CanWriteFn,
    fn_read: CanReadFn,
}

impl KvaserAdapter {
    pub fn try_load() -> Result<Self, String> {
        let lib = unsafe {
            Library::new("canlib32.dll")
                .or_else(|_| Library::new("C:\\Windows\\System32\\canlib32.dll"))
                .map_err(|e| format!("未找到 Kvaser canlib32.dll: {}", e))?
        };

        unsafe {
            if let Ok(init_fn) = lib.get::<CanInitializeLibraryFn>(b"canInitializeLibrary") {
                (*init_fn)();
            }

            let fn_open_channel = *lib.get::<CanOpenChannelFn>(b"canOpenChannel").map_err(|e| e.to_string())?.into_raw();
            let fn_close = *lib.get::<CanCloseFn>(b"canClose").map_err(|e| e.to_string())?.into_raw();
            let fn_set_bus_params = *lib.get::<CanSetBusParamsFn>(b"canSetBusParams").map_err(|e| e.to_string())?.into_raw();
            let fn_bus_on = *lib.get::<CanBusOnFn>(b"canBusOn").map_err(|e| e.to_string())?.into_raw();
            let fn_bus_off = *lib.get::<CanBusOffFn>(b"canBusOff").map_err(|e| e.to_string())?.into_raw();
            let fn_write = *lib.get::<CanWriteFn>(b"canWrite").map_err(|e| e.to_string())?.into_raw();
            let fn_read = *lib.get::<CanReadFn>(b"canRead").map_err(|e| e.to_string())?.into_raw();

            Ok(Self {
                _lib: lib,
                handle: -1,
                channel_name: "Channel 0".to_string(),
                opened: false,
                fn_open_channel,
                fn_close,
                fn_set_bus_params,
                fn_bus_on,
                fn_bus_off,
                fn_write,
                fn_read,
            })
        }
    }

    fn channel_from_str(channel: &str) -> i32 {
        let num_str: String = channel.chars().filter(|c| c.is_ascii_digit()).collect();
        num_str.parse::<i32>().unwrap_or(0)
    }

    fn baudrate_to_kvaser_params(bitrate: u32) -> (i64, u32, u32, u32, u32) {
        match bitrate {
            1000000 => (-1, 0, 0, 0, 0),
            500000 => (-2, 0, 0, 0, 0),
            250000 => (-3, 0, 0, 0, 0),
            125000 => (-4, 0, 0, 0, 0),
            100000 => (-5, 0, 0, 0, 0),
            _ => (bitrate as i64, 4, 3, 1, 1),
        }
    }
}

impl CanAdapter for KvaserAdapter {
    fn open(&mut self, channel: &str, bitrate: u32) -> Result<(), String> {
        self.close();
        let ch_idx = Self::channel_from_str(channel);
        let hnd = unsafe { (self.fn_open_channel)(ch_idx, CAN_OPEN_ACCEPT_VIRTUAL) };
        if hnd < 0 {
            return Err(format!("Kvaser 打开通道 {} 失败 (句柄: {})", channel, hnd));
        }

        let (freq, tseg1, tseg2, sjw, nosamp) = Self::baudrate_to_kvaser_params(bitrate);
        let st = unsafe { (self.fn_set_bus_params)(hnd, freq, tseg1, tseg2, sjw, nosamp, 0) };
        if st != CAN_OK {
            unsafe { (self.fn_close)(hnd) };
            return Err(format!("Kvaser 设置波特率 {} 失败 (状态码: {})", bitrate, st));
        }

        let st_on = unsafe { (self.fn_bus_on)(hnd) };
        if st_on != CAN_OK {
            unsafe { (self.fn_close)(hnd) };
            return Err(format!("Kvaser 启动总线 (BusOn) 失败 (状态码: {})", st_on));
        }

        self.handle = hnd;
        self.channel_name = channel.to_string();
        self.opened = true;
        Ok(())
    }

    fn close(&mut self) {
        if self.opened && self.handle >= 0 {
            unsafe {
                let _ = (self.fn_bus_off)(self.handle);
                let _ = (self.fn_close)(self.handle);
            }
            self.handle = -1;
            self.opened = false;
        }
    }

    fn is_open(&self) -> bool {
        self.opened
    }

    fn send(&mut self, frame: &CanFrame) -> Result<(), String> {
        if !self.opened || self.handle < 0 {
            return Err("Kvaser 设备未连接".to_string());
        }
        let flags = if frame.is_extended { CAN_MSG_EXT } else { 0 };
        let dlc = frame.data.len().min(8) as u32;
        let st = unsafe {
            (self.fn_write)(self.handle, frame.id as i64, frame.data.as_ptr(), dlc, flags)
        };
        if st == CAN_OK {
            Ok(())
        } else {
            Err(format!("Kvaser 发送失败 (状态码: {})", st))
        }
    }

    fn receive(&mut self, _timeout: Duration) -> Result<Option<CanFrame>, String> {
        if !self.opened || self.handle < 0 {
            return Err("Kvaser 设备未连接".to_string());
        }

        let mut id: i64 = 0;
        let mut data = [0u8; 8];
        let mut dlc: u32 = 0;
        let mut flags: u32 = 0;
        let mut time: u64 = 0;

        let st = unsafe {
            (self.fn_read)(
                self.handle,
                &mut id,
                data.as_mut_ptr(),
                &mut dlc,
                &mut flags,
                &mut time,
            )
        };

        if st == CAN_OK {
            let is_extended = (flags & CAN_MSG_EXT) != 0;
            let len = (dlc as usize).min(8);
            Ok(Some(CanFrame {
                id: id as u32,
                is_extended,
                data: data[..len].to_vec(),
                is_rx: true,
                timestamp_us: time * 1000,
            }))
        } else if st == CAN_ERR_NOMSG {
            Ok(None)
        } else {
            Err(format!("Kvaser 接收错误 (状态码: {})", st))
        }
    }

    fn get_active_channel_name(&self) -> String {
        self.channel_name.clone()
    }
}

impl Drop for KvaserAdapter {
    fn drop(&mut self) {
        self.close();
    }
}