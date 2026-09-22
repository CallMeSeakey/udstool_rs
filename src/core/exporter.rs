// src/core/exporter.rs
use chrono::{DateTime, Datelike, Local, Timelike};
use flate2::write::ZlibEncoder;
use flate2::Compression;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

/// 具备高精度总线时间戳的原生 CAN 记录项
#[derive(Clone, Debug)]
pub struct CanLogItem {
    pub timestamp_us: u64, // 微秒级高精度单调时间戳
    pub channel: u8,
    pub id: u32,
    pub is_ext: bool,
    pub is_rx: bool,
    pub dlc: u8,
    pub data: Vec<u8>,
    pub formatted: String, // 供 UI 界面渲染的高性能预渲染字符串
}

pub struct LogExporter;

impl LogExporter {
    /// 导出刷写日志
    pub fn export_flash_log(logs: &[(String, String)]) -> Result<Option<PathBuf>, String> {
        let time_tag = Local::now().format("%Y%m%d_%H%M%S").to_string();
        let default_name = format!("flash_log_{}.txt", time_tag);

        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Text Log (*.txt, *.log)", &["txt", "log"])
            .set_file_name(&default_name)
            .save_file()
        {
            let mut file = File::create(&path).map_err(|e| e.to_string())?;
            for (msg, _) in logs {
                writeln!(file, "{}", msg).map_err(|e| e.to_string())?;
            }
            return Ok(Some(path));
        }
        Ok(None)
    }

    /// 导出 CAN 总线原始高精度报文日志
    pub fn export_can_log(logs: &[CanLogItem]) -> Result<Option<PathBuf>, String> {
        let now = Local::now();
        let time_tag = now.format("%Y%m%d_%H%M%S").to_string();
        let default_name = format!("can_trace_{}.asc", time_tag);

        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Vector ASCII Trace (*.asc)", &["asc"])
            .add_filter("PCAN Trace (*.trc)", &["trc"])
            .add_filter("Vector Binary Log (*.blf)", &["blf"])
            .add_filter("ASAM MDF4 Measurement (*.mf4)", &["mf4"])
            .add_filter("Text Log (*.log, *.txt)", &["log", "txt"])
            .set_file_name(&default_name)
            .save_file()
        {
            let ext = path
                .extension()
                .unwrap_or_default()
                .to_str()
                .unwrap_or("")
                .to_lowercase();

            match ext.as_str() {
                "asc" => Self::export_asc(&path, logs, now)?,
                "trc" => Self::export_trc(&path, logs, now)?,
                "blf" => Self::export_blf(&path, logs, now)?,
                "mf4" => Self::export_mf4(&path, logs, now)?,
                _ => Self::export_raw_log(&path, logs)?,
            }
            return Ok(Some(path));
        }
        Ok(None)
    }

    /// 导出 Vector CANalyzer / CANoe 规范的 .asc 格式 (保留 6 位小数，微秒精度)
    fn export_asc(path: &Path, frames: &[CanLogItem], time: DateTime<Local>) -> Result<(), String> {
        let mut file = File::create(path).map_err(|e| e.to_string())?;

        writeln!(file, "date {}", time.format("%a %b %d %I:%M:%S %p %Y")).map_err(|e| e.to_string())?;
        writeln!(file, "base hex  timestamps absolute").map_err(|e| e.to_string())?;
        writeln!(file, "internal events logged").map_err(|e| e.to_string())?;
        writeln!(file, "// version 8.1.0").map_err(|e| e.to_string())?;
        writeln!(file, "Begin TriggerBlock {}", time.format("%a %b %d %I:%M:%S %p %Y")).map_err(|e| e.to_string())?;
        writeln!(file, "   0.000000 Start of measurement").map_err(|e| e.to_string())?;

        let base_us = frames.first().map(|f| f.timestamp_us).unwrap_or(0);

        for f in frames {
            let offset_sec = (f.timestamp_us.saturating_sub(base_us)) as f64 / 1_000_000.0;
            let dir = if f.is_rx { "Rx" } else { "Tx" };
            let id_str = if f.is_ext {
                format!("{:08X}x", f.id)
            } else {
                format!("{:03X}", f.id)
            };

            let mut data_hex = String::with_capacity(f.data.len() * 3);
            for b in &f.data {
                data_hex.push_str(&format!(" {:02X}", b));
            }

            writeln!(
                file,
                "{:11.6} {}  {:<9} {}   d {:<2}{}",
                offset_sec, f.channel, id_str, dir, f.dlc, data_hex
            )
            .map_err(|e| e.to_string())?;
        }

        writeln!(file, "End TriggerBlock").map_err(|e| e.to_string())?;
        Ok(())
    }

