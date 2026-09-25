fn main() {
    let ghostty = "../../third_party/ghostty/zig-out";
    let out = std::env::var("OUT_DIR").unwrap();
    // Copied alone into OUT_DIR: next to the .dylib the linker would pick the dylib.
    std::fs::copy(format!("{ghostty}/lib/libghostty-vt.a"), format!("{out}/libghostty-vt.a"))
        .expect("libghostty-vt.a missing: run scripts/build-ghostty.sh");
    cc::Build::new()
        .file("src/shim.c")
        .include(format!("{ghostty}/include"))
        .compile("pocketshim");
    println!("cargo:rustc-link-search=native={out}");
    println!("cargo:rustc-link-lib=static=ghostty-vt");
    println!("cargo:rerun-if-changed=src/shim.c");
    println!("cargo:rerun-if-changed={ghostty}/lib/libghostty-vt.a");
}
