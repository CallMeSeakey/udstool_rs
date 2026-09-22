use libloading::{Library, Symbol};

pub type SecurityAlgoFn = unsafe extern "C" fn(i32, *const u8, i32, *mut u8, i32) -> i32;

pub struct SecurityManager {
    _lib: Option<Library>,
    algo_fn: Option<SecurityAlgoFn>,
}

impl SecurityManager {
    pub fn load(path: &str) -> Self {
        unsafe {
            if let Ok(lib) = Library::new(path) {
                let func: Result<Symbol<SecurityAlgoFn>, _> = lib.get(b"security_algo");
                if let Ok(sym) = func {
                    return Self {
                        algo_fn: Some(*sym),
                        _lib: Some(lib),
                    };
                }
            }
        }
        Self {
            _lib: None,
            algo_fn: None,
        }
    }

    pub fn compute_key(&self, level: i32, seed: &[u8]) -> Vec<u8> {
        if let Some(func) = self.algo_fn {
            let mut out = vec![0u8; 64];
            unsafe {
                let len = func(
                    level,
                    seed.as_ptr(),
                    seed.len() as i32,
                    out.as_mut_ptr(),
                    out.len() as i32,
                );
                if len > 0 {
                    out.truncate(len as usize);
                    return out;
                }
            }
        }
        // 内置 RC4 默认备用算法
        Self::builtin_rc4(seed)
    }

    fn builtin_rc4(seed: &[u8]) -> Vec<u8> {
        let mut key = vec![0u8; seed.len().max(4)];
        let mut s: [u8; 256] = [0; 256];
        for i in 0..256 {
            s[i] = i as u8;
        }
        let mut j: usize = 0;
        for i in 0..256 {
            j = (j + s[i] as usize + seed[i % seed.len()] as usize) % 256;
            s.swap(i, j);
        }
        let (mut i, mut j) = (0usize, 0usize);
        for byte in key.iter_mut() {
            i = (i + 1) % 256;
            j = (j + s[i] as usize) % 256;
            s.swap(i, j);
            *byte ^= s[(s[i] as usize + s[j] as usize) % 256];
        }
        key
    }
}
