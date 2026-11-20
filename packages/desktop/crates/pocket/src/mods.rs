use crate::desktop::Desktop;
use gpui_kit::*;
use std::path::Path;
use theme::Theme;

pub(crate) fn look(home: &Path, event: &str, fields: &serde_json::Value) -> Option<Theme> {
    let fields = fields.to_string();
    ::mods::list(home).into_iter().fold(None, |look, m| {
        match ::mods::fire(&m.source, event, &fields).and_then(|looks| looks.iter().map(|l| Theme::parse(l)).collect::<Result<Vec<_>, _>>()) {
            Ok(set) => set.into_iter().last().or(look),
            Err(why) => {
                eprintln!("mod {}: {why}", m.id);
                look
            }
        }
    })
}

impl Desktop {
    pub(crate) fn mods_prompt(&mut self, text: &str, cx: &mut Context<Self>) {
        self.themes.prompts += 1;
        let (home, fields, run) = (self.store.home().to_path_buf(), serde_json::json!({ "text": text }), self.themes.prompts);
        cx.spawn(async move |this, cx| {
            let Some(set) = cx.background_spawn(async move { look(&home, "prompt", &fields) }).await else { return };
            this.update(cx, |d, cx| {
                if d.themes.prompt_look(run, set) {
                    d.apply_theme(cx);
                }
            })
            .ok();
        })
        .detach();
    }
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
        let painted = look(&home, "start", &serde_json::json!({})).unwrap();
        assert_eq!((painted.name.as_str(), painted.dark), ("Paper", false));
        std::fs::remove_dir_all(home).unwrap();
        let home = mods_home("last", &[("a", &paper), ("b", throws), ("c", not_a_theme), ("d", &ink)]);
        assert_eq!(look(&home, "start", &serde_json::json!({})).unwrap().name, "Ink");
        std::fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn no_mods_set_no_look() {
        let home = mods_home("none", &[]);
        assert_eq!(look(&home, "start", &serde_json::json!({})), None);
        std::fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn the_example_mod_paints_paper() {
        let home = mods_home("paper", &[("paper", include_str!("../../../mods/paper/mod.js"))]);
        let paper = look(&home, "start", &serde_json::json!({})).unwrap();
        assert_eq!((paper.name.as_str(), paper.dark, paper.color(theme::BUBBLE)), ("Paper", false, 0xfbf8f4ff));
        std::fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn the_koi_mod_turns_red_and_fills_the_pond_for_a_bug_fix() {
        let home = mods_home("koi", &[("koi", include_str!("../../../mods/koi/mod.js"))]);
        let calm = look(&home, "start", &serde_json::json!({})).unwrap().scene().cloned().unwrap();
        let bug = look(&home, "prompt", &serde_json::json!({ "text": "please fix the login bug on the settings page" })).unwrap().scene().cloned().unwrap();
        let koi = |scene: &theme::Scene| (scene.sprites[0].count, scene.sprites[0].speed, scene.sprites[0].cells[1][3]);
        assert_eq!(koi(&calm), (2, 30., Some(0xee8a3cff)));
        assert_eq!(koi(&bug), (5, 95., Some(0xd64545ff)));
        std::fs::remove_dir_all(home).unwrap();
    }
}
