use libloading::{Library, Symbol};
use std::path::Path;

pub type SecurityAlgoFn = unsafe extern "C" fn(
    level: i32,
    seed: *const u8,
    seed_len: i32,
    key_out: *mut u8,
    key_len: i32,
) -> i32;

pub struct DynamicSecurityAlgo {
    _lib: Library,
    algo_fn: SecurityAlgoFn,
}

impl DynamicSecurityAlgo {
    pub fn load<P: AsRef<Path>>(dll_path: P) -> Result<Self, String> {
        let path = dll_path.as_ref();
        if !path.exists() {
            return Err(format!("文件不存在: {}", path.display()));
        }

        unsafe {
            let lib = Library::new(path)
                .map_err(|e| format!("动态库加载失败: {}", e))?;

            let func: Symbol<SecurityAlgoFn> = lib
                .get(b"security_algo\0")
                .map_err(|e| format!("导出符号 security_algo 未找到: {}", e))?;

            Ok(Self {
                algo_fn: *func,
                _lib: lib,
            })
        }
    }

    pub fn calculate_key(&self, level: i32, seed: &[u8], expected_key_len: usize) -> Result<Vec<u8>, String> {
        let mut key_out = vec![0u8; expected_key_len.max(4)];

        let ret = unsafe {
            (self.algo_fn)(
                level,
                seed.as_ptr(),
                seed.len() as i32,
                key_out.as_mut_ptr(),
                key_out.len() as i32,
            )
        };

        if ret < 0 {
            return Err("动态库计算返回错误 (-1)".to_string());
        }

        let actual_len = (ret as usize).min(key_out.len());
        key_out.truncate(actual_len);
        Ok(key_out)
    }
}

/// 纯 Rust 内置 RC4 算法（完全对应 rc4.c 的逻辑）
pub fn fallback_rc4_algo(_level: i32, seed: &[u8], key_len: usize) -> Vec<u8> {
    let mut seed4 = [0u8; 4];
    let copy_len = seed.len().min(4);
    seed4[..copy_len].copy_from_slice(&seed[..copy_len]);

    let mut s = [0u8; 256];
    for i in 0..256 {
        s[i] = i as u8;
    }

    let mut j: usize = 0;
    for i in 0..256 {
        j = (j + s[i] as usize + seed4[i % 4] as usize) & 0xFF;
        s.swap(i, j);
    }

    let out_len = key_len.min(4);
    let mut key_out = vec![0u8; out_len];
    let mut ki: usize = 0;
    let mut kj: usize = 0;

    for k in 0..out_len {
        ki = (ki + 1) & 0xFF;
        kj = (kj + s[ki] as usize) & 0xFF;
        s.swap(ki, kj);
        key_out[k] = s[(s[ki] as usize + s[kj] as usize) & 0xFF];
    }

    key_out
}

/// 统一计算入口：优先尝试加载动态库，不存在/加载失败时回退内置 RC4
pub fn compute_key_with_fallback(
    dll_path: &str,
    level: i32,
    seed: &[u8],
    expected_key_len: usize,
) -> (Vec<u8>, Option<String>) {
    match DynamicSecurityAlgo::load(dll_path) {
        Ok(algo) => match algo.calculate_key(level, seed, expected_key_len) {
            Ok(k) => (k, None),
            Err(e) => {
                let warn = format!("动态库计算失败 ({})，回退内置 RC4 算法", e);
                (fallback_rc4_algo(level, seed, expected_key_len), Some(warn))
            }
        },
        Err(e) => {
            let warn = format!("无法加载算法库 [{}] ({})，已自动使用内置 RC4 算法兜底", dll_path, e);
            (fallback_rc4_algo(level, seed, expected_key_len), Some(warn))
        }
    }
}
