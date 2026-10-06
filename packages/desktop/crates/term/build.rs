fn main() {
    let ghostty = "../../../../third_party/ghostty/zig-out";
    // Universal, so arm64 and x86_64 targets link the same file.
    let lib = format!("{ghostty}/lib/ghostty-vt.xcframework/macos-arm64_x86_64/libghostty-vt.a");
    let out = std::env::var("OUT_DIR").unwrap();
    // Copied alone into OUT_DIR: next to the .dylib the linker would pick the dylib.
    std::fs::copy(&lib, format!("{out}/libghostty-vt.a")).expect("libghostty-vt.a missing: run scripts/build-ghostty.sh");
    cc::Build::new()
        .file("src/shim.c")
        .include(format!("{ghostty}/include"))
        .compile("pocketshim");
    println!("cargo:rustc-link-search=native={out}");
    println!("cargo:rustc-link-lib=static=ghostty-vt");
    println!("cargo:rerun-if-changed=src/shim.c");
    println!("cargo:rerun-if-changed={lib}");
}
