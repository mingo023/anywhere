use crate::desktop::Desktop;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use gpui_kit::component::input::Input;
use gpui_kit::component::scroll::ScrollableElement as _;
use hosts::remote::{self, Entry};
use std::ops::Range;
use theme::*;
use ui::Variant;

/// A folder row's height.
const ROW: f32 = 28.;

/// Where Add project's host browser stands: a folder on a host, and its folders once listed.
#[derive(Clone, Default, PartialEq, Debug)]
pub(crate) struct Browse {
    pub host: String,
    pub dir: String,
    pub folders: Option<Result<Vec<String>, String>>,
}

impl Browse {
    pub fn at(host: &str, dir: &str) -> Self {
        Self { host: host.into(), dir: dir.into(), folders: None }
    }

    /// Takes the listing of `dir` on `host`, unless the browser has moved on since asking.
    pub fn listed(&mut self, host: &str, dir: &str, folders: Result<Vec<String>, String>) {
        if self.host == host && self.dir == dir {
            self.folders = Some(folders);
        }
    }

    /// The list's rows as `(name, folder it opens)`: `..` unless at a root, then the listed folders.
    pub fn rows(&self) -> Vec<(String, String)> {
        let up = parent(&self.dir);
        let up = (!self.dir.is_empty() && up != self.dir).then(|| ("..".to_string(), up));
        let names = match &self.folders {
            Some(Ok(names)) => names.as_slice(),
            _ => &[],
        };
        up.into_iter().chain(names.iter().map(|n| (n.clone(), child(&self.dir, n)))).collect()
    }
}

/// The folder holding `dir`; a root, `/` or a Windows drive's, holds itself.
pub(crate) fn parent(dir: &str) -> String {
    let up = dir.trim_end_matches('/').rsplit_once('/').map_or("", |(up, _)| up);
    match up {
        _ if hosts::on_windows(dir) && up.len() <= 2 => format!("{}/", &dir[..2]),
        "" => "/".into(),
        up => up.into(),
    }
}

pub(crate) fn child(dir: &str, name: &str) -> String {
    format!("{}/{name}", dir.trim_end_matches('/'))
}

/// A listing's folders, by name.
pub(crate) fn folders(entries: Vec<Entry>) -> Vec<String> {
    let mut names: Vec<String> = entries.into_iter().filter(|e| e.dir).map(|e| e.name).collect();
    names.sort_by_key(|n| n.to_lowercase());
    names
}

