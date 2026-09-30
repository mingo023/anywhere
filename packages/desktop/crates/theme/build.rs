use std::{env, fs, path::Path};

fn main() {
    println!("cargo:rerun-if-changed=assets/material");
    let mut names: Vec<String> = fs::read_dir("assets/material")
        .expect("assets/material exists")
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| n.ends_with(".svg"))
        .collect();
    names.sort();
    let entries: String = names
        .iter()
        .map(|n| format!("(\"icons/material/{n}\", include_bytes!(concat!(env!(\"CARGO_MANIFEST_DIR\"), \"/assets/material/{n}\"))),\n"))
        .collect();
    fs::write(Path::new(&env::var("OUT_DIR").unwrap()).join("material.rs"), format!("&[\n{entries}]")).unwrap();
}
