use gpui_kit::component::input::InputState;
use gpui_kit::*;
use theme::*;

pub(crate) fn default_base<'a>(branches: impl Iterator<Item = &'a str> + Clone, preferred: &str, current: &str) -> usize {
    let at = |name: &str| branches.clone().position(|b| b == name);
    at(preferred).or_else(|| at("main")).or_else(|| at("master")).or_else(|| at(current)).unwrap_or(0)
}

pub(crate) fn home() -> String {
    std::env::home_dir().map(|h| h.to_string_lossy().into_owned()).unwrap_or_default()
}

static SHARED: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// The home every channel on this Mac shares, holding its worktrees and Scratch folders. Read once until `reshare`,
/// since settings render it every frame.
pub(crate) fn shared() -> String {
    let mut shared = SHARED.lock().unwrap_or_else(|e| e.into_inner());
    shared.get_or_insert_with(|| daemon::home::shared_home(home().as_ref()).to_string_lossy().into_owned()).clone()
}

/// Reads the shared home again, after pocketd moved it.
pub(crate) fn reshare() {
    *SHARED.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

pub(crate) fn typed_or(input: &Entity<InputState>, fallback: impl FnOnce() -> String, cx: &App) -> String {
    let v = input.read(cx).value().trim().to_string();
    if v.is_empty() { fallback() } else { v }
}

pub fn footer(note: impl IntoElement, cancel: Stateful<Div>, submit: Stateful<Div>) -> Div {
    div()
        .pt(px(4.))
        .flex()
        .items_center()
        .gap(px(8.))
        .child(div().flex_1().min_w_0().text_size(px(13.)).text_color(TEXT_4).child(note))
        .child(cancel)
        .child(submit)
}

#[cfg(test)]
mod tests {
    use super::default_base;

    #[test]
    fn the_base_is_the_preferred_branch_else_main_master_the_current_one_or_the_first() {
        let base = |branches: &[&str], preferred, current| default_base(branches.iter().copied(), preferred, current);
        assert_eq!(base(&["dev", "main", "master", "rel"], "rel", "dev"), 3);
        assert_eq!(base(&["dev", "master", "main"], "gone", "dev"), 2);
        assert_eq!(base(&["dev", "master"], "", "dev"), 1);
        assert_eq!(base(&["dev", "feat"], "", "feat"), 1);
        assert_eq!(base(&["dev", "feat"], "", "detached"), 0);
    }
}
