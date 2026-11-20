use std::path::Path;
use theme::Theme;

pub(crate) fn look(home: &Path) -> Option<Theme> {
    ::mods::list(home).into_iter().fold(None, |look, m| {
        match ::mods::fire(&m.source, "start").and_then(|looks| looks.iter().map(|l| Theme::parse(l)).collect::<Result<Vec<_>, _>>()) {
            Ok(set) => set.into_iter().last().or(look),
            Err(why) => {
                eprintln!("mod {}: {why}", m.id);
                look
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::look;

    fn mods_home(name: &str, mods: &[(&str, &str)]) -> std::path::PathBuf {
        let home = std::env::temp_dir().join(format!("pocket-mods-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        for (id, source) in mods {
            std::fs::create_dir_all(home.join("mods").join(id)).unwrap();
            std::fs::write(home.join("mods").join(id).join("mod.js"), source).unwrap();
        }
        home
    }

    fn sets(name: &str) -> String {
        format!("export function register(on) {{ on('start', $ => $.theme.set({{ name: '{name}', appearance: 'light', backdrop: '#102030' }})) }}")
    }

    #[test]
    fn the_last_look_a_mod_sets_wins_and_a_broken_mod_changes_nothing() {
        let (paper, ink) = (sets("Paper"), sets("Ink"));
        let throws = "export function register(on) { on('start', () => { throw new Error('x') }) }";
        let not_a_theme = "export function register(on) { on('start', $ => $.theme.set({ name: 'No appearance' })) }";
        let home = mods_home("broken", &[("a", &paper), ("b", throws), ("c", not_a_theme)]);
        let painted = look(&home).unwrap();
        assert_eq!((painted.name.as_str(), painted.dark), ("Paper", false));
        std::fs::remove_dir_all(home).unwrap();
        let home = mods_home("last", &[("a", &paper), ("b", throws), ("c", not_a_theme), ("d", &ink)]);
        assert_eq!(look(&home).unwrap().name, "Ink");
        std::fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn no_mods_set_no_look() {
        let home = mods_home("none", &[]);
        assert_eq!(look(&home), None);
        std::fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn the_example_mod_paints_paper() {
        let home = mods_home("paper", &[("paper", include_str!("../../../mods/paper/mod.js"))]);
        let paper = look(&home).unwrap();
        assert_eq!((paper.name.as_str(), paper.dark, paper.color(theme::BUBBLE)), ("Paper", false, 0xfbf8f4ff));
        std::fs::remove_dir_all(home).unwrap();
    }
}
