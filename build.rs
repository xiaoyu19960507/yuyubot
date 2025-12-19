fn main() {
    // 读取 Cargo.toml 中的版本号
    let version = env!("CARGO_PKG_VERSION");
    println!("cargo:rustc-env=APP_VERSION={}", version);

    let mut res = winres::WindowsResource::new();
    res.set_icon("res/favicon.ico");
    res.compile().unwrap();
}
