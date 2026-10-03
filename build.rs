// Embed the icon and version info in the Windows executable.
fn main() {
    println!("cargo:rerun-if-changed=assets/rustsheet.ico");
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/rustsheet.ico")
            .set("ProductName", "RustSheet")
            .set("FileDescription", "RustSheet")
            .set(
                "LegalCopyright",
                "Copyright (c) 2026 Divhanthelion. MIT License.",
            );
        if let Err(e) = res.compile() {
            println!("cargo:warning=could not embed Windows resources: {e}");
        }
    }
}
