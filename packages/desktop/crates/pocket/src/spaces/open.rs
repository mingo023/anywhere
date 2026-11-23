use super::FLASH;
use crate::desktop::Desktop;
use crate::modals::host_finder::Opener;
use gpui_kit::*;
use std::time::Instant;

impl Desktop {
    /// Open folder for the space on screen: Finder's picker on This Mac, the host finder on a host.
    pub(crate) fn open_folder(&mut self, opener: Opener, window: &mut Window, cx: &mut Context<Self>) {
        self.close_menus(cx);
        self.new_form.update(cx, |f, cx| f.close_picker(cx));
        if let Some(host) = self.space().host().map(str::to_string) {
            return self.open_host_finder(host, None, opener, window, cx);
        }
        match opener {
            Opener::NewSession => self.pick_path(true, window, cx, |d, paths, window, cx| {
                if let Some(path) = paths.into_iter().next() {
                    d.pick_new_project(path.to_string_lossy().into_owned(), window, cx);
                }
            }),
            _ => self.add_local_project(window, cx),
        }
    }

    /// Keeps the folder `key` as a project, with the settings it had before or the defaults in the next colour.
    pub(crate) fn save_folder(&mut self, key: &str, cx: &mut Context<Self>) {
        let cfg = self.new_repo_config(key, cx);
        self.store.repos.entry(key.to_string()).or_insert(cfg);
        self.keep_project(key, cx);
    }

    /// Lights `key`, a project's row or a space's button, then lets it go.
    pub(crate) fn flash(&mut self, key: String, cx: &mut Context<Self>) {
        self.spaces.flash(key, Instant::now());
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(FLASH).await;
            this.update(cx, |_, cx| cx.notify()).ok();
        })
        .detach();
        cx.notify();
    }

    /// A folder chosen from the sidebar: its row lights up and the page opens on it.
    pub(crate) fn open_chosen_folder(&mut self, key: String, window: &mut Window, cx: &mut Context<Self>) {
        self.save_folder(&key, cx);
        self.flash(key.clone(), cx);
        self.new_place(key, window, cx);
    }
}
