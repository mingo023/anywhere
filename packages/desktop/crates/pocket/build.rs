fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        windows_icon();
    }
}

/// Embeds the app icon as resource 1, which gpui loads for the window and Explorer shows for the exe.
/// zig compiles it, being needed for a Windows build anyway (scripts/build-ghostty.sh).
fn windows_icon() {
    let ico = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../../assets/AppIcon.ico");
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    std::fs::write(out.join("app.rc"), format!("1 ICON \"{}\"\n", ico.display().to_string().replace('\\', "/"))).unwrap();
    let res = out.join("app.res");
    let status = std::process::Command::new("zig")
        .arg("rc")
        .arg(format!("/fo{}", res.display()))
        .arg(out.join("app.rc"))
        .status()
        .unwrap_or_else(|e| panic!("zig rc: {e}; install zig to build for Windows"));
    assert!(status.success(), "zig rc failed");
    println!("cargo:rustc-link-arg-bins={}", res.display());
    println!("cargo:rerun-if-changed={}", ico.display());
}
