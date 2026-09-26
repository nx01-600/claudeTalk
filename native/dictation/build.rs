// Embeds the app icon (tray, Start Menu shortcut, Explorer).
fn main() {
    println!("cargo:rerun-if-changed=../../assets/claudetalk.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../assets/claudetalk.ico");
        res.set("FileDescription", "claudeTalk voice dictation");
        res.set("ProductName", "claudeTalk");
        if let Err(e) = res.compile() {
            println!("cargo:warning=icon not embedded: {e}");
        }
    }
}
