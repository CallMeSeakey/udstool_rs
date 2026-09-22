// src/core/can/pcan_adapter.rs
use super::{CanAdapter, CanFrame};
use crate::i18n::{t, t_fmt};
use libloading::Library;
use std::time::Duration;

#[repr(C)]
#[derive(Copy, Clone)]
struct TPCANMsg {
    pub id: u32,
    pub msgtype: u8,
    pub len: u8,
    pub data: [u8; 8],
}

#[repr(C)]
#[derive(Copy, Clone)]
struct TPCANTimestamp {
    pub millis: u32,
    pub millis_overflow: u16,
    pub micros: u16,
}

type PcanInitFn = unsafe extern "system" fn(u16, u16, u8, u32, u16) -> u32;
type PcanUninitFn = unsafe extern "system" fn(u16) -> u32;
type PcanReadFn = unsafe extern "system" fn(u16, *mut TPCANMsg, *mut TPCANTimestamp) -> u32;
type PcanWriteFn = unsafe extern "system" fn(u16, *mut TPCANMsg) -> u32;
type PcanResetFn = unsafe extern "system" fn(u16) -> u32;
type PcanGetErrTextFn = unsafe extern "system" fn(u32, u16, *mut u8) -> u32;

pub fn get_channels() -> Vec<String> {
    (1..=8).map(|i| format!("PCAN_USBBUS{}", i)).collect()
}

pub struct PcanAdapter {
    _lib: Library,
    fn_init: PcanInitFn,
    fn_uninit: PcanUninitFn,
    fn_read: PcanReadFn,
    fn_write: PcanWriteFn,
    fn_reset: Option<PcanResetFn>,
    fn_get_err: Option<PcanGetErrTextFn>,
    channel_handle: u16,
    channel_name: String,
    opened: bool,
}

impl PcanAdapter {
    pub fn try_load() -> Result<Self, String> {
        let lib = unsafe {
            Library::new("PCANBasic.dll")
                .or_else(|_| Library::new("C:\\Windows\\System32\\PCANBasic.dll"))
                .map_err(|e| format!("PCANBasic.dll: {}", e))?
        };

        unsafe {
            let fn_init = *lib.get::<PcanInitFn>(b"CAN_Initialize").map_err(|e| e.to_string())?.into_raw();
            let fn_uninit = *lib.get::<PcanUninitFn>(b"CAN_Uninitialize").map_err(|e| e.to_string())?.into_raw();
            let fn_read = *lib.get::<PcanReadFn>(b"CAN_Read").map_err(|e| e.to_string())?.into_raw();
            let fn_write = *lib.get::<PcanWriteFn>(b"CAN_Write").map_err(|e| e.to_string())?.into_raw();
            let fn_reset = lib.get::<PcanResetFn>(b"CAN_Reset").ok().map(|s| *s.into_raw());
            let fn_get_err = lib.get::<PcanGetErrTextFn>(b"CAN_GetErrorText").ok().map(|s| *s.into_raw());

            Ok(Self {
                _lib: lib,
                channel_handle: 0x51,
                channel_name: "PCAN_USBBUS1".to_string(),
                opened: false,
                fn_init,
                fn_uninit,
                fn_read,
                fn_write,
                fn_reset,
                fn_get_err,
            })
        }
    }

    fn channel_from_name(name: &str) -> u16 {
        match name.trim() {
            "PCAN_USBBUS1" => 0x51,
            "PCAN_USBBUS2" => 0x52,
            "PCAN_USBBUS3" => 0x53,
            "PCAN_USBBUS4" => 0x54,
            "PCAN_USBBUS5" => 0x55,
            "PCAN_USBBUS6" => 0x56,
            "PCAN_USBBUS7" => 0x57,
            "PCAN_USBBUS8" => 0x58,
            _ => 0x51,
        }
    }

    fn baudrate_to_pcan(baud: u32) -> u16 {
        match baud {
            1000000 => 0x0014,
            500000 => 0x001C,
            250000 => 0x011C,
            125000 => 0x031C,
            100000 => 0x432F,
            _ => 0x011C,
        }
    }

