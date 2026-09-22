// src/core/can/vector_adapter.rs
use super::{CanAdapter, CanFrame};
use libloading::Library;
use std::collections::VecDeque;
use std::ffi::{CStr, CString};
use std::time::Duration;

pub type XlPortHandle = i32;
pub type XlAccessMask = u64;
pub type XlStatus = i32;

const XL_SUCCESS: XlStatus = 0;
const XL_ERR_QUEUE_IS_EMPTY: XlStatus = 10;
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
type XlGetApplConfigFn =
    unsafe extern "system" fn(*const i8, u32, *mut u32, *mut u32, *mut u32, u32) -> XlStatus;
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
type XlCanSetChannelBitrateFn =
    unsafe extern "system" fn(XlPortHandle, XlAccessMask, u32) -> XlStatus;
type XlActivateChannelFn =
    unsafe extern "system" fn(XlPortHandle, XlAccessMask, u32, u32) -> XlStatus;
type XlDeactivateChannelFn = unsafe extern "system" fn(XlPortHandle, XlAccessMask) -> XlStatus;
type XlCanTransmitFn =
    unsafe extern "system" fn(XlPortHandle, XlAccessMask, *mut u32, *mut XlEvent) -> XlStatus;
type XlReceiveFn = unsafe extern "system" fn(XlPortHandle, *mut u32, *mut XlEvent) -> XlStatus;
type XlGetErrorStringFn = unsafe extern "system" fn(XlStatus) -> *const i8;

pub fn get_channels() -> Vec<String> {
    (1..=4).map(|i| format!("CAN {}", i)).collect()
}

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
            let fn_open_driver = *lib
                .get::<XlOpenDriverFn>(b"xlOpenDriver")
                .map_err(|e| e.to_string())?
                .into_raw();
            let fn_close_driver = *lib
                .get::<XlCloseDriverFn>(b"xlCloseDriver")
                .map_err(|e| e.to_string())?
                .into_raw();
            let fn_open_port = *lib
                .get::<XlOpenPortFn>(b"xlOpenPort")
                .map_err(|e| e.to_string())?
                .into_raw();
            let fn_close_port = *lib
                .get::<XlClosePortFn>(b"xlClosePort")
                .map_err(|e| e.to_string())?
                .into_raw();
            let fn_set_bitrate = *lib
                .get::<XlCanSetChannelBitrateFn>(b"xlCanSetChannelBitrate")
                .map_err(|e| e.to_string())?
                .into_raw();
            let fn_activate_channel = *lib
                .get::<XlActivateChannelFn>(b"xlActivateChannel")
                .map_err(|e| e.to_string())?
                .into_raw();
            let fn_deactivate_channel = *lib
                .get::<XlDeactivateChannelFn>(b"xlDeactivateChannel")
                .map_err(|e| e.to_string())?
                .into_raw();
            let fn_can_transmit = *lib
                .get::<XlCanTransmitFn>(b"xlCanTransmit")
                .map_err(|e| e.to_string())?
                .into_raw();
            let fn_receive = *lib
                .get::<XlReceiveFn>(b"xlReceive")
                .map_err(|e| e.to_string())?
                .into_raw();
            let fn_get_error_string = *lib
                .get::<XlGetErrorStringFn>(b"xlGetErrorString")
                .map_err(|e| e.to_string())?
                .into_raw();
            let fn_get_appl_config = *lib
                .get::<XlGetApplConfigFn>(b"xlGetApplConfig")
                .map_err(|e| e.to_string())?
                .into_raw();
            let fn_get_channel_mask = *lib
                .get::<XlGetChannelMaskFn>(b"xlGetChannelMask")
                .map_err(|e| e.to_string())?
                .into_raw();

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

        // 提取通道索引 (1-based -> 0-based)
        let ch_num: u32 = channel
            .chars()
            .filter(|c| c.is_ascii_digit())
            .collect::<String>()
            .parse::<u32>()
            .unwrap_or(1)
            .saturating_sub(1);

        let mut hw_type: u32 = 0;
        let mut hw_index: u32 = 0;
        let mut hw_channel: u32 = 0;
        let mut access_mask: XlAccessMask = 0;

        // 优先从现有 Vector 硬件应用配置中查找对应的物理 Mask
        let app_names = ["RustUdsTool", "CANoe", "CANalyzer"];
        for app in &app_names {
            let app_c = CString::new(*app).unwrap();
            let st = unsafe {
                (self.fn_get_appl_config)(
                    app_c.as_ptr(),
                    ch_num,
                    &mut hw_type,
                    &mut hw_index,
                    &mut hw_channel,
                    XL_BUS_TYPE_CAN,
                )
            };

            if st == XL_SUCCESS && hw_type != 0 {
                let m = unsafe {
                    (self.fn_get_channel_mask)(hw_type as i32, hw_index as i32, hw_channel as i32)
                };
                if m > 0 {
                    access_mask = m as u64;
                    break;
                }
            }
        }

        // 兜底：直接按通道位偏移作为掩码
        if access_mask == 0 {
            access_mask = 1u64 << ch_num;
        }

        let app_name = CString::new("RustUdsTool").unwrap();
        let mut port_handle: XlPortHandle = -1;
        let rx_queue_size: u32 = 1024;
        let xl_interface_version: u8 = 3;

        // ==================== 权限策略：以 Rust App 为主控 ====================
        // 步骤 1：优先尝试获取完整的 Init Access 权限
        let mut permission_mask: XlAccessMask = access_mask;
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
            permission_mask = 0;
            st = unsafe {
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
        }

        if st != XL_SUCCESS {
            return Err(format!(
                "Vector 打开端口失败: {} ({}) [Channel Mask: 0x{:X}]",
                self.err_str(st),
                st,
                access_mask
            ));
        }

        // 步骤 3：只有主控端（获得了 permission_mask 授权）才去设定波特率
        if permission_mask != 0 {
            let br_st = unsafe { (self.fn_set_bitrate)(port_handle, access_mask, bitrate) };
            if br_st != XL_SUCCESS {
                unsafe { (self.fn_close_port)(port_handle) };
                return Err(format!(
                    "Vector 设置波特率 {} 失败: {} ({})",
                    bitrate,
                    self.err_str(br_st),
                    br_st
                ));
            }
        }

        // 步骤 4：激活通道通信
        st = unsafe {
            (self.fn_activate_channel)(
                port_handle,
                access_mask,
                XL_BUS_TYPE_CAN,
                XL_ACTIVATE_RESET_TX_FIFO,
            )
        };

        if st != XL_SUCCESS {
            unsafe { (self.fn_close_port)(port_handle) };
            return Err(format!(
                "Vector 激活通道失败: {} ({})",
                self.err_str(st),
                st
            ));
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
            (self.fn_can_transmit)(
                self.port_handle,
                self.channel_mask,
                &mut msg_count,
                &mut xl_event,
            )
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

        // 每次批量读取最多 64 帧缓冲
        for _ in 0..64 {
            let st = unsafe { (self.fn_receive)(self.port_handle, &mut msg_count, &mut xl_event) };
            if st == XL_SUCCESS && msg_count > 0 {
                if xl_event.tag == XL_RECEIVE_MSG {
                    let msg = unsafe { xl_event.tag_data.msg };
                    // 仅提取纯接收帧，滤除 TX 本机回环回显
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
