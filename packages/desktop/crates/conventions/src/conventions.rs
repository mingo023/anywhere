#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    const DRAWING_CRATES: [&str; 4] = ["pocket", "ui", "theme", "storybook"];
    const DESKTOP_FIELD_CEILING: usize = 74;

    fn root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn crates() -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(root().join("crates")).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
        names.sort();
        names
    }

    fn manifest(name: &str) -> String {
        fs::read_to_string(root().join("crates").join(name).join("Cargo.toml")).unwrap()
    }

    fn dependencies(manifest: &str) -> Vec<String> {
        manifest
            .split("\n[")
            .filter(|table| {
                let header = table.lines().next().unwrap_or_default();
                header.ends_with("dependencies]") && !header.ends_with("dev-dependencies]")
            })
            .flat_map(|table| table.lines().skip(1))
            .filter_map(|l| l.split(['.', ' ', '=']).next())
            .filter(|n| !n.is_empty() && !n.starts_with('#'))
            .map(str::to_string)
            .collect()
    }

    fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                rust_files(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
    }

    #[test]
    fn model_crates_never_depend_on_a_crate_that_draws() {
        let mut broken = Vec::new();
        for name in crates().iter().filter(|n| !DRAWING_CRATES.contains(&n.as_str())) {
            for dep in dependencies(&manifest(name)).into_iter().filter(|d| DRAWING_CRATES.contains(&d.as_str())) {
                broken.push(format!("{name} -> {dep}"));
            }
        }
        assert!(
            broken.is_empty(),
            "model crates depend on view crates: {broken:?}. Move the code that needs the view into `pocket`, or the logic it shares into the model crate (ADR 0003)."
        );
    }

    #[test]
    fn every_crate_takes_the_workspace_lints() {
        let broken: Vec<String> = crates().into_iter().filter(|n| !manifest(n).contains("\n[lints]\nworkspace = true")).collect();
        assert!(broken.is_empty(), "crates without the workspace lints: {broken:?}. Add `[lints]` with `workspace = true` to each Cargo.toml.");
    }

    #[test]
    fn a_crate_lib_is_named_after_the_crate() {
        let broken: Vec<String> = crates()
            .into_iter()
            .filter(|name| {
                let src = root().join("crates").join(name).join("src");
                src.join("lib.rs").exists() || (!src.join("main.rs").exists() && !src.join(format!("{name}.rs")).exists())
            })
            .collect();
        assert!(broken.is_empty(), "crates without `src/<crate>.rs`: {broken:?}. Rename `lib.rs` and set `[lib] path` in Cargo.toml.");
    }

    #[test]
    fn no_module_is_a_mod_rs() {
        let mut files = Vec::new();
        rust_files(&root().join("crates"), &mut files);
        let broken: Vec<_> = files.iter().filter(|p| p.file_name().is_some_and(|f| f == "mod.rs")).collect();
        assert!(broken.is_empty(), "{broken:?}: a module with children is `foo.rs` plus `foo/`. Move each `foo/mod.rs` to `foo.rs`.");
    }

    #[test]
    fn the_layout_adr_maps_every_crate() {
        let adr = fs::read_to_string(root().join("../../docs/adr/0003-desktop-code-layout.md")).unwrap();
        let missing: Vec<String> = crates().into_iter().filter(|n| !adr.contains(&format!("`{n}`"))).collect();
        assert!(missing.is_empty(), "crates missing from the map in docs/adr/0003-desktop-code-layout.md: {missing:?}. Add a row saying what each owns.");
    }

    #[test]
    fn desktop_gets_no_new_fields() {
        let source = fs::read_to_string(root().join("crates/pocket/src/desktop.rs")).unwrap();
        let body = source.split("pub struct Desktop {").nth(1).unwrap().split("\n}").next().unwrap();
        let fields = body.lines().map(str::trim_start).filter(|l| !l.starts_with("//") && l.contains(": ")).count();
        assert!(
            fields <= DESKTOP_FIELD_CEILING,
            "`Desktop` has {fields} fields, over the {DESKTOP_FIELD_CEILING} allowed. Give the feature its own state struct in its module and compose that (ADR 0003)."
        );
        assert!(fields == DESKTOP_FIELD_CEILING, "`Desktop` is down to {fields} fields. Lower `DESKTOP_FIELD_CEILING` to {fields}; never raise it.");
    }

    #[test]
    fn dependencies_reads_every_table_but_dev_dependencies() {
        let manifest = "[package]\nname = \"x\"\n\n[dependencies]\nui.workspace = true\ngit = { path = \"g\" }\n\n[dev-dependencies]\ntheme.workspace = true\n\n[target.'cfg(windows)'.dependencies]\npocket.workspace = true\n";
        assert_eq!(dependencies(manifest), ["ui", "git", "pocket"]);
    }
}
