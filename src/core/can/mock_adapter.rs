// src/core/can/mock_adapter.rs
use super::{CanAdapter, CanFrame};
use std::collections::VecDeque;
use std::time::Duration;

pub fn get_channels() -> Vec<String> {
    vec!["Virtual Channel 1".to_string(), "Virtual Channel 2".to_string()]
}

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
        "VIRTUAL_BUS".to_string()
    }
}