    /// 导出 PEAK PCAN Trace 规范的 .trc 格式 (v2.1，微秒转化毫秒带 3 位小数)
    fn export_trc(path: &Path, frames: &[CanLogItem], time: DateTime<Local>) -> Result<(), String> {
        let mut file = File::create(path).map_err(|e| e.to_string())?;

        writeln!(file, ";$FILEVERSION=2.1").map_err(|e| e.to_string())?;
        writeln!(
            file,
            ";$STARTTIME={}",
            (time.timestamp_millis() as f64) / 86400000.0 + 25569.0
        )
        .map_err(|e| e.to_string())?;
        writeln!(file, ";$COLUMNS=N,O,T,B,I,d,R,D").map_err(|e| e.to_string())?;
        writeln!(file, ";").map_err(|e| e.to_string())?;
        writeln!(file, ";   Message   Time    Type    ID     Length  Data Bytes").map_err(|e| e.to_string())?;
        writeln!(file, ";   Number    Offset  Bus     (hex)          (hex) ...").map_err(|e| e.to_string())?;
        writeln!(file, ";------------------------------------------------------------------").map_err(|e| e.to_string())?;

        let base_us = frames.first().map(|f| f.timestamp_us).unwrap_or(0);

        for (idx, f) in frames.iter().enumerate() {
            let offset_ms = (f.timestamp_us.saturating_sub(base_us)) as f64 / 1_000.0;
            let type_str = if f.is_rx { "Rx" } else { "Tx" };
            let mut data_hex = String::with_capacity(f.data.len() * 3);
            for b in &f.data {
                data_hex.push_str(&format!(" {:02X}", b));
            }

            writeln!(
                file,
                "{:7} {:10.3}   {:1}  {:2}  {:08X}  {:2}{}",
                idx + 1,
                offset_ms,
                f.channel,
                type_str,
                f.id,
                f.dlc,
                data_hex
            )
            .map_err(|e| e.to_string())?;
        }

        Ok(())
    }

