fn main() {
    println!("cargo:rerun-if-changed=assets/tonal.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("assets/tonal.ico")
            .set("ProductName", "Tonal")
            .set("FileDescription", "Tonal — Music")
            .set("ProductVersion", env!("CARGO_PKG_VERSION"))
            .set("FileVersion", env!("CARGO_PKG_VERSION"))
            .compile()
            .expect("Compile the Windows application icon");
    }
}