impl Desktop {
    /// Starts browsing `host` from its home folder.
    pub(crate) fn browse_host(&mut self, host: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(client) = remote::connected(&host) else { return };
        self.repo_form.draft.browse = Some(Browse::at(&host, ""));
        cx.spawn_in(window, async move |this, cx| {
            let home = cx.background_executor().spawn(async move { client.status().map(|s| s.user_home.replace('\\', "/")) }).await;
            this.update_in(cx, |d, window, cx| match home {
                Ok(home) => d.browse_to(home, window, cx),
                Err(e) => {
                    if let Some(b) = d.repo_form.draft.browse.as_mut() {
                        b.listed(&host, "", Err(e.to_string()));
                    }
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// Shows `dir` on the host being browsed, and lists its folders off the UI thread.
    pub(crate) fn browse_to(&mut self, dir: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(b) = self.repo_form.draft.browse.as_mut() else { return };
        let Some(client) = remote::connected(&b.host) else { return };
        *b = Browse::at(&b.host.clone(), &dir);
        let host = b.host.clone();
        self.repo_form.host_path.update(cx, |s, cx| s.set_value(dir.clone(), window, cx));
        cx.spawn(async move |this, cx| {
            let asked = dir.clone();
            let listed = cx.background_executor().spawn(async move { client.list(&asked).map(folders).map_err(|e| e.to_string()) }).await;
            this.update(cx, |d, cx| {
                if let Some(b) = d.repo_form.draft.browse.as_mut() {
                    b.listed(&host, &dir, listed);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// Opens the browsed folder as the project's.
    pub(crate) fn open_browsed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(b) = self.repo_form.draft.browse.as_ref() else { return };
        let key = hosts::place(&b.host, &b.dir);
        self.set_repo_path(key, window, cx);
        cx.notify();
    }

    /// The connected hosts to pick from, then the folder being browsed on the picked one.
    pub(super) fn host_browser(&self, cx: &mut Context<Self>) -> Div {
        let browse = self.repo_form.draft.browse.clone();
        let picked = browse.as_ref().map(|b| b.host.clone());
        let online: Vec<_> = self.store.hosts.iter().filter(|h| remote::connected(&h.id).is_some()).collect();
        if online.is_empty() {
            return div().text_size(px(12.5)).text_color(TEXT_3).child("No host is connected. Add one in Settings › Hosts.");
        }
        let chips = div().flex().flex_wrap().gap(px(6.)).children(online.iter().enumerate().map(|(i, h)| {
            let on = picked.as_deref() == Some(h.id.as_str());
            let id = h.id.clone();
            ui::button(("host-pick", i), if on { Variant::Secondary } else { Variant::Ghost }, Some("server"), h.name.clone())
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.browse_host(id.clone(), window, cx)))
        }));
        let Some(b) = browse else { return div().flex().flex_col().gap(px(8.)).child(chips) };
        let path = ui::field_box()
            .child(icon("globe", 13., TEXT_3))
            .child(div().flex_1().font_family(MONO).child(Input::new(&self.repo_form.host_path).appearance(false).p_0().text_size(px(13.))))
            .child(ui::button("host-open", Variant::Secondary, None, "Open").when(!b.dir.is_empty(), |d| d.on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open_browsed(window, cx)))));
        let rows = b.rows();
        let height = px((rows.len() as f32 * ROW).min(200.));
        let list = uniform_list(
            "host-folders",
            rows.len(),
            cx.processor(move |_, range: Range<usize>, _, cx| {
                range
                    .map(|i| {
                        let (name, dir) = rows[i].clone();
                        div()
                            .id(("host-folder", i))
                            .w_full()
                            .h(px(ROW))
                            .px(px(10.))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .rounded(px(7.))
                            .cursor_pointer()
                            .hover(|s| s.bg(FILL_2))
                            .child(icon("folder", 13., TEXT_3))
                            .child(div().truncate().font_family(MONO).text_size(px(12.5)).child(name))
                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.browse_to(dir.clone(), window, cx)))
                    })
                    .collect::<Vec<_>>()
            }),
        )
        .track_scroll(&self.repo_form.host_scroll)
        .size_full();
        let list = div().relative().h(height).child(list).vertical_scrollbar(&self.repo_form.host_scroll);
        let note = match &b.folders {
            None => Some(div().px(px(10.)).text_size(px(12.5)).text_color(TEXT_4).child("Listing…")),
            Some(Err(e)) => Some(div().px(px(10.)).text_size(px(12.5)).text_color(FAILED).child(e.clone())),
            Some(Ok(_)) => None,
        };
        div().flex().flex_col().gap(px(8.)).child(chips).child(path).child(div().flex().flex_col().child(list).children(note))
    }
}

#[cfg(test)]
mod tests {
    use super::{Browse, child, folders, parent};
    use hosts::remote::Entry;

    #[test]
    fn going_up_stops_at_the_root() {
        assert_eq!((parent("/home/mingo"), parent("/home/"), parent("/home"), parent("/")), ("/home".into(), "/".into(), "/".into(), "/".into()));
        assert_eq!((child("/home", "mingo"), child("/", "srv")), ("/home/mingo".into(), "/srv".into()));
    }

    #[test]
    fn going_up_on_windows_stops_at_the_drive() {
        assert_eq!((parent("C:/Users/me"), parent("C:/Users"), parent("C:/"), parent("D:")), ("C:/Users".into(), "C:/".into(), "C:/".into(), "D:/".into()));
        assert_eq!(child("C:/", "Users"), "C:/Users");
    }

    #[test]
    fn the_list_offers_going_up_except_at_a_root() {
        let mut b = Browse::at("pc", "C:/Users");
        b.listed("pc", "C:/Users", Ok(vec!["me".into()]));
        assert_eq!(b.rows(), [("..".to_string(), "C:/".to_string()), ("me".into(), "C:/Users/me".into())]);
        assert_eq!(Browse::at("pc", "C:/").rows(), []);
        assert_eq!(Browse::at("devbox", "/").rows(), []);
        assert_eq!(Browse::at("devbox", "").rows(), [], "nothing until the home folder is known");
    }

    #[test]
    fn a_listing_shows_only_folders_in_name_order() {
        let entry = |name: &str, dir: bool| Entry { name: name.into(), dir, ..Default::default() };
        assert_eq!(folders(vec![entry("src", true), entry("README.md", false), entry("Apps", true)]), ["Apps", "src"]);
    }

    #[test]
    fn a_listing_for_a_folder_already_left_is_dropped() {
        let mut b = Browse::at("devbox", "/home/mingo/app");
        b.listed("devbox", "/home/mingo", Ok(vec!["app".into()]));
        assert_eq!(b.folders, None);
        b.listed("devbox", "/home/mingo/app", Ok(vec!["src".into()]));
        assert_eq!(b.folders, Some(Ok(vec!["src".into()])));
    }
}
