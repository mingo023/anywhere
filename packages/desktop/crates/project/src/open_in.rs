use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// An app a folder can be opened in.
#[derive(Debug, PartialEq)]
pub struct Opener {
    /// What `desktop.json` remembers it by.
    pub id: &'static str,
    pub label: &'static str,
    /// Its `.app` names; JetBrains' differ by edition and installer.
    pub bundles: &'static [&'static str],
    /// The bundled icon it shows when it has no `.icns`.
    pub generic_icon: &'static str,
    /// The command-line flags that name its folder, when it ignores a folder handed to `open`.
    pub folder_flags: &'static [&'static str],
}

enum Slot {
    App(Opener),
    Editions { id: &'static str, label: &'static str, apps: &'static [Opener] },
}

struct CatalogueGroup {
    id: &'static str,
    label: &'static str,
    icon: &'static str,
    slots: &'static [Slot],
}

const fn ide(id: &'static str, label: &'static str, bundles: &'static [&'static str]) -> Opener {
    Opener { id, label, bundles, generic_icon: "external", folder_flags: &[] }
}

const fn terminal(id: &'static str, label: &'static str, bundles: &'static [&'static str], folder_flags: &'static [&'static str]) -> Opener {
    Opener { id, label, bundles, generic_icon: "terminal", folder_flags }
}

pub static FINDER: Opener = Opener { id: "finder", label: "Finder", bundles: &["Finder.app"], generic_icon: "folder", folder_flags: &[] };
const FINDER_BUNDLE: &str = "/System/Library/CoreServices/Finder.app";

const VS_CODE: &[Opener] = &[ide("vscode", "VS Code", &["Visual Studio Code.app"]), ide("vscode-insiders", "VS Code Insiders", &["Visual Studio Code - Insiders.app"])];

const JETBRAINS: &[Opener] = &[
    ide("intellij", "IntelliJ IDEA", &["IntelliJ IDEA.app", "IntelliJ IDEA Ultimate.app", "IntelliJ IDEA CE.app", "IntelliJ IDEA Community Edition.app"]),
    ide("webstorm", "WebStorm", &["WebStorm.app"]),
    ide("pycharm", "PyCharm", &["PyCharm.app", "PyCharm Professional Edition.app", "PyCharm CE.app", "PyCharm Community Edition.app"]),
    ide("goland", "GoLand", &["GoLand.app"]),
    ide("clion", "CLion", &["CLion.app"]),
    ide("rustrover", "RustRover", &["RustRover.app"]),
    ide("rider", "Rider", &["Rider.app"]),
    ide("phpstorm", "PhpStorm", &["PhpStorm.app"]),
    ide("rubymine", "RubyMine", &["RubyMine.app"]),
    ide("datagrip", "DataGrip", &["DataGrip.app"]),
    ide("android-studio", "Android Studio", &["Android Studio.app"]),
];

const IDES: &[Slot] = &[
    Slot::App(ide("cursor", "Cursor", &["Cursor.app"])),
    Slot::App(ide("antigravity", "Antigravity", &["Antigravity.app"])),
    Slot::App(ide("devin", "Devin", &["Devin.app"])),
    Slot::App(ide("zed", "Zed", &["Zed.app"])),
    Slot::App(ide("sublime", "Sublime Text", &["Sublime Text.app"])),
    Slot::App(ide("xcode", "Xcode", &["Xcode.app"])),
    Slot::Editions { id: "vscode-editions", label: "VS Code", apps: VS_CODE },
    Slot::Editions { id: "jetbrains", label: "JetBrains", apps: JETBRAINS },
];

const TERMINALS: &[Slot] = &[
    Slot::App(terminal("terminal", "Terminal", &["Terminal.app"], &[])),
    Slot::App(terminal("iterm", "iTerm", &["iTerm.app"], &[])),
    Slot::App(terminal("ghostty", "Ghostty", &["Ghostty.app"], &[])),
    Slot::App(terminal("warp", "Warp", &["Warp.app"], &[])),
    Slot::App(terminal("kitty", "kitty", &["kitty.app"], &["--directory"])),
    Slot::App(terminal("alacritty", "Alacritty", &["Alacritty.app"], &["--working-directory"])),
    Slot::App(terminal("wezterm", "WezTerm", &["WezTerm.app"], &["start", "--cwd"])),
];

static GROUPS: [CatalogueGroup; 2] = [CatalogueGroup { id: "ide", label: "IDE", icon: "code", slots: IDES }, CatalogueGroup { id: "terminal", label: "Terminal", icon: "terminal", slots: TERMINALS }];

fn apps() -> impl Iterator<Item = &'static Opener> {
    GROUPS.iter().flat_map(|group| group.slots.iter()).flat_map(|slot| match slot {
        Slot::App(app) => std::slice::from_ref(app),
        Slot::Editions { apps, .. } => *apps,
    })
}

