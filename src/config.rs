// src/config.rs
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};

pub const APP_VERSION: &str = "0.9.0";
pub const APP_AUTHOR: &str = "Hugo";
pub const APP_EMAIL: &str = "yinpuhui@csshuobo.com";
pub const APP_COMPANY: &str = "SonnePower";
pub const CUSTOM_PRODUCT_ID: &str = "__custom__";

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct ProductConfig {
    pub name: String,
    pub txid: String,
    pub rxid: String,
    pub baudrate: String,
    pub app_address: String,
    pub boot_address: String,

    #[serde(default)]
    pub override_security: bool,
    #[serde(default)]
    pub security_service: Option<String>,
    #[serde(default)]
    pub security_algo_or_cert: Option<String>,
    #[serde(default)]
    pub verify_method: Option<String>,

    #[serde(default)]
    pub flash_rid_overrides: HashMap<String, RidOverride>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct RidOverride {
    pub erase_rid: Option<String>,
    pub verify_rid: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct DidConfig {
    pub did: u16,
    pub name: String,
    pub name_i18n: HashMap<String, String>,
    pub fmt: String,
    pub len: usize,
    pub rw: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct AppConfig {
    #[serde(default = "default_lang")]
    pub language: String,
    pub default_interface: String,
    pub default_channel: String,
    pub default_baudrate: String,
    pub default_product: String,
    #[serde(default = "default_flash_type_val")]
    pub default_flash_type: String,
    #[serde(default)]
    pub theme_mode: usize,
    #[serde(default)]
    pub default_filter_mode: usize,
    #[serde(default = "default_true")]
    pub default_auto_reboot: bool,
    pub security_service: String,
    pub verify_method: String,
    pub isotp_tx_padding: String,
    pub auth_0x29_cert_path: String,
    pub products: HashMap<String, ProductConfig>,
    pub dids: HashMap<String, DidConfig>,
    pub security_algos: HashMap<String, String>,
}

impl AppConfig {
    /// 确保 products 列表中始终存在系统预设的自定义实体配置
    pub fn ensure_custom_product(&mut self) {
        if !self.products.contains_key(CUSTOM_PRODUCT_ID) {
            self.products.insert(
                CUSTOM_PRODUCT_ID.to_string(),
                ProductConfig {
                    name: CUSTOM_PRODUCT_ID.to_string(),
                    txid: "0x18FFFE68".to_string(),
                    rxid: "0x18FF68FE".to_string(),
                    baudrate: "1000000".to_string(),
                    app_address: "0x15020000".to_string(),
                    boot_address: "0x08000000".to_string(),
                    override_security: false,
                    security_service: None,
                    security_algo_or_cert: None,
                    verify_method: None,
                    flash_rid_overrides: get_default_rid_overrides(),
                },
            );
        }
    }
}

fn default_lang() -> String {
    "zh".to_string()
}

fn default_flash_type_val() -> String {
    "app".to_string()
}

fn default_true() -> bool {
    true
}

pub fn get_default_rid_overrides() -> HashMap<String, RidOverride> {
    let mut map = HashMap::new();
    map.insert(
        "主RTS程序".to_string(),
        RidOverride {
            erase_rid: Some("0x1001".to_string()),
            verify_rid: Some("0x1002".to_string()),
        },
    );
    map.insert(
        "主APP程序".to_string(),
        RidOverride {
            erase_rid: Some("0x1011".to_string()),
            verify_rid: Some("0x1012".to_string()),
        },
    );
    map.insert(
        "从MCU程序".to_string(),
        RidOverride {
            erase_rid: Some("0x1021".to_string()),
            verify_rid: Some("0x1022".to_string()),
        },
    );
    map.insert(
        "BOOT程序".to_string(),
        RidOverride {
            erase_rid: Some("0x1041".to_string()),
            verify_rid: Some("0x1042".to_string()),
        },
    );
    map
}

pub fn get_default_dids() -> HashMap<String, DidConfig> {
    let mut map = HashMap::new();
    let defs = [
        ("dev_sn", 0xFD20, "设备SN", "Device SN", "ascii", 22, "RW"),
        ("short_sn", 0xFD21, "简短SN", "Short SN", "hex", 4, "RW"),
        ("hardware_version", 0xFD22, "硬件版本", "HW Version", "version", 4, "RW"),
        ("prouditon_date", 0xFD23, "生产日期", "Mfg Date", "utc", 4, "RW"),
        ("node_id", 0xFD27, "节点ID", "Node ID", "hex", 8, "RW"),
        ("can_baudrate", 0xFD28, "CAN波特率(kbps)", "CAN Baudrate (kbps)", "baud", 8, "RW"),
        ("uds_version", 0xFDA0, "UDS版本", "UDS Version", "version", 4, "RO"),
        ("uds_phys_tx_id", 0xFDA1, "UDS Phys TX ID", "UDS Phys TX ID", "hex", 4, "RO"),
        ("uds_phys_rx_id", 0xFDA2, "UDS Phys RX ID", "UDS Phys RX ID", "hex", 4, "RO"),
        ("boot_version", 0xFDA3, "Boot版本号", "Boot Version", "version", 4, "RO"),
        ("boot_build_date", 0xFDA4, "Boot构建日期", "Boot Build Date", "utc", 4, "RO"),
        ("boot_commit_id", 0xFDBE, "Boot提交ID", "Boot Commit ID", "ascii", 8, "RO"),
        ("app_version", 0xFDB3, "App版本号", "App Version", "version", 4, "RO"),
        ("app_build_date", 0xFDB4, "App构建日期", "App Build Date", "utc", 4, "RO"),
        ("app_commit_id", 0xFDBF, "App提交ID", "App Commit ID", "ascii", 8, "RO"),
    ];

    for (k, did, zh, en, fmt, len, rw) in defs {
        let mut i18n_map = HashMap::new();
        i18n_map.insert("zh".to_string(), zh.to_string());
        i18n_map.insert("en".to_string(), en.to_string());
        map.insert(
            k.to_string(),
            DidConfig {
                did,
                name: zh.to_string(),
                name_i18n: i18n_map,
                fmt: fmt.to_string(),
                len,
                rw: rw.to_string(),
            },
        );
    }
    map
}

impl Default for AppConfig {
    fn default() -> Self {
        let mut products = HashMap::new();
        products.insert(
            "SPC-SDIO-L2".to_string(),
            ProductConfig {
                name: "SPC-SDIO-L2".to_string(),
                txid: "0x18FFFE58".to_string(),
                rxid: "0x18FF58FE".to_string(),
                baudrate: "500000".to_string(),
                app_address: "0x08010000".to_string(),
                boot_address: "0x08000000".to_string(),
                override_security: false,
                security_service: None,
                security_algo_or_cert: None,
                verify_method: None,
                flash_rid_overrides: get_default_rid_overrides(),
            },
        );
        products.insert(
            "SPC-SDIO-S6".to_string(),
            ProductConfig {
                name: "SPC-SDIO-S6".to_string(),
                txid: "0x18FFFE32".to_string(),
                rxid: "0x18FF32FE".to_string(),
                baudrate: "250000".to_string(),
                app_address: "0x08020000".to_string(),
                boot_address: "0x08000000".to_string(),
                override_security: false,
                security_service: None,
                security_algo_or_cert: None,
                verify_method: None,
                flash_rid_overrides: get_default_rid_overrides(),
            },
        );
        products.insert(
            "KM4731".to_string(),
            ProductConfig {
                name: "KM4731".to_string(),
                txid: "0x18FF28FD".to_string(),
                rxid: "0x18FF1F11".to_string(),
                baudrate: "1000000".to_string(),
                app_address: "0x15020000".to_string(),
                boot_address: "0x08000000".to_string(),
                override_security: false,
                security_service: None,
                security_algo_or_cert: None,
                verify_method: None,
                flash_rid_overrides: get_default_rid_overrides(),
            },
        );

        // 默认配置中注入自定义产品项
        products.insert(
            CUSTOM_PRODUCT_ID.to_string(),
            ProductConfig {
                name: CUSTOM_PRODUCT_ID.to_string(),
                txid: "0x18FFFE68".to_string(),
                rxid: "0x18FF68FE".to_string(),
                baudrate: "1000000".to_string(),
                app_address: "0x15020000".to_string(),
                boot_address: "0x08000000".to_string(),
                override_security: false,
                security_service: None,
                security_algo_or_cert: None,
                verify_method: None,
                flash_rid_overrides: get_default_rid_overrides(),
            },
        );

        Self {
            language: "zh".to_string(),
            default_interface: "PCAN USB".to_string(),
            default_channel: "PCAN_USBBUS1".to_string(),
            default_baudrate: "250000".to_string(),
            default_product: "SPC-SDIO-S6".to_string(),
            default_flash_type: "app".to_string(),
            theme_mode: 0,
            default_filter_mode: 0,
            default_auto_reboot: true,
            security_service: "0x27".to_string(),
            verify_method: "crc32".to_string(),
            isotp_tx_padding: "0x55".to_string(),
            auth_0x29_cert_path: String::new(),
            products,
            dids: get_default_dids(),
            security_algos: HashMap::new(),
        }
    }
}

// 同时兼容 V1 和 V2 头部
const DAT_MAGIC_V1: &[u8] = b"UDS_CFG_DAT_V1\x00";
const DAT_MAGIC_V2: &[u8] = b"UDS_CFG_DAT_V2\x00";

pub fn load_config_dat() -> AppConfig {
    let mut need_create_default = false;

    if let Ok(mut file) = File::open("config.dat") {
        let mut buffer = Vec::new();
        if file.read_to_end(&mut buffer).is_ok() {
            let payload_opt = if buffer.starts_with(DAT_MAGIC_V2) {
                Some(&buffer[DAT_MAGIC_V2.len()..])
            } else if buffer.starts_with(DAT_MAGIC_V1) {
                Some(&buffer[DAT_MAGIC_V1.len()..])
            } else {
                None
            };

            if let Some(payload) = payload_opt {
                let mut decoder = ZlibDecoder::new(payload);
                let mut decoded_data = Vec::new();
                if decoder.read_to_end(&mut decoded_data).is_ok() {
                    if let Ok(mut cfg) = serde_json::from_slice::<AppConfig>(&decoded_data) {
                        if cfg.dids.is_empty() {
                            cfg.dids = get_default_dids();
                        }
                        if cfg.language.is_empty() {
                            cfg.language = "zh".to_string();
                        }
                        // 关键兼容补齐：对从已有 config.dat 中反序列化的数据强制注入自定义产品项
                        cfg.ensure_custom_product();
                        return cfg;
                    }
                }
            }
        }
    } else {
        need_create_default = true;
    }

    let default_cfg = AppConfig::default();
    if need_create_default {
        let _ = save_config_dat(&default_cfg);
    }
    default_cfg
}

pub fn save_config_dat(cfg: &AppConfig) -> bool {
    if let Ok(bytes) = serde_json::to_vec(cfg) {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        if encoder.write_all(&bytes).is_ok() {
            if let Ok(compressed) = encoder.finish() {
                if let Ok(mut file) = File::create("config.dat") {
                    let mut data = Vec::from(DAT_MAGIC_V2);
                    data.extend_from_slice(&compressed);
                    return file.write_all(&data).is_ok();
                }
            }
        }
    }
    false
}