//! Embed the app icon in the Windows executable.

fn main() {
    println!("cargo:rerun-if-changed=../../assets/cryptdeck.ico");

    // Only with the MSVC toolchain, which ships the resource compiler.
    // Release builds use it; cross-checks from Linux (windows-gnu) skip this.
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_os == "windows" && target_env == "msvc" {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../assets/cryptdeck.ico");
        res.compile().expect("failed to embed the Windows icon");
    }
}