/// Where the apps installed on this Mac are, by id.
pub type Installed = HashMap<&'static str, PathBuf>;

/// The folders apps are installed in; JetBrains Toolbox links its IDEs into its own.
pub fn search_roots(home: &Path) -> Vec<PathBuf> {
    vec![PathBuf::from("/Applications"), home.join("Applications"), home.join("Applications/JetBrains Toolbox"), PathBuf::from("/System/Applications/Utilities")]
}

/// The catalogue's apps found under `roots`, and Finder, which every Mac has.
pub fn installed(roots: &[PathBuf], exists: impl Fn(&Path) -> bool) -> Installed {
    let mut found: Installed = apps().filter_map(|app| Some((app.id, app.bundles.iter().flat_map(|name| roots.iter().map(move |root| root.join(name))).find(|path| exists(path))?))).collect();
    found.insert(FINDER.id, PathBuf::from(FINDER_BUNDLE));
    found
}

#[derive(Debug, PartialEq)]
pub enum Entry {
    App(&'static Opener),
    Editions { id: &'static str, label: &'static str, apps: Vec<&'static Opener> },
}

#[derive(Debug, PartialEq)]
pub struct Group {
    pub id: &'static str,
    pub label: &'static str,
    pub icon: &'static str,
    pub entries: Vec<Entry>,
}

/// The menu's groups holding only what is installed; empty groups and edition lists are left out.
pub fn menu(installed: &Installed) -> Vec<Group> {
    let have = |app: &Opener| installed.contains_key(app.id);
    GROUPS
        .iter()
        .filter_map(|group| {
            let entries: Vec<Entry> = group
                .slots
                .iter()
                .filter_map(|slot| match slot {
                    Slot::App(app) => have(app).then_some(Entry::App(app)),
                    Slot::Editions { id, label, apps } => {
                        let apps: Vec<&'static Opener> = apps.iter().filter(|app| have(app)).collect();
                        (!apps.is_empty()).then_some(Entry::Editions { id: *id, label: *label, apps })
                    }
                })
                .collect();
            (!entries.is_empty()).then_some(Group { id: group.id, label: group.label, icon: group.icon, entries })
        })
        .collect()
}

/// The app the button opens: the remembered one while it's installed, else Finder.
pub fn selected(remembered: &str, installed: &Installed) -> &'static Opener {
    std::iter::once(&FINDER).chain(apps()).find(|app| app.id == remembered && installed.contains_key(app.id)).unwrap_or(&FINDER)
}

/// A program and its arguments.
#[derive(Debug, PartialEq)]
pub struct Cmd {
    pub program: &'static str,
    pub args: Vec<String>,
}

fn text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// Opens `folder` in `app`, installed at `bundle`. `-n` because a running app never sees `--args`.
pub fn launch(app: &Opener, bundle: &Path, folder: &str) -> Cmd {
    let args = if app.folder_flags.is_empty() {
        vec!["-a".into(), text(bundle), folder.into()]
    } else {
        ["-n".to_string(), "-a".into(), text(bundle), "--args".into()].into_iter().chain(app.folder_flags.iter().map(|flag| flag.to_string())).chain([folder.to_string()]).collect()
    };
    Cmd { program: "/usr/bin/open", args }
}

/// Reads the app's `CFBundleIconFile`.
pub fn icon_name(bundle: &Path) -> Cmd {
    Cmd { program: "/usr/bin/plutil", args: vec!["-extract".into(), "CFBundleIconFile".into(), "raw".into(), "-o".into(), "-".into(), text(&bundle.join("Contents/Info.plist"))] }
}

/// The `.icns` a `CFBundleIconFile` value names; the value may leave off the extension.
pub fn icns_path(bundle: &Path, name: &str) -> Option<PathBuf> {
    let name = name.trim();
    let file = if name.ends_with(".icns") { name.to_string() } else { format!("{name}.icns") };
    (!name.is_empty()).then(|| bundle.join("Contents/Resources").join(file))
}

/// Writes `icns` as a 64px PNG at `png`.
pub fn png_icon(icns: &Path, png: &Path) -> Cmd {
    Cmd { program: "/usr/bin/sips", args: vec!["-s".into(), "format".into(), "png".into(), "-Z".into(), "64".into(), text(icns), "--out".into(), text(png)] }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn with(paths: &[&str]) -> Installed {
        let paths: HashSet<PathBuf> = paths.iter().map(|p| PathBuf::from(*p)).collect();
        installed(&search_roots(Path::new("/Users/me")), |p| paths.contains(p))
    }

    fn ids(entries: &[Entry]) -> Vec<&'static str> {
        entries
            .iter()
            .map(|e| match e {
                Entry::App(app) => app.id,
                Entry::Editions { id, .. } => *id,
            })
            .collect()
    }

    #[test]
    fn no_two_apps_share_an_id() {
        let all: Vec<_> = apps().map(|a| a.id).chain([FINDER.id]).collect();
        let unique: HashSet<_> = all.iter().collect();
        assert_eq!(unique.len(), all.len());
    }

