// src/core/can/mod.rs
pub mod etas_adapter;
pub mod gs_usb_adapter; // 1. 引入 gs_usb 模块
pub mod kvaser_adapter;
pub mod mock_adapter;
pub mod pcan_adapter;
pub mod vector_adapter;

use std::time::Duration;

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

pub struct CanDriverDescriptor {
    pub name: &'static str,
    pub get_channels: fn() -> Vec<String>,
    pub create: fn() -> Result<Box<dyn CanAdapter>, String>,
}

static DRIVERS: &[CanDriverDescriptor] = &[
    CanDriverDescriptor {
        name: "PCAN USB",
        get_channels: pcan_adapter::get_channels,
        create: || pcan_adapter::PcanAdapter::try_load().map(|a| Box::new(a) as Box<dyn CanAdapter>),
    },
    CanDriverDescriptor {
        name: "Vector VN1610",
        get_channels: vector_adapter::get_channels,
        create: || vector_adapter::VectorAdapter::try_load().map(|a| Box::new(a) as Box<dyn CanAdapter>),
    },
    CanDriverDescriptor {
        name: "Kvaser USBcan",
        get_channels: kvaser_adapter::get_channels,
        create: || kvaser_adapter::KvaserAdapter::try_load().map(|a| Box::new(a) as Box<dyn CanAdapter>),
    },
    CanDriverDescriptor {
        name: "ETAS ES582",
        get_channels: etas_adapter::get_channels,
        create: || etas_adapter::EtasAdapter::try_load().map(|a| Box::new(a) as Box<dyn CanAdapter>),
    },
    // 2. 注册 gs_usb (candleLight)
    CanDriverDescriptor {
        name: "gs_usb (candleLight)",
        get_channels: gs_usb_adapter::get_channels,
        create: || gs_usb_adapter::GsUsbAdapter::try_load().map(|a| Box::new(a) as Box<dyn CanAdapter>),
    },
    CanDriverDescriptor {
        name: "Virtual CAN",
        get_channels: mock_adapter::get_channels,
        create: || Ok(Box::new(mock_adapter::MockCanAdapter::new())),
    },
];

pub fn supported_hardware_list() -> Vec<&'static str> {
    DRIVERS.iter().map(|d| d.name).collect()
}

pub fn get_available_channels(hardware_name: &str) -> Vec<String> {
    for driver in DRIVERS {
        let first_token = driver.name.split_whitespace().next().unwrap_or(driver.name);
        if hardware_name.contains(first_token) || hardware_name.eq_ignore_ascii_case(driver.name) {
            return (driver.get_channels)();
        }
    }
    vec!["Channel 0".to_string(), "Channel 1".to_string()]
}

pub fn create_adapter(interface_name: &str) -> Result<Box<dyn CanAdapter>, String> {
    for driver in DRIVERS {
        let first_token = driver.name.split_whitespace().next().unwrap_or(driver.name);
        if interface_name.contains(first_token) || interface_name.eq_ignore_ascii_case(driver.name) {
            return (driver.create)();
        }
    }
    Ok(Box::new(mock_adapter::MockCanAdapter::new()))
}