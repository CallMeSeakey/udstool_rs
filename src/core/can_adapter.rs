// src/core/can_adapter.rs
use std::collections::VecDeque;
use std::ffi::{CStr, CString};
use std::time::Duration;
use libloading::Library;
use crate::i18n::{t, t_fmt};

#[derive(Clone, Debug)]
pub struct CanFrame {
    pub id: u32,
    pub is_extended: bool,
    pub data: Vec<u8>,
    pub is_rx: bool,
    pub timestamp_us: u64,
}

pub trait CanAdapter: Send + Sync {
    fn open(&mut self, channel: &str, bitrate: u32) -> Result<(), String>;
    fn close(&mut self);
    fn is_open(&self) -> bool;
    fn send(&mut self, frame: &CanFrame) -> Result<(), String>;
    fn receive(&mut self, timeout: Duration) -> Result<Option<CanFrame>, String>;
    fn get_active_channel_name(&self) -> String;
}

// =========================================================================
// 1. PCAN 驱动适配实现
// =========================================================================
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

        let f_init = self.fn_init;
        let f_uninit = self.fn_uninit;

        let mut res = unsafe { f_init(handle, btr, 0, 0, 0) };

        if (res & 0x00001000) != 0 {
            unsafe { f_uninit(handle) };
            std::thread::sleep(Duration::from_millis(50));
            res = unsafe { f_init(handle, btr, 0, 0, 0) };
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

// =========================================================================
// 2. Vector XL-API (VN1610 / CANoe) 驱动适配实现
// =========================================================================
pub type XlPortHandle = i32;
pub type XlAccessMask = u64;
pub type XlStatus = i32;

const XL_SUCCESS: XlStatus = 0;
const XL_ACTIVATE_RESET_TX_FIFO: u32 = 0x0040;
const XL_BUS_TYPE_CAN: u32 = 0x00000001;
const XL_TRANSMIT_MSG: u8 = 10;
const XL_RECEIVE_MSG: u8 = 1;
const XL_CAN_MSG_FLAG_TX: u16 = 0x0002;
const XL_CAN_EXT_MSG_ID: u32 = 0x80000000;

#[repr(C)]
#[derive(Copy, Clone)]
pub struct XlCanMsg {
    pub id: u32,
    pub flags: u16,
    pub dlc: u16,
    pub res1: u64,
    pub data: [u8; 8],
    pub res2: u64,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub union XlTagData {
    pub msg: XlCanMsg,
    pub raw: [u8; 64],
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct XlEvent {
    pub tag: u8,
    pub chan_index: u8,
    pub trans_id: u8,
    pub port_handle: u8,
    pub flags: u8,
    pub reserved: u8,
    pub time_stamp: u64,
    pub tag_data: XlTagData,
}

type XlOpenDriverFn = unsafe extern "system" fn() -> XlStatus;
type XlCloseDriverFn = unsafe extern "system" fn() -> XlStatus;
type XlGetApplConfigFn = unsafe extern "system" fn(
    *const i8,
    u32,
    *mut u32,
    *mut u32,
    *mut u32,
    u32,
) -> XlStatus;
type XlGetChannelMaskFn = unsafe extern "system" fn(i32, i32, i32) -> i64;
type XlOpenPortFn = unsafe extern "system" fn(
    *mut XlPortHandle,
    *const i8,
    XlAccessMask,
    *mut XlAccessMask,
    u32,
    u8,
    u8,
) -> XlStatus;
type XlClosePortFn = unsafe extern "system" fn(XlPortHandle) -> XlStatus;
type XlCanSetChannelBitrateFn = unsafe extern "system" fn(XlPortHandle, XlAccessMask, u32) -> XlStatus;
type XlActivateChannelFn = unsafe extern "system" fn(XlPortHandle, XlAccessMask, u32, u32) -> XlStatus;
type XlDeactivateChannelFn = unsafe extern "system" fn(XlPortHandle, XlAccessMask) -> XlStatus;
type XlCanTransmitFn = unsafe extern "system" fn(XlPortHandle, XlAccessMask, *mut u32, *mut XlEvent) -> XlStatus;
type XlReceiveFn = unsafe extern "system" fn(XlPortHandle, *mut u32, *mut XlEvent) -> XlStatus;
type XlGetErrorStringFn = unsafe extern "system" fn(XlStatus) -> *const i8;

pub struct VectorAdapter {
    _lib: Library,
    port_handle: XlPortHandle,
    channel_mask: XlAccessMask,
    channel_name: String,
    opened: bool,
    local_rx_queue: VecDeque<CanFrame>,
    fn_close_driver: XlCloseDriverFn,
    fn_open_port: XlOpenPortFn,
    fn_close_port: XlClosePortFn,
    fn_set_bitrate: XlCanSetChannelBitrateFn,
    fn_activate_channel: XlActivateChannelFn,
    fn_deactivate_channel: XlDeactivateChannelFn,
    fn_can_transmit: XlCanTransmitFn,
    fn_receive: XlReceiveFn,
    fn_get_error_string: XlGetErrorStringFn,
    fn_get_appl_config: XlGetApplConfigFn,
    fn_get_channel_mask: XlGetChannelMaskFn,
}

impl VectorAdapter {
    pub fn try_load() -> Result<Self, String> {
        let lib = unsafe {
            Library::new("vxlapi64.dll")
                .or_else(|_| Library::new("vxlapi.dll"))
                .or_else(|_| Library::new("C:\\Windows\\System32\\vxlapi64.dll"))
                .or_else(|_| Library::new("C:\\Windows\\System32\\vxlapi.dll"))
                .map_err(|e| format!("未找到 Vector 动态库: {}", e))?
        };

        unsafe {
            let fn_open_driver = *lib.get::<XlOpenDriverFn>(b"xlOpenDriver").map_err(|e| e.to_string())?.into_raw();
            let fn_close_driver = *lib.get::<XlCloseDriverFn>(b"xlCloseDriver").map_err(|e| e.to_string())?.into_raw();
            let fn_open_port = *lib.get::<XlOpenPortFn>(b"xlOpenPort").map_err(|e| e.to_string())?.into_raw();
            let fn_close_port = *lib.get::<XlClosePortFn>(b"xlClosePort").map_err(|e| e.to_string())?.into_raw();
            let fn_set_bitrate = *lib.get::<XlCanSetChannelBitrateFn>(b"xlCanSetChannelBitrate").map_err(|e| e.to_string())?.into_raw();
            let fn_activate_channel = *lib.get::<XlActivateChannelFn>(b"xlActivateChannel").map_err(|e| e.to_string())?.into_raw();
            let fn_deactivate_channel = *lib.get::<XlDeactivateChannelFn>(b"xlDeactivateChannel").map_err(|e| e.to_string())?.into_raw();
            let fn_can_transmit = *lib.get::<XlCanTransmitFn>(b"xlCanTransmit").map_err(|e| e.to_string())?.into_raw();
            let fn_receive = *lib.get::<XlReceiveFn>(b"xlReceive").map_err(|e| e.to_string())?.into_raw();
            let fn_get_error_string = *lib.get::<XlGetErrorStringFn>(b"xlGetErrorString").map_err(|e| e.to_string())?.into_raw();
            let fn_get_appl_config = *lib.get::<XlGetApplConfigFn>(b"xlGetApplConfig").map_err(|e| e.to_string())?.into_raw();
            let fn_get_channel_mask = *lib.get::<XlGetChannelMaskFn>(b"xlGetChannelMask").map_err(|e| e.to_string())?.into_raw();

            let st = fn_open_driver();
            if st != XL_SUCCESS {
                return Err(format!("xlOpenDriver 失败: 状态码 {}", st));
            }

            Ok(Self {
                _lib: lib,
                port_handle: -1,
                channel_mask: 0,
                channel_name: "CAN 1".to_string(),
                opened: false,
                local_rx_queue: VecDeque::with_capacity(1024),
                fn_close_driver,
                fn_open_port,
                fn_close_port,
                fn_set_bitrate,
                fn_activate_channel,
                fn_deactivate_channel,
                fn_can_transmit,
                fn_receive,
                fn_get_error_string,
                fn_get_appl_config,
                fn_get_channel_mask,
            })
        }
    }

    fn err_str(&self, status: XlStatus) -> String {
        unsafe {
            let ptr = (self.fn_get_error_string)(status);
            if !ptr.is_null() {
                CStr::from_ptr(ptr).to_string_lossy().into_owned()
            } else {
                format!("Error {}", status)
            }
        }
    }
}

impl CanAdapter for VectorAdapter {
    fn open(&mut self, channel: &str, bitrate: u32) -> Result<(), String> {
        self.close();

        let app_ch: u32 = if channel.contains('2') { 1 } else { 0 };

        let mut hw_type: u32 = 0;
        let mut hw_index: u32 = 0;
        let mut hw_channel: u32 = 0;
        let mut calculated_mask: i64 = 0;

        let query_apps = ["CANoe", "CANalyzer", "RustFlashingTool"];
        for app in &query_apps {
            let app_c = CString::new(*app).unwrap();
            let st = unsafe {
                (self.fn_get_appl_config)(
                    app_c.as_ptr(),
                    app_ch,
                    &mut hw_type,
                    &mut hw_index,
                    &mut hw_channel,
                    XL_BUS_TYPE_CAN,
                )
            };

            if st == XL_SUCCESS && hw_type != 0 {
                calculated_mask = unsafe {
                    (self.fn_get_channel_mask)(hw_type as i32, hw_index as i32, hw_channel as i32)
                };
                if calculated_mask > 0 {
                    break;
                }
            }
        }

        let access_mask: XlAccessMask = if calculated_mask > 0 {
            calculated_mask as u64
        } else {
            1u64 << app_ch
        };

        let app_name = CString::new("RustFlashingTool").unwrap();
        let mut port_handle: XlPortHandle = -1;
        let mut permission_mask: XlAccessMask = access_mask;
        let rx_queue_size: u32 = 2048;
        let xl_interface_version: u8 = 3;

        let mut st = unsafe {
            (self.fn_open_port)(
                &mut port_handle,
                app_name.as_ptr(),
                access_mask,
                &mut permission_mask,
                rx_queue_size,
                xl_interface_version,
                XL_BUS_TYPE_CAN as u8,
            )
        };

        if st != XL_SUCCESS {
            return Err(format!("Vector 打开端口失败: {} ({})", self.err_str(st), st));
        }

        st = unsafe { (self.fn_set_bitrate)(port_handle, access_mask, bitrate) };
        if st != XL_SUCCESS {
            unsafe { (self.fn_close_port)(port_handle) };
            return Err(format!("Vector 设置波特率 {} 失败: {}", bitrate, self.err_str(st)));
        }

        st = unsafe { (self.fn_activate_channel)(port_handle, access_mask, XL_BUS_TYPE_CAN, XL_ACTIVATE_RESET_TX_FIFO) };
        if st != XL_SUCCESS {
            unsafe { (self.fn_close_port)(port_handle) };
            return Err(format!("Vector 激活通道失败: {}", self.err_str(st)));
        }

        self.port_handle = port_handle;
        self.channel_mask = access_mask;
        self.channel_name = channel.to_string();
        self.local_rx_queue.clear();
        self.opened = true;
        Ok(())
    }

    fn close(&mut self) {
        if self.opened {
            unsafe {
                if self.port_handle >= 0 {
                    let _ = (self.fn_deactivate_channel)(self.port_handle, self.channel_mask);
                    let _ = (self.fn_close_port)(self.port_handle);
                }
            }
            self.opened = false;
            self.port_handle = -1;
            self.channel_mask = 0;
            self.local_rx_queue.clear();
        }
    }

    fn is_open(&self) -> bool {
        self.opened
    }

    fn send(&mut self, frame: &CanFrame) -> Result<(), String> {
        if !self.opened || self.port_handle < 0 {
            return Err("Vector 设备未开启".to_string());
        }

        let mut xl_event: XlEvent = unsafe { std::mem::zeroed() };
        xl_event.tag = XL_TRANSMIT_MSG;

        let can_id = if frame.is_extended {
            frame.id | XL_CAN_EXT_MSG_ID
        } else {
            frame.id
        };

        let mut data_buf = [0u8; 8];
        let len = frame.data.len().min(8);
        data_buf[..len].copy_from_slice(&frame.data[..len]);

        xl_event.tag_data.msg = XlCanMsg {
            id: can_id,
            flags: 0,
            dlc: len as u16,
            res1: 0,
            data: data_buf,
            res2: 0,
        };

        let mut msg_count: u32 = 1;
        let st = unsafe {
            (self.fn_can_transmit)(self.port_handle, self.channel_mask, &mut msg_count, &mut xl_event)
        };

        if st == XL_SUCCESS {
            Ok(())
        } else {
            Err(format!("Vector 发送失败: {} ({})", self.err_str(st), st))
        }
    }

    fn receive(&mut self, _timeout: Duration) -> Result<Option<CanFrame>, String> {
        if !self.opened || self.port_handle < 0 {
            return Err("Vector 设备未开启".to_string());
        }

        if let Some(frame) = self.local_rx_queue.pop_front() {
            return Ok(Some(frame));
        }

        let mut xl_event: XlEvent = unsafe { std::mem::zeroed() };
        let mut msg_count: u32 = 1;

        for _ in 0..64 {
            let st = unsafe { (self.fn_receive)(self.port_handle, &mut msg_count, &mut xl_event) };

            if st == XL_SUCCESS && msg_count > 0 {
                if xl_event.tag == XL_RECEIVE_MSG {
                    let msg = unsafe { xl_event.tag_data.msg };

                    if (msg.flags & XL_CAN_MSG_FLAG_TX) == 0 {
                        let is_extended = (msg.id & XL_CAN_EXT_MSG_ID) != 0;
                        let id = msg.id & !XL_CAN_EXT_MSG_ID;
                        let len = (msg.dlc as usize).min(8);

                        let frame = CanFrame {
                            id,
                            is_extended,
                            data: msg.data[..len].to_vec(),
                            is_rx: true,
                            timestamp_us: xl_event.time_stamp / 1000,
                        };
                        self.local_rx_queue.push_back(frame);
                    }
                }
            } else {
                break;
            }
        }

        Ok(self.local_rx_queue.pop_front())
    }

    fn get_active_channel_name(&self) -> String {
        self.channel_name.clone()
    }
}

impl Drop for VectorAdapter {
    fn drop(&mut self) {
        self.close();
        unsafe {
            let _ = (self.fn_close_driver)();
        }
    }
}

// =========================================================================
// 3. Kvaser CANlib (canlib32.dll) 驱动适配实现
// =========================================================================
pub type CanHandle = i32;
pub type CanStatus = i32;

const CAN_OK: CanStatus = 0;
const CAN_ERR_NOMSG: CanStatus = -2;
const CAN_OPEN_ACCEPT_VIRTUAL: i32 = 0x0020;
const CAN_MSG_EXT: u32 = 0x0004;

type CanInitializeLibraryFn = unsafe extern "system" fn();
type CanOpenChannelFn = unsafe extern "system" fn(i32, i32) -> CanHandle;
type CanCloseFn = unsafe extern "system" fn(CanHandle) -> CanStatus;
type CanSetBusParamsFn = unsafe extern "system" fn(CanHandle, i64, u32, u32, u32, u32, u32) -> CanStatus;
type CanBusOnFn = unsafe extern "system" fn(CanHandle) -> CanStatus;
type CanBusOffFn = unsafe extern "system" fn(CanHandle) -> CanStatus;
type CanWriteFn = unsafe extern "system" fn(CanHandle, i64, *const u8, u32, u32) -> CanStatus;
type CanReadFn = unsafe extern "system" fn(CanHandle, *mut i64, *mut u8, *mut u32, *mut u32, *mut u64) -> CanStatus;
type CanGetNumberOfChannelsFn = unsafe extern "system" fn(*mut i32) -> CanStatus;

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
            (self.fn_write)(
                self.handle,
                frame.id as i64,
                frame.data.as_ptr(),
                dlc,
                flags,
            )
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

// =========================================================================
// 4. Mock 虚拟设备适配实现
// =========================================================================
pub struct MockCanAdapter {
    opened: bool,
    rx_queue: VecDeque<CanFrame>,
}

impl Default for MockCanAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl MockCanAdapter {
    pub fn new() -> Self {
        Self {
            opened: false,
            rx_queue: VecDeque::new(),
        }
    }
}

impl CanAdapter for MockCanAdapter {
    fn open(&mut self, _channel: &str, _bitrate: u32) -> Result<(), String> {
        self.opened = true;
        Ok(())
    }

    fn close(&mut self) {
        self.opened = false;
        self.rx_queue.clear();
    }

    fn is_open(&self) -> bool {
        self.opened
    }

    fn send(&mut self, _frame: &CanFrame) -> Result<(), String> {
        Ok(())
    }

    fn receive(&mut self, _timeout: Duration) -> Result<Option<CanFrame>, String> {
        Ok(self.rx_queue.pop_front())
    }

    fn get_active_channel_name(&self) -> String {
        "MOCK_BUS".to_string()
    }
}

// =========================================================================
// 5. 驱动元数据注册表 (Descriptor Registry)
// =========================================================================
struct CanDriverDescriptor {
    name: &'static str,
    get_channels: fn() -> Vec<String>,
    create: fn() -> Result<Box<dyn CanAdapter>, String>,
}

fn get_pcan_channels() -> Vec<String> {
    (1..=8).map(|i| format!("PCAN_USBBUS{}", i)).collect()
}

fn get_vector_channels() -> Vec<String> {
    (1..=4).map(|i| format!("CAN {}", i)).collect()
}

fn get_kvaser_channels() -> Vec<String> {
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

fn get_socketcan_channels() -> Vec<String> {
    (0..=3).map(|i| format!("can{}", i)).collect()
}

static DRIVERS: &[CanDriverDescriptor] = &[
    CanDriverDescriptor {
        name: "PCAN USB",
        get_channels: get_pcan_channels,
        create: || PcanAdapter::try_load().map(|a| Box::new(a) as Box<dyn CanAdapter>),
    },
    CanDriverDescriptor {
        name: "Vector VN1610",
        get_channels: get_vector_channels,
        create: || VectorAdapter::try_load().map(|a| Box::new(a) as Box<dyn CanAdapter>),
    },
    CanDriverDescriptor {
        name: "Kvaser USBcan",
        get_channels: get_kvaser_channels,
        create: || KvaserAdapter::try_load().map(|a| Box::new(a) as Box<dyn CanAdapter>),
    },
    CanDriverDescriptor {
        name: "SocketCAN",
        get_channels: get_socketcan_channels,
        create: || Ok(Box::new(MockCanAdapter::new())),
    },
];

/// 供 UI 调用的全局硬件列表
pub fn supported_hardware_list() -> Vec<&'static str> {
    DRIVERS.iter().map(|d| d.name).collect()
}

/// 供 UI 调用的全局可用通道枚举
pub fn get_available_channels(hardware_name: &str) -> Vec<String> {
    for driver in DRIVERS {
        let first_token = driver.name.split_whitespace().next().unwrap_or(driver.name);
        if hardware_name.contains(first_token) || hardware_name.eq_ignore_ascii_case(driver.name) {
            return (driver.get_channels)();
        }
    }
    vec!["Channel 0".to_string(), "Channel 1".to_string()]
}

/// 统一适配器工厂
pub fn create_adapter(interface_name: &str) -> Result<Box<dyn CanAdapter>, String> {
    for driver in DRIVERS {
        let first_token = driver.name.split_whitespace().next().unwrap_or(driver.name);
        if interface_name.contains(first_token) || interface_name.eq_ignore_ascii_case(driver.name) {
            return (driver.create)();
        }
    }
    Ok(Box::new(MockCanAdapter::new()))
}