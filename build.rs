// build.rs
fn main() {
    // 告诉 cargo 只要 udstools.ico 发生变化就重新执行构建
    println!("cargo:rerun-if-changed=udstools.ico");

    #[cfg(target_os = "windows")]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("udstools.ico");
        // 显式指定语言代码 0x0804 (简体中文) 或 0x0409 (美式英语)，防止 MSVC rc.exe 编译丢失资源项
        res.set_language(0x0804);
        if let Err(e) = res.compile() {
            eprintln!("winres 编译 Windows 资源失败: {}", e);
        }
    }
}