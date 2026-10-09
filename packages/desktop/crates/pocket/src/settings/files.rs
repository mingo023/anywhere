use super::catalog::{Look, Setting};
use crate::desktop::Desktop;
use gpui_kit::Context;
use store::prefs::files::{Autosave, NewTabs};

const NEW_TABS: [NewTabs; 2] = [NewTabs::End, NewTabs::AfterCurrent];
const AUTOSAVES: [Autosave; 3] = [Autosave::Off, Autosave::AfterDelay, Autosave::OnBlur];

fn relist(d: &mut Desktop, cx: &mut Context<Desktop>) {
    d.relist_explorer(cx);
}

pub(super) const ROWS: &[Setting] = &[
    Setting::choice("new-tabs", "Tabs", "Open new tabs", &["At the end", "After current"], Look::Segmented, |s| s.files.new_tabs as usize, |s, i| s.files.new_tabs = NEW_TABS[i]),
    Setting::switch("preview-tabs", "Tabs", "Preview tabs", |s| s.files.preview_tabs, |s, on| s.files.preview_tabs = on).hint("A single click opens a temporary tab; the next click replaces it"),
    Setting::choice("markdown", "Tabs", "Open Markdown as", &["Rendered", "Source"], Look::Segmented, |s| usize::from(s.files.markdown_source), |s, i| s.files.markdown_source = i == 1),
    Setting::switch("soft-wrap", "Editor", "Soft wrap", |s| s.files.soft_wrap, |s, on| s.files.soft_wrap = on),
    Setting::switch("line-numbers", "Editor", "Line numbers", |s| s.files.line_numbers, |s, on| s.files.line_numbers = on),
    Setting::stepper("editor-font-size", "Editor", "Font size", (9, 24, 1), " pt", |s| s.files.font_size as i32, |s, n| s.files.font_size = n as u32),
    Setting::stepper("tab-width", "Editor", "Tab width", (1, 8, 1), " spaces", |s| s.files.tab_width as i32, |s, n| s.files.tab_width = n as u32),
    Setting::choice("autosave", "Editor", "Autosave", &["Off", "After 1 second", "When focus changes"], Look::Dropdown, |s| s.files.autosave as usize, |s, i| s.files.autosave = AUTOSAVES[i]),
    Setting::switch("hide-dotfiles", "Explorer", "Hide dotfiles", |s| s.files.hide_dotfiles, |s, on| s.files.hide_dotfiles = on).hint("Files and folders that start with a dot").effect(relist),
    Setting::switch("hide-ignored", "Explorer", "Hide gitignored files", |s| s.files.hide_ignored, |s, on| s.files.hide_ignored = on).effect(relist),
    Setting::switch("folders-first", "Explorer", "Folders first", |s| s.files.folders_first, |s, on| s.files.folders_first = on).effect(relist),
    Setting::switch("case-sensitive", "Explorer", "Case-sensitive sort", |s| s.files.case_sensitive, |s, on| s.files.case_sensitive = on)
        .hint("Off sorts README next to readme")
        .advanced()
        .effect(relist),
    Setting::text("also-hide", "Explorer", "Also hide", "node_modules, *.log", |s| s.files.also_hide.clone(), |s, v| s.files.also_hide = v)
        .hint("Names separated by commas; * matches any run of characters")
        .advanced()
        .effect(relist),
];

#[cfg(test)]
mod tests {
    use super::ROWS;
    use super::super::catalog::Control;
    use store::Store;

    #[test]
    fn the_files_rows_read_back_what_they_write() {
        let mut store = Store::default();
        for r in ROWS {
            if let Control::Choice { options, get, set, .. } = r.control {
                set(&mut store, options.len() - 1);
                assert_eq!(get(&store), options.len() - 1, "{}", r.id);
            }
        }
    }
}
