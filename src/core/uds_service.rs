use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};

/// 将接收到的字节解析为 UI 展示文本
pub fn parse_did_payload(fmt: &str, data: &[u8]) -> String {
    if data.is_empty() {
        return "".to_string();
    }
    match fmt.to_lowercase().as_str() {
        "ascii" => String::from_utf8_lossy(data)
            .trim_matches('\0')
            .trim()
            .to_string(),
        "hex" => {
            let s: Vec<String> = data.iter().map(|b| format!("{:02X}", b)).collect();
            format!("0x{}", s.join(""))
        }
        "version" => {
            // 4 字节 ASCII: 比如 '1' '0' '0' 'B' -> V1.00.B
            if data.len() >= 4 {
                let d0 = data[0] as char;
                let d1 = data[1] as char;
                let d2 = data[2] as char;
                let d3 = data[3] as char;
                if d0.is_ascii_digit()
                    && d1.is_ascii_digit()
                    && d2.is_ascii_digit()
                    && d3.is_ascii_alphabetic()
                {
                    format!("V{}.{}{}.{}", d0, d1, d2, d3)
                } else {
                    // 若下位机直接返回数值 BCD: 0x01, 0x00, 0x00, 0x42('B')
                    format!("V{}.{:02X}.{}", data[0], data[1], data[2] as char)
                }
            } else {
                String::from_utf8_lossy(data).trim().to_string()
            }
        }
        "utc" => {
            // 4 字节 Unix 时间戳
            if data.len() >= 4 {
                let ts = u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as i64;
                if let Some(dt) = DateTime::from_timestamp(ts, 0) {
                    let utc_dt: DateTime<Utc> = dt;
                    utc_dt.format("%Y-%m-%d %H:%M:%S").to_string()
                } else {
                    format!("0x{:08X}", ts)
                }
            } else {
                let s: Vec<String> = data.iter().map(|b| format!("{:02X}", b)).collect();
                format!("0x{}", s.join(""))
            }
        }
        "baud" => {
            // 每 2 字节 (u16 大端序) 解析为一个波特率值，多个用逗号隔开显示
            let mut baud_list = Vec::new();
            let mut i = 0;
            while i + 1 < data.len() && baud_list.len() < 4 {
                let val = u16::from_be_bytes([data[i], data[i + 1]]);
                baud_list.push(val.to_string());
                i += 2;
            }

            if !baud_list.is_empty() {
                baud_list.join(", ")
            } else if !data.is_empty() {
                data[0].to_string()
            } else {
                "".to_string()
            }
        }
        "dec" => {
            if data.len() == 1 {
                data[0].to_string()
            } else if data.len() == 2 {
                u16::from_be_bytes([data[0], data[1]]).to_string()
            } else if data.len() >= 4 {
                u32::from_be_bytes([data[0], data[1], data[2], data[3]]).to_string()
            } else {
                data[0].to_string()
            }
        }
        _ => String::from_utf8_lossy(data).to_string(),
    }
}

/// 写入 DID 时的输入校验与数据打包转换
pub fn validate_and_build_did_payload(
    key: &str,
    fmt: &str,
    max_len: usize,
    input: &str,
) -> Result<Vec<u8>, &'static str> {
    let raw = input.trim();
    if raw.is_empty() {
        return Err("val_err_empty");
    }

    match fmt.to_lowercase().as_str() {
        "version" => {
            let s = raw.trim_start_matches('V').trim_start_matches('v');
            let parts: Vec<&str> = s.split('.').collect();
            if parts.len() != 3 {
                return Err("val_err_version");
            }
            let major = parts[0];
            let minor = parts[1];
            let rev = parts[2];

            if major.len() != 1 || !major.chars().all(|c| c.is_ascii_digit()) {
                return Err("val_err_version");
            }
            if minor.len() != 2 || !minor.chars().all(|c| c.is_ascii_digit()) {
                return Err("val_err_version");
            }
            if rev.len() != 1 || !rev.chars().all(|c| c.is_ascii_uppercase()) {
                return Err("val_err_version");
            }

            let mut payload = Vec::with_capacity(4);
            payload.push(major.as_bytes()[0]);
            payload.push(minor.as_bytes()[0]);
            payload.push(minor.as_bytes()[1]);
            payload.push(rev.as_bytes()[0]);
            Ok(payload)
        }
        "utc" => {
            let dt = NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S")
                .map_err(|_| "val_err_utc")?;
            let ts = Utc.from_utc_datetime(&dt).timestamp();
            if ts < 0 || ts > u32::MAX as i64 {
                return Err("val_err_utc");
            }
            Ok((ts as u32).to_be_bytes().to_vec())
        }
        "ascii" => {
            let bytes = raw.as_bytes();
            if key == "dev_sn" {
                if bytes.len() != 22 || !raw.is_ascii() {
                    return Err("val_err_dev_sn");
                }
                return Ok(bytes.to_vec());
            }

            if bytes.len() > max_len {
                return Err("val_err_len_exceeded");
            }
            let mut out = bytes.to_vec();
            while out.len() < max_len {
                out.push(0x00);
            }
            Ok(out)
        }
        "hex" => {
            let clean = raw.trim_start_matches("0x").trim_start_matches("0X");
            if clean.len() % 2 != 0 {
                return Err("val_err_short_sn");
            }
            let byte_count = clean.len() / 2;
            if key == "short_sn" && byte_count != 4 {
                return Err("val_err_short_sn");
            }
            if byte_count > max_len {
                return Err("val_err_len_exceeded");
            }

            let mut out = Vec::new();
            for i in (0..clean.len()).step_by(2) {
                let b = u8::from_str_radix(&clean[i..i + 2], 16).map_err(|_| "val_err_short_sn")?;
                out.push(b);
            }
            while out.len() < max_len {
                out.push(0x00);
            }
            Ok(out)
        }
        "baud" => {
            // 1. 支持以英文逗号 ',' 或中文逗号 '，' 分隔多项
            let items: Vec<&str> = raw
                .split(|c| c == ',' || c == '，')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();

            if items.is_empty() {
                return Err("val_err_baud");
            }
            if items.len() > 4 {
                return Err("val_err_baud_count");
            }

            const VALID_BAUDRATES: [u32; 5] = [100, 125, 250, 500, 1000];
            let mut payload = Vec::with_capacity(items.len() * 2);

            for item in items {
                // 2. 支持十进制与十六进制 (0x/0X 前缀) 输入
                let baud_val: u32 = if item.starts_with("0x") || item.starts_with("0X") {
                    u32::from_str_radix(&item[2..], 16).map_err(|_| "val_err_baud")?
                } else {
                    item.parse::<u32>().map_err(|_| "val_err_baud")?
                };

                // 3. 校验有效取值范围
                if !VALID_BAUDRATES.contains(&baud_val) {
                    return Err("val_err_baud_range");
                }

                // 4. 每组占 2 字节（u16 大端序）
                let b_u16 = baud_val as u16;
                payload.extend_from_slice(&b_u16.to_be_bytes());
            }

            // 根据组数自动生成对应字节数: 1组->2字节, 2组->4字节, 3组->6字节, 4组->8字节
            Ok(payload)
        }
        _ => Ok(raw.as_bytes().to_vec()),
    }
}