    /// 导出符合 Vector 官方 CAN_MESSAGE2 (Type 86) 规范的 Binary Log (.blf)
    fn export_blf(path: &Path, frames: &[CanLogItem], time: DateTime<Local>) -> Result<(), String> {
        let mut file = File::create(path).map_err(|e| e.to_string())?;
        let base_us = frames.first().map(|f| f.timestamp_us).unwrap_or(0);
        let last_us = frames.last().map(|f| f.timestamp_us).unwrap_or(base_us);
        let duration_ns = (last_us.saturating_sub(base_us)) * 1_000;

        // 1. 组装容器内部所有的 CAN_MESSAGE2 (Type 86) 对象
        let mut uncompressed_data = Vec::with_capacity(frames.len() * 64);

        for f in frames {
            let total_obj_size: u32 = 64; // 32 字节 Header + 32 字节 Body = 64 字节
            let obj_type: u32 = 86;       // 标准 CAN_MESSAGE2 (兼容所有现代 BLF 查看器及 CANoe)
            let ts_ns = (f.timestamp_us.saturating_sub(base_us)) * 1_000;

            // -------- V2 Object Base Header (32 字节) --------
            uncompressed_data.extend_from_slice(b"LOBJ");                      // Signature "LOBJ"
            uncompressed_data.extend_from_slice(&32u16.to_le_bytes());         // Header size = 32
            uncompressed_data.extend_from_slice(&1u16.to_le_bytes());          // Header version = 1
            uncompressed_data.extend_from_slice(&total_obj_size.to_le_bytes()); // Total object size = 64
            uncompressed_data.extend_from_slice(&obj_type.to_le_bytes());      // Type = 86
            uncompressed_data.extend_from_slice(&1u32.to_le_bytes());          // Flags = 1 (Time valid)
            uncompressed_data.extend_from_slice(&0u16.to_le_bytes());          // Client index
            uncompressed_data.extend_from_slice(&0u16.to_le_bytes());          // Object version
            uncompressed_data.extend_from_slice(&ts_ns.to_le_bytes());         // Timestamp (ns)

            // -------- CAN_MESSAGE2 Body (32 字节) --------
            let channel: u16 = (f.channel as u16).max(1);
            uncompressed_data.extend_from_slice(&channel.to_le_bytes());       // 2B: Channel

            // Flags: bit 0: 0=Rx, 1=Tx
            let flags: u8 = if f.is_rx { 0x00 } else { 0x01 };
            uncompressed_data.push(flags);                                     // 1B: Flags
            uncompressed_data.push(f.dlc);                                     // 1B: DLC

            // CAN ID (最高位 bit 31 为扩展帧 IDE 标志)
            let can_id = if f.is_ext { f.id | 0x8000_0000 } else { f.id };
            uncompressed_data.extend_from_slice(&can_id.to_le_bytes());        // 4B: CAN ID

            // 8 字节数据
            let mut data_padded = [0u8; 8];
            for (i, b) in f.data.iter().take(8).enumerate() {
                data_padded[i] = *b;
            }
            uncompressed_data.extend_from_slice(&data_padded);                 // 8B: Data

            // 其余 16 字节固定填充 (frameLength, bitCount, reserved)
            uncompressed_data.extend_from_slice(&[0u8; 16]);                   // 16B: Reserved padding
        }

        // 2. 对所有对象采用 zlib (Deflate) 压缩
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder
            .write_all(&uncompressed_data)
            .map_err(|e| e.to_string())?;
        let compressed_bytes = encoder.finish().map_err(|e| e.to_string())?;

        // 3. 构建 LOG_CONTAINER
        let uncompressed_size = uncompressed_data.len() as u32;
        let compressed_size = compressed_bytes.len() as u32;
        let container_header_size: u32 = 32 + 16;
        let raw_total_container_size = container_header_size + compressed_size;

        // 严格 4 字节填充对齐
        let pad_len = (4 - (raw_total_container_size % 4)) % 4;
        let total_container_size = raw_total_container_size + pad_len;

        let mut log_container = Vec::with_capacity(total_container_size as usize);

        // --- Container Base Header (32 字节) ---
        log_container.extend_from_slice(b"LOBJ");
        log_container.extend_from_slice(&32u16.to_le_bytes());
        log_container.extend_from_slice(&1u16.to_le_bytes());
        log_container.extend_from_slice(&total_container_size.to_le_bytes());
        log_container.extend_from_slice(&10u32.to_le_bytes()); // Type 10 = LOG_CONTAINER
        log_container.extend_from_slice(&0u32.to_le_bytes());  // Flags
        log_container.extend_from_slice(&0u16.to_le_bytes());  // Client index
        log_container.extend_from_slice(&0u16.to_le_bytes());  // Object version
        log_container.extend_from_slice(&0u64.to_le_bytes());  // Timestamp 0

        // --- Container Specific Header (16 字节) ---
        log_container.extend_from_slice(&2u16.to_le_bytes());  // Compression method: 2 = Deflate
        log_container.extend_from_slice(&[0u8; 6]);            // Reserved
        log_container.extend_from_slice(&uncompressed_size.to_le_bytes());
        log_container.extend_from_slice(&[0u8; 4]);            // Reserved

        log_container.extend_from_slice(&compressed_bytes);
        for _ in 0..pad_len {
            log_container.push(0x00);
        }

        // 4. 构建完整的 144 字节 Vector BLF 文件头
        let mut header = [0u8; 144];
        header[0..4].copy_from_slice(b"LOGG");
        header[4..8].copy_from_slice(&144u32.to_le_bytes());
        header[8] = 2; // Application: CANoe / CANalyzer
        header[9] = 1; // Major
        header[10] = 0;
        header[11] = 0;
        header[12..16].copy_from_slice(&1u32.to_le_bytes()); // File Version = 1

        let file_total_len = 144 + log_container.len() as u64;
        header[16..24].copy_from_slice(&file_total_len.to_le_bytes());
        header[24..32].copy_from_slice(&(uncompressed_size as u64).to_le_bytes());
        header[32..36].copy_from_slice(&(frames.len() as u32).to_le_bytes()); // Total Objects
        header[36..40].copy_from_slice(&1u32.to_le_bytes());                  // Total Containers = 1

        let duration_chrono = chrono::Duration::nanoseconds(duration_ns as i64);
        let stop_time = time + duration_chrono;

        // Start Time (SYSTEMTIME, 16 字节, 偏移 40..56)
        header[40..42].copy_from_slice(&(time.year() as u16).to_le_bytes());
        header[42..44].copy_from_slice(&(time.month() as u16).to_le_bytes());
        header[44..46].copy_from_slice(&(time.weekday().num_days_from_sunday() as u16).to_le_bytes());
        header[46..48].copy_from_slice(&(time.day() as u16).to_le_bytes());
        header[48..50].copy_from_slice(&(time.hour() as u16).to_le_bytes());
        header[50..52].copy_from_slice(&(time.minute() as u16).to_le_bytes());
        header[52..54].copy_from_slice(&(time.second() as u16).to_le_bytes());
        header[54..56].copy_from_slice(&(time.timestamp_subsec_millis() as u16).to_le_bytes());

        // Stop Time (SYSTEMTIME, 16 字节, 偏移 56..72)
        header[56..58].copy_from_slice(&(stop_time.year() as u16).to_le_bytes());
        header[58..60].copy_from_slice(&(stop_time.month() as u16).to_le_bytes());
        header[60..62].copy_from_slice(&(stop_time.weekday().num_days_from_sunday() as u16).to_le_bytes());
        header[62..64].copy_from_slice(&(stop_time.day() as u16).to_le_bytes());
        header[64..66].copy_from_slice(&(stop_time.hour() as u16).to_le_bytes());
        header[66..68].copy_from_slice(&(stop_time.minute() as u16).to_le_bytes());
        header[68..70].copy_from_slice(&(stop_time.second() as u16).to_le_bytes());
        header[70..72].copy_from_slice(&(stop_time.timestamp_subsec_millis() as u16).to_le_bytes());

        // 起始与结束的高精纳秒时间戳 (偏移 72..88)
        header[72..80].copy_from_slice(&0u64.to_le_bytes());
        header[80..88].copy_from_slice(&duration_ns.to_le_bytes());

        file.write_all(&header).map_err(|e| e.to_string())?;
        file.write_all(&log_container).map_err(|e| e.to_string())?;

        Ok(())
    }

