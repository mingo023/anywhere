//! Mods: JavaScript that hooks the app's events, kept in `<home>/mods/<id>/mod.js` and run in QuickJS.
//!
//! A mod exports `register(on)`. Each `on(event, hook)` adds a hook, called as `hook($, e)`, where `$` is what the mod
//! can ask of the app: `$.theme.set(look)`, a look being a theme file's fields as an object.

use rquickjs::{CatchResultExt, Context, Function, Module, Runtime};
use std::path::Path;
use std::time::{Duration, Instant};

const TIME_LIMIT: Duration = Duration::from_millis(250);
const MEMORY_LIMIT: usize = 16 << 20;

const PRELUDE: &str = r#"
globalThis.__fire = (register, event) => {
    const hooks = [];
    register((name, hook) => {
        if (typeof hook !== 'function') throw new TypeError(`on('${name}', …) needs a function`);
        hooks.push([name, hook]);
    });
    const looks = [];
    const $ = { theme: { set(look) { looks.push(JSON.stringify(look)); } } };
    for (const [name, hook] of hooks) if (name === event) hook($, { event });
    return looks;
};
"#;

#[derive(Clone, Debug, PartialEq)]
pub struct Mod {
    pub id: String,
    pub source: String,
}

/// Every mod in `home`, in id order; a folder without a readable `mod.js` is left out.
pub fn list(home: &Path) -> Vec<Mod> {
    let Ok(entries) = std::fs::read_dir(home.join("mods")) else { return Vec::new() };
    let mut mods: Vec<Mod> = entries
        .flatten()
        .filter_map(|entry| Some(Mod { id: entry.file_name().to_str()?.to_string(), source: std::fs::read_to_string(entry.path().join("mod.js")).ok()? }))
        .collect();
    mods.sort_by(|a, b| a.id.cmp(&b.id));
    mods
}

/// Runs the hooks `source` adds for `event` in a fresh engine, and returns the looks they set, as JSON, in order.
/// The error says why the mod failed: it threw, it doesn't export `register`, or it ran past its time or memory.
pub fn fire(source: &str, event: &str) -> Result<Vec<String>, String> {
    let runtime = Runtime::new().map_err(|e| e.to_string())?;
    runtime.set_memory_limit(MEMORY_LIMIT);
    let deadline = Instant::now() + TIME_LIMIT;
    runtime.set_interrupt_handler(Some(Box::new(move || Instant::now() > deadline)));
    let context = Context::full(&runtime).map_err(|e| e.to_string())?;
    context.with(|cx| {
        let run = || -> rquickjs::Result<Vec<String>> {
            cx.eval::<(), _>(PRELUDE)?;
            let (module, done) = Module::declare(cx.clone(), "mod.js", source)?.eval()?;
            done.finish::<()>()?;
            let register: Function = module.get("register")?;
            let fire: Function = cx.globals().get("__fire")?;
            fire.call((register, event))
        };
        run().catch(&cx).map_err(|e| e.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::{Mod, fire, list};

    #[test]
    fn a_start_hook_sets_the_looks_it_asks_for_in_order() {
        let source = r#"export function register(on) {
            on('start', $ => { $.theme.set({ name: 'A' }); $.theme.set({ name: 'B', accent: '#ff0000' }); });
            on('other', $ => $.theme.set({ name: 'C' }));
        }"#;
        assert_eq!(fire(source, "start").unwrap(), [r#"{"name":"A"}"#, r##"{"name":"B","accent":"#ff0000"}"##]);
        assert_eq!(fire(source, "nothing").unwrap(), Vec::<String>::new());
    }

    #[test]
    fn a_mod_computes_its_look_in_code() {
        let source = "export const register = on => on('start', $ => $.theme.set({ name: ['a', 'b'].map(s => s.toUpperCase()).join('') }));";
        assert_eq!(fire(source, "start").unwrap(), [r#"{"name":"AB"}"#]);
    }

    #[test]
    fn a_mod_that_fails_says_why() {
        let thrown = fire("export function register(on) { on('start', () => { throw new Error('no paper') }) }", "start").unwrap_err();
        assert!(thrown.contains("no paper"), "{thrown}");
        assert!(fire("export const x = 1;", "start").is_err());
        assert!(fire("export function register(on) { on('start', 3) }", "start").unwrap_err().contains("needs a function"));
        assert!(fire("export function register( {", "start").is_err());
    }

    #[test]
    fn a_mod_that_runs_forever_or_hogs_memory_is_stopped() {
        let started = std::time::Instant::now();
        assert!(fire("export function register(on) { on('start', () => { while (true) {} }) }", "start").is_err());
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
        assert!(fire("export function register(on) { on('start', () => { 'x'.repeat(64 << 20) }) }", "start").is_err());
    }

    #[test]
    fn mods_are_listed_by_id_and_a_folder_without_mod_js_is_skipped() {
        let home = std::env::temp_dir().join(format!("pocket-mods-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        assert_eq!(list(&home), []);
        for (id, file) in [("zebra", Some("z")), ("apple", Some("a")), ("empty", None)] {
            let dir = home.join("mods").join(id);
            std::fs::create_dir_all(&dir).unwrap();
            if let Some(source) = file {
                std::fs::write(dir.join("mod.js"), source).unwrap();
            }
        }
        let ids: Vec<Mod> = list(&home);
        assert_eq!(ids.iter().map(|m| (m.id.as_str(), m.source.as_str())).collect::<Vec<_>>(), [("apple", "a"), ("zebra", "z")]);
        std::fs::remove_dir_all(home).unwrap();
    }
}
