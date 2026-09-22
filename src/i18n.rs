// src/i18n.rs
#![allow(dead_code)]
use std::collections::HashMap;
use std::fs;
use std::sync::RwLock;

// 编译期内嵌默认语言包作为双重保险
const BUILTIN_ZH: &str = include_str!("../locales/zh.json");
const BUILTIN_EN: &str = include_str!("../locales/en.json");

lazy_static::lazy_static! {
    static ref CURRENT_LANG: RwLock<String> = RwLock::new("zh".to_string());
    static ref TRANSLATIONS: RwLock<HashMap<String, HashMap<String, String>>> = RwLock::new(HashMap::new());
}

pub fn load_locales() {
    let mut map = TRANSLATIONS.write().unwrap();

    // 加载中文：先读磁盘，失败或解析错误则使用内置数据
    let mut zh_loaded = false;
    if let Ok(content) = fs::read_to_string("locales/zh.json") {
        if let Ok(json) = serde_json::from_str::<HashMap<String, String>>(&content) {
            map.insert("zh".to_string(), json);
            zh_loaded = true;
        }
    }
    if !zh_loaded {
        if let Ok(json) = serde_json::from_str::<HashMap<String, String>>(BUILTIN_ZH) {
            map.insert("zh".to_string(), json);
        }
    }

    // 加载英文：先读磁盘，失败或解析错误则使用内置数据
    let mut en_loaded = false;
    if let Ok(content) = fs::read_to_string("locales/en.json") {
        if let Ok(json) = serde_json::from_str::<HashMap<String, String>>(&content) {
            map.insert("en".to_string(), json);
            en_loaded = true;
        }
    }
    if !en_loaded {
        if let Ok(json) = serde_json::from_str::<HashMap<String, String>>(BUILTIN_EN) {
            map.insert("en".to_string(), json);
        }
    }
}

pub fn set_language(lang: &str) {
    let mut current = CURRENT_LANG.write().unwrap();
    *current = lang.to_string();
}

pub fn get_language() -> String {
    CURRENT_LANG.read().unwrap().clone()
}

pub fn t(key: &str) -> String {
    let lang = get_language();
    let trans = TRANSLATIONS.read().unwrap();

    // 1. 优先在当前语言中寻找
    if let Some(map) = trans.get(&lang) {
        if let Some(val) = map.get(key) {
            return val.clone();
        }
    }

    // 2. 当前非中文但未找到时，回退到中文
    if lang != "zh" {
        if let Some(map) = trans.get("zh") {
            if let Some(val) = map.get(key) {
                return val.clone();
            }
        }
    }

    // 3. 中文也没找到时，回退到英文
    if lang != "en" {
        if let Some(map) = trans.get("en") {
            if let Some(val) = map.get(key) {
                return val.clone();
            }
        }
    }

    key.to_string()
}

pub fn t_fmt(key: &str, args: &[(&str, &str)]) -> String {
    let mut text = t(key);
    for (k, v) in args {
        text = text.replace(&format!("{{{}}}", k), v);
    }
    text
}

pub fn sync_did_i18n(key: &str, zh_val: &str, en_val: &str) {
    let mut trans = TRANSLATIONS.write().unwrap();

    if let Some(zh_map) = trans.get_mut("zh") {
        zh_map.insert(key.to_string(), zh_val.to_string());
    }
    if let Some(en_map) = trans.get_mut("en") {
        en_map.insert(key.to_string(), en_val.to_string());
    }

    let _ = fs::create_dir_all("locales");
    for (lang, val) in &[("zh", zh_val), ("en", en_val)] {
        let path = format!("locales/{}.json", lang);
        let mut current_map = if let Ok(content) = fs::read_to_string(&path) {
            serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&content).unwrap_or_default()
        } else {
            serde_json::Map::new()
        };
        current_map.insert(key.to_string(), serde_json::Value::String(val.to_string()));
        if let Ok(new_json) = serde_json::to_string_pretty(&current_map) {
            let _ = fs::write(&path, new_json);
        }
    }
}

pub fn remove_did_i18n(key: &str) {
    let mut trans = TRANSLATIONS.write().unwrap();

    if let Some(zh_map) = trans.get_mut("zh") {
        zh_map.remove(key);
    }
    if let Some(en_map) = trans.get_mut("en") {
        en_map.remove(key);
    }

    for lang in &["zh", "en"] {
        let path = format!("locales/{}.json", lang);
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(mut map) = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&content) {
                map.remove(key);
                if let Ok(new_json) = serde_json::to_string_pretty(&map) {
                    let _ = fs::write(&path, new_json);
                }
            }
        }
    }
}
