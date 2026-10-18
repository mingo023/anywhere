fn main() {
    let third_party = "../../../../third_party/ghostty";
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap();
    let arch = match std::env::var("CARGO_CFG_TARGET_ARCH").unwrap().as_str() {
        "aarch64" => "arm64",
        _ => "amd64",
    };
    // As scripts/build-ghostty.sh lays them out; macOS's is universal, so arm64 and x86_64 targets link the same file.
    let (ghostty, lib) = match os.as_str() {
        "macos" => (format!("{third_party}/zig-out"), "lib/ghostty-vt.xcframework/macos-arm64_x86_64/libghostty-vt.a"),
        "windows" => (format!("{third_party}/zig-out-windows-{arch}"), "lib/ghostty-vt-static.lib"),
        _ => (format!("{third_party}/zig-out-{os}-{arch}"), "lib/libghostty-vt.a"),
    };
    let lib = format!("{ghostty}/{lib}");
    let out = std::env::var("OUT_DIR").unwrap();
    // Copied alone into OUT_DIR: next to the .dylib the linker would pick the dylib.
    std::fs::copy(&lib, format!("{out}/libghostty-vt.a"))
        .unwrap_or_else(|e| panic!("{lib}: {e}; run scripts/build-ghostty.sh for this target"));
    cc::Build::new()
        .file("src/shim.c")
        .include(format!("{ghostty}/include"))
        // Linked statically; without it Windows headers declare every function dllimport.
        .define("GHOSTTY_STATIC", None)
        .compile("pocketshim");
    println!("cargo:rustc-link-search=native={out}");
    println!("cargo:rustc-link-lib=static=ghostty-vt");
    println!("cargo:rerun-if-changed=src/shim.c");
    println!("cargo:rerun-if-changed={lib}");
}