    /// 导出 ASAM MDF4 标准测量文件格式 (.mf4)
    fn export_mf4(path: &Path, frames: &[CanLogItem], time: DateTime<Local>) -> Result<(), String> {
        let mut file = File::create(path).map_err(|e| e.to_string())?;
        let base_us = frames.first().map(|f| f.timestamp_us).unwrap_or(0);

        let mut id_block = [0u8; 64];
        id_block[0..8].copy_from_slice(b"MDF     ");
        id_block[8..16].copy_from_slice(b"4.10    ");
        id_block[16..24].copy_from_slice(b"TOOL_RS ");
        id_block[28..30].copy_from_slice(&410u16.to_le_bytes());
        file.write_all(&id_block).map_err(|e| e.to_string())?;

        let mut hd_block = Vec::new();
        hd_block.extend_from_slice(b"##HD");
        hd_block.extend_from_slice(&[0u8; 4]);
        hd_block.extend_from_slice(&104u64.to_le_bytes());
        hd_block.extend_from_slice(&6u64.to_le_bytes());

        let dg_link: u64 = 64 + 104;
        hd_block.extend_from_slice(&dg_link.to_le_bytes());
        hd_block.extend_from_slice(&0u64.to_le_bytes());
        hd_block.extend_from_slice(&0u64.to_le_bytes());
        hd_block.extend_from_slice(&0u64.to_le_bytes());
        hd_block.extend_from_slice(&0u64.to_le_bytes());
        hd_block.extend_from_slice(&0u64.to_le_bytes());

        let ns_since_epoch = (time.timestamp_nanos_opt().unwrap_or(0)) as u64;
        hd_block.extend_from_slice(&ns_since_epoch.to_le_bytes());
        hd_block.extend_from_slice(&0i16.to_le_bytes());
        hd_block.extend_from_slice(&0i16.to_le_bytes());
        hd_block.extend_from_slice(&1u8.to_le_bytes());
        hd_block.extend_from_slice(&[0u8; 23]);
        file.write_all(&hd_block).map_err(|e| e.to_string())?;

        let dt_link = dg_link + 64;
        let mut dg_block = Vec::new();
        dg_block.extend_from_slice(b"##DG");
        dg_block.extend_from_slice(&[0u8; 4]);
        dg_block.extend_from_slice(&64u64.to_le_bytes());
        dg_block.extend_from_slice(&4u64.to_le_bytes());
        dg_block.extend_from_slice(&0u64.to_le_bytes());
        dg_block.extend_from_slice(&0u64.to_le_bytes());
        dg_block.extend_from_slice(&dt_link.to_le_bytes());
        dg_block.extend_from_slice(&0u64.to_le_bytes());
        dg_block.extend_from_slice(&1u8.to_le_bytes());
        dg_block.extend_from_slice(&[0u8; 7]);
        file.write_all(&dg_block).map_err(|e| e.to_string())?;

        let record_size = 24usize;
        let data_payload_size = frames.len() * record_size;
        let dt_block_len = 24 + data_payload_size as u64;

        let mut dt_header = Vec::new();
        dt_header.extend_from_slice(b"##DT");
        dt_header.extend_from_slice(&[0u8; 4]);
        dt_header.extend_from_slice(&dt_block_len.to_le_bytes());
        dt_header.extend_from_slice(&0u64.to_le_bytes());
        file.write_all(&dt_header).map_err(|e| e.to_string())?;

        for f in frames {
            let mut record = [0u8; 24];
            let ts_ns = (f.timestamp_us.saturating_sub(base_us)) * 1_000;
            record[0..8].copy_from_slice(&ts_ns.to_le_bytes());
            record[8] = f.channel;
            record[9] = if f.is_rx { 0 } else { 1 };
            let can_id = if f.is_ext { f.id | 0x8000_0000 } else { f.id };
            record[12..16].copy_from_slice(&can_id.to_le_bytes());
            for (i, b) in f.data.iter().take(8).enumerate() {
                record[16 + i] = *b;
            }
            file.write_all(&record).map_err(|e| e.to_string())?;
        }

        Ok(())
    }

    /// 导出原始文本
    fn export_raw_log(path: &Path, frames: &[CanLogItem]) -> Result<(), String> {
        let mut file = File::create(path).map_err(|e| e.to_string())?;
        for f in frames {
            let dir = if f.is_rx { "RX" } else { "TX" };
            writeln!(file, "[{}] {}", dir, f.formatted).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}