    fn get_error_string(&self, err_code: u32) -> String {
        if let Some(f_err) = self.fn_get_err {
            let mut buf = vec![0u8; 256];
            let res = unsafe { f_err(err_code, 0, buf.as_mut_ptr()) };
            if res == 0 {
                return String::from_utf8_lossy(&buf).trim_matches('\0').trim().to_string();
            }
        }
        format!("0x{:08X}", err_code)
    }
}

impl CanAdapter for PcanAdapter {
    fn open(&mut self, channel: &str, bitrate: u32) -> Result<(), String> {
        let handle = Self::channel_from_name(channel);
        let btr = Self::baudrate_to_pcan(bitrate);

        let mut res = unsafe { (self.fn_init)(handle, btr, 0, 0, 0) };

        if (res & 0x00001000) != 0 {
            unsafe { (self.fn_uninit)(handle) };
            std::thread::sleep(Duration::from_millis(50));
            res = unsafe { (self.fn_init)(handle, btr, 0, 0, 0) };
        }

        if res == 0 || (res & 0x00001000) != 0 {
            self.channel_handle = handle;
            self.channel_name = channel.to_string();
            self.opened = true;
            if let Some(f_reset) = self.fn_reset {
                unsafe { f_reset(handle) };
            }
            return Ok(());
        }

        let pcan_err_str = self.get_error_string(res);
        let code_hex = format!("{:08X}", res);
        let err_desc = match res {
            0x04000000 => t("pcan_err_illhw"),
            0x00000020 => t("pcan_err_buslight"),
            0x00000040 => t("pcan_err_busheavy"),
            0x00000080 => t("pcan_err_busoff"),
            _ if (res & 0x00001000) != 0 => t("pcan_err_occupied"),
            _ => format!("{} ({})", pcan_err_str, t_fmt("pcan_err_unknown", &[("code", &code_hex)])),
        };
        Err(err_desc)
    }

    fn close(&mut self) {
        if self.opened {
            unsafe { (self.fn_uninit)(self.channel_handle) };
            self.opened = false;
        }
    }

    fn is_open(&self) -> bool {
        self.opened
    }

    fn send(&mut self, frame: &CanFrame) -> Result<(), String> {
        if !self.opened {
            return Err(t("pcan_err_illhw"));
        }
        let mut msg = TPCANMsg {
            id: frame.id,
            msgtype: if frame.is_extended { 0x02 } else { 0x00 },
            len: frame.data.len().min(8) as u8,
            data: [0u8; 8],
        };
        msg.data[..frame.data.len().min(8)].copy_from_slice(&frame.data[..frame.data.len().min(8)]);
        let res = unsafe { (self.fn_write)(self.channel_handle, &mut msg) };
        if res == 0 {
            Ok(())
        } else {
            let code_hex = format!("{:08X}", res);
            Err(t_fmt("pcan_err_unknown", &[("code", &code_hex)]))
        }
    }

    fn receive(&mut self, _timeout: Duration) -> Result<Option<CanFrame>, String> {
        if !self.opened {
            return Err(t("pcan_err_illhw"));
        }
        let mut msg = TPCANMsg {
            id: 0,
            msgtype: 0,
            len: 0,
            data: [0u8; 8],
        };
        let mut ts = TPCANTimestamp {
            millis: 0,
            millis_overflow: 0,
            micros: 0,
        };
        let res = unsafe { (self.fn_read)(self.channel_handle, &mut msg, &mut ts) };
        if res == 0 {
            if (msg.msgtype & 0x01) != 0 {
                return Ok(None);
            }
            let frame = CanFrame {
                id: msg.id,
                is_extended: (msg.msgtype & 0x02) != 0,
                data: msg.data[..msg.len as usize].to_vec(),
                is_rx: true,
                timestamp_us: (ts.millis as u64) * 1000 + (ts.micros as u64),
            };
            Ok(Some(frame))
        } else if res == 0x00020 {
            Ok(None)
        } else {
            let code_hex = format!("{:08X}", res);
            Err(t_fmt("pcan_err_unknown", &[("code", &code_hex)]))
        }
    }

    fn get_active_channel_name(&self) -> String {
        self.channel_name.clone()
    }
}