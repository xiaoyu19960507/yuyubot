fn main() {
    let version = env!("CARGO_PKG_VERSION");
    println!("cargo:rustc-env=APP_VERSION={}", version);

    let mut res = winres::WindowsResource::new();
    res.set_icon("res/favicon.ico");
    res.set("ProductName", "YuyuBot");
    res.set("FileDescription", "YuyuBot Bot 管理框架");
    res.set("InternalName", "yuyubot");
    res.set("OriginalFilename", "yuyubot.exe");
    res.set("CompanyName", "super1207");
    res.set("LegalCopyright", "Unlicense");
    res.compile().unwrap();
}