    #[test]
    fn finder_is_always_installed_and_the_rest_only_when_found() {
        let found = with(&["/Applications/Cursor.app"]);
        assert_eq!(found.len(), 2);
        assert_eq!(found["cursor"], Path::new("/Applications/Cursor.app"));
        assert_eq!(found["finder"], Path::new(FINDER_BUNDLE));
    }

    #[test]
    fn apps_are_found_in_each_root_and_under_jetbrains_toolbox() {
        let found = with(&["/Users/me/Applications/Zed.app", "/Users/me/Applications/JetBrains Toolbox/GoLand.app", "/System/Applications/Utilities/Terminal.app"]);
        assert_eq!(found["zed"], Path::new("/Users/me/Applications/Zed.app"));
        assert_eq!(found["goland"], Path::new("/Users/me/Applications/JetBrains Toolbox/GoLand.app"));
        assert_eq!(found["terminal"], Path::new("/System/Applications/Utilities/Terminal.app"));
    }

    #[test]
    fn an_edition_is_found_under_any_of_its_bundle_names() {
        assert_eq!(with(&["/Applications/IntelliJ IDEA CE.app"])["intellij"], Path::new("/Applications/IntelliJ IDEA CE.app"));
    }

    #[test]
    fn empty_groups_and_edition_lists_vanish() {
        assert!(menu(&with(&[])).is_empty());
        let groups = menu(&with(&["/Applications/Cursor.app"]));
        assert_eq!(groups.len(), 1);
        assert_eq!((groups[0].id, ids(&groups[0].entries)), ("ide", vec!["cursor"]));
    }

    #[test]
    fn a_lone_edition_stays_in_its_submenu() {
        let groups = menu(&with(&["/Applications/Visual Studio Code - Insiders.app"]));
        let [Entry::Editions { label: "VS Code", apps, .. }] = groups[0].entries.as_slice() else { panic!("{groups:?}") };
        assert_eq!(apps.iter().map(|a| a.id).collect::<Vec<_>>(), ["vscode-insiders"]);
    }

    #[test]
    fn the_terminal_group_lists_terminals() {
        let groups = menu(&with(&["/System/Applications/Utilities/Terminal.app", "/Applications/iTerm.app"]));
        assert_eq!((groups[0].id, ids(&groups[0].entries)), ("terminal", vec!["terminal", "iterm"]));
    }

    #[test]
    fn the_remembered_app_is_used_while_installed_else_finder() {
        let found = with(&["/Applications/Zed.app"]);
        assert_eq!(selected("zed", &found).id, "zed");
        assert_eq!(selected("cursor", &found), &FINDER);
        assert_eq!(selected("", &found), &FINDER);
        assert_eq!(selected("nonsense", &found), &FINDER);
    }

    #[test]
    fn launching_opens_the_folder_with_the_bundle() {
        let zed = ide("zed", "Zed", &["Zed.app"]);
        assert_eq!(launch(&zed, Path::new("/Applications/Zed.app"), "/w/app"), Cmd { program: "/usr/bin/open", args: vec!["-a".into(), "/Applications/Zed.app".into(), "/w/app".into()] });
    }

    #[test]
    fn a_terminal_that_ignores_a_folder_gets_it_on_its_command_line() {
        let wezterm = apps().find(|a| a.id == "wezterm").unwrap();
        assert_eq!(launch(wezterm, Path::new("/Applications/WezTerm.app"), "/w/app").args, ["-n", "-a", "/Applications/WezTerm.app", "--args", "start", "--cwd", "/w/app"]);
    }

    #[test]
    fn the_icon_name_is_read_from_the_bundles_plist() {
        assert_eq!(icon_name(Path::new("/Applications/Zed.app")).args, ["-extract", "CFBundleIconFile", "raw", "-o", "-", "/Applications/Zed.app/Contents/Info.plist"]);
    }

    #[test]
    fn a_bare_icon_name_gets_its_extension_and_a_blank_one_has_no_path() {
        let bundle = Path::new("/A.app");
        assert_eq!(icns_path(bundle, "AppIcon\n"), Some(PathBuf::from("/A.app/Contents/Resources/AppIcon.icns")));
        assert_eq!(icns_path(bundle, "Icon.icns"), Some(PathBuf::from("/A.app/Contents/Resources/Icon.icns")));
        assert_eq!(icns_path(bundle, "  "), None);
    }

    #[test]
    fn the_png_is_made_by_sips_at_64_points() {
        let cmd = png_icon(Path::new("/a.icns"), Path::new("/c/a.png"));
        assert_eq!((cmd.program, cmd.args), ("/usr/bin/sips", vec!["-s".to_string(), "format".into(), "png".into(), "-Z".into(), "64".into(), "/a.icns".into(), "--out".into(), "/c/a.png".into()]));
    }
}
