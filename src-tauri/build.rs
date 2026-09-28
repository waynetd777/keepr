fn main() {
    // SMAppService (src/login_item.rs) lives in ServiceManagement and SMB mounting (src/smb.rs)
    // in NetFS; without these the classes and symbols are missing at run time.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-link-lib=framework=ServiceManagement");
        println!("cargo:rustc-link-lib=framework=NetFS");
    }
    tauri_build::build()
}
