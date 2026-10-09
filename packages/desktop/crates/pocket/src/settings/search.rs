use super::catalog::{self, Setting};
use super::{Section, card, keybindings};
use crate::desktop::Desktop;
use crate::palette::runs;
use gpui_kit::*;
use theme::*;

/// "7 settings match "worktree" in 4 sections", counting `n` hits across `sections`.
fn summary(query: &str, n: usize, sections: usize) -> String {
    match (n, sections) {
        (0, _) => format!("No settings match \u{201c}{query}\u{201d}"),
        (1, _) => format!("1 setting matches \u{201c}{query}\u{201d}"),
        (n, 1) => format!("{n} settings match \u{201c}{query}\u{201d}"),
        (n, s) => format!("{n} settings match \u{201c}{query}\u{201d} in {s} sections"),
    }
}

/// The query's words on an amber wash, as the design marks matches.
fn marked(text: String, words: &[String]) -> StyledText {
    let style = HighlightStyle { color: Some(TEXT.into()), background_color: Some(WAITING_BG.into()), ..Default::default() };
    let marks: Vec<_> = runs(&text, words).into_iter().map(|r| (r, style)).collect();
    StyledText::new(text).with_highlights(marks)
}

/// A section's results under its icon and name.
fn result_group(section: Section, rows: Div) -> Div {
    let heading = div()
        .mb(px(7.))
        .ml(px(2.))
        .flex()
        .items_center()
        .gap(px(7.))
        .text_size(px(13.))
        .line_height(px(18.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(TEXT_2)
        .child(icon(section.icon(), 14., TEXT_3))
        .child(section.label());
    div().flex().flex_col().child(heading).child(rows)
}

/// "Group · hint" under a result's label.
fn detail(setting: &Setting) -> String {
    match setting.hint {
        Some(hint) => format!("{} \u{b7} {hint}", setting.group),
        None => setting.group.to_string(),
    }
}

impl Desktop {
    pub(super) fn search_results(&mut self, words: &[String], cx: &mut Context<Self>) -> Div {
        let hits = catalog::search(words);
        let keys = keybindings::search(words, &self.store.keys);
        let query = self.settings.search.read(cx).value().trim().to_string();
        let n = hits.iter().map(|(_, rows)| rows.len()).sum::<usize>() + keys.len();
        let sections = hits.len() + usize::from(!keys.is_empty());
        let mut column = div().child(div().text_size(px(13.)).text_color(TEXT_2).child(summary(&query, n, sections)));
        for (section, rows) in hits {
            let rows = rows
                .into_iter()
                .map(|setting| {
                    let text = div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(1.))
                        .child(div().text_size(px(13.)).line_height(px(18.)).font_weight(FontWeight::MEDIUM).text_color(TEXT).child(marked(setting.label.to_string(), words)))
                        .child(div().truncate().text_size(px(12.)).line_height(px(16.)).text_color(TEXT_3).child(marked(detail(setting), words)));
                    div().child(
                        div()
                            .id(SharedString::from(format!("settings-hit-{}", setting.id)))
                            .px(px(14.))
                            .py(px(9.))
                            .flex()
                            .items_center()
                            .gap(px(12.))
                            .cursor_pointer()
                            .hover(|d| d.bg(FILL_1))
                            .child(text)
                            .child(icon("chevron-right", 13., TEXT_4))
                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.open_result(section, setting, window, cx))),
                    )
                })
                .collect();
            column = column.child(result_group(section, card(rows)));
        }
        if !keys.is_empty() {
            let rows = keys
                .into_iter()
                .map(|(label, keys)| {
                    div().child(
                        div()
                            .id(SharedString::from(format!("settings-key-{label}")))
                            .px(px(14.))
                            .py(px(9.))
                            .flex()
                            .items_center()
                            .gap(px(12.))
                            .cursor_pointer()
                            .hover(|d| d.bg(FILL_1))
                            .child(div().flex_1().min_w_0().text_size(px(13.)).line_height(px(18.)).font_weight(FontWeight::MEDIUM).text_color(TEXT).child(marked(label.to_string(), words)))
                            .child(keys)
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open_section_result(Section::Keyboard, window, cx))),
                    )
                })
                .collect();
            column = column.child(result_group(Section::Keyboard, card(rows)));
        }
        column
    }

    /// Shows the row's section with the row tinted, unfolding Advanced if it lives there.
    fn open_result(&mut self, section: Section, setting: &'static Setting, window: &mut Window, cx: &mut Context<Self>) {
        if setting.advanced {
            self.store.advanced.insert(section.id().to_string());
            self.save_soon(cx);
        }
        self.open_section_result(section, window, cx);
        self.settings.found = Some(setting.id);
    }

    fn open_section_result(&mut self, section: Section, window: &mut Window, cx: &mut Context<Self>) {
        self.show_section(section, cx);
        self.settings.search.update(cx, |s, cx| s.set_value("", window, cx));
        window.focus(&self.root, cx);
        cx.notify();
    }
    pub(super) fn open_first_result(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let first = catalog::search(&self.settings.query(cx)).into_iter().find_map(|(s, rows)| Some((s, *rows.first()?)));
        if let Some((section, setting)) = first {
            self.open_result(section, setting, window, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{detail, summary};

    #[test]
    fn the_summary_counts_settings_and_sections() {
        assert_eq!(summary("vim", 0, 0), "No settings match \u{201c}vim\u{201d}");
        assert_eq!(summary("ban", 1, 1), "1 setting matches \u{201c}ban\u{201d}");
        assert_eq!(summary("s", 2, 1), "2 settings match \u{201c}s\u{201d}");
        assert_eq!(summary("s", 3, 2), "3 settings match \u{201c}s\u{201d} in 2 sections");
    }

    #[test]
    fn a_result_names_its_group_before_its_hint() {
        let rows = crate::settings::notifications::ROWS;
        let sounds = rows.iter().find(|r| r.id == "sounds").unwrap();
        assert_eq!([detail(&rows[0]), detail(sounds)], ["Banners \u{b7} A macOS banner when a session needs you, fails or is done", "Sounds"]);
    }
}
