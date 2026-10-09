use super::catalog::{Look, Setting};
use super::field_box;
use crate::desktop::Desktop;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::*;
use store::Store;
use store::prefs::browser::{Engine, Home, search_problem};
use theme::*;

const ENGINES: [Engine; 5] = [Engine::Google, Engine::DuckDuckGo, Engine::Bing, Engine::Kagi, Engine::Custom];
const HOMES: [Home; 3] = [Home::DevServer, Home::Blank, Home::Custom];

/// How many projects have a dev server URL, as the reset sheet names it.
fn dev_urls(s: &Store) -> String {
    match s.repos.values().filter(|r| !r.dev_url.is_empty()).count() {
        0 => "None".into(),
        1 => "1 project".into(),
        n => format!("{n} projects"),
    }
}

pub(super) const ROWS: &[Setting] = &[
    Setting::choice("search-engine", "General", "Search engine", &["Google", "DuckDuckGo", "Bing", "Kagi", "Custom…"], Look::Dropdown, |s| s.browser.engine as usize, |s, i| s.browser.engine = ENGINES[i]),
    Setting::text("custom-search", "General", "Search URL", "https://example.com/search?q=%s", |s| s.browser.custom_search.clone(), |s, v| s.browser.custom_search = v)
        .hint("%s stands for what you type")
        .note(|s| search_problem(&s.browser.custom_search).map(Into::into))
        .under(|s| s.browser.engine == Engine::Custom),
    Setting::choice("home-page", "General", "Home page", &["Dev server", "Blank", "Custom"], Look::Segmented, |s| s.browser.home as usize, |s, i| s.browser.home = HOMES[i])
        .hint("Project dev server opens the URL below for the focused project"),
    Setting::text("custom-home", "General", "Custom home page", "https://", |s| s.browser.custom_home.clone(), |s, v| s.browser.custom_home = v).under(|s| s.browser.home == Home::Custom),
    Setting::custom("dev-servers", "Dev servers", "Dev servers", |d, _, _| d.dev_servers()).resets(dev_urls, |s, _| s.repos.values_mut().for_each(|r| r.dev_url.clear())),
];

impl Desktop {
    /// Builds each project's dev server field afresh when Settings opens, since fields need the window.
    pub(crate) fn load_dev_urls(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let fields = self.store.projects.iter().map(|p| {
            let url = self.store.repos.get(p).map(|r| r.dev_url.clone()).unwrap_or_default();
            let field = cx.new(|cx| InputState::new(window, cx).placeholder("Not set").default_value(url));
            let path = p.clone();
            let sub = cx.subscribe(&field, move |this, field, ev: &InputEvent, cx| {
                if let InputEvent::Change = ev {
                    this.store.repos.entry(path.clone()).or_default().dev_url = field.read(cx).value().to_string();
                    this.save_soon(cx);
                }
            });
            (p.clone(), (field, sub))
        });
        self.settings.dev_urls = fields.collect();
    }

    fn dev_servers(&mut self) -> Div {
        let caption = div().px(px(14.)).py(px(9.)).border_b(px(0.5)).border_color(SEPARATOR).text_size(px(12.)).text_color(TEXT_3).child("One URL per project. Also editable in each project\u{2019}s settings.");
        let rows = self.store.projects.iter().filter_map(|p| {
            let (field, _) = self.settings.dev_urls.get(p)?;
            let input = field_box().w(px(180.)).child(div().flex_1().min_w_0().font_family(MONO).child(Input::new(field).appearance(false).p_0().text_size(px(12.))));
            Some(
                div()
                    .px(px(14.))
                    .py(px(7.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .border_b(px(0.5))
                    .border_color(SEPARATOR)
                    .child(self.project_tile(p).size(px(20.)).rounded(px(5.)).text_size(px(8.5)))
                    .child(div().flex_1().min_w_0().truncate().text_size(px(13.)).text_color(TEXT).child(self.repo_name(p)))
                    .child(input),
            )
        });
        div().flex().flex_col().child(caption).children(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::ROWS;
    use super::super::catalog::Control;
    use store::Store;

    #[test]
    fn the_browser_rows_read_back_what_they_write() {
        let mut store = Store::default();
        for r in ROWS {
            if let Control::Choice { options, get, set, .. } = r.control {
                set(&mut store, options.len() - 1);
                assert_eq!(get(&store), options.len() - 1, "{}", r.id);
            }
        }
    }
}
