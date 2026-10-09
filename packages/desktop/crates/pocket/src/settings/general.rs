use super::catalog::{Look, Setting};
use super::host::service_line;
use super::{action, row};
use crate::desktop::Desktop;
use crate::updates::Update;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use store::Layout;
use store::prefs::general::ConfirmQuit;
use theme::*;

fn update_status(update: &Update, available: bool) -> (Option<Token>, String) {
    match update {
        Update::Ready { version } => (Some(WAITING_DOT), format!("Version {version} is ready to install")),
        Update::WhatsNew { version } => (Some(SUCCESS), format!("Updated to {version}")),
        Update::Idle if available => (None, "Checks for updates automatically".into()),
        Update::Idle => (None, "Updates come with release builds".into()),
    }
}

const CHECK_HOURS: [u32; 4] = [1, 6, 24, 168];
const LAUNCH_LAYOUTS: [Option<Layout>; 4] = [None, Some(Layout::Sidebars), Some(Layout::Compact), Some(Layout::Focus)];
const CONFIRM: [ConfirmQuit; 3] = [ConfirmQuit::Never, ConfirmQuit::Always, ConfirmQuit::Working];
const PR_SECS: [u32; 4] = [15, 30, 60, 300];
const LINGER: [u32; 4] = [0, 2, 10, 30];

/// Where `value` sits in `options`; a value no option names reads as `fallback`.
fn index<T: PartialEq>(options: &[T], value: T, fallback: usize) -> usize {
    options.iter().position(|o| *o == value).unwrap_or(fallback)
}

pub(super) const ROWS: &[Setting] = &[
    Setting::custom("update", "Software update", "Software update", |d, _, cx| d.update_row(cx)).hint("Check for a new version of Anywhere"),
    Setting::switch("auto-install", "Software update", "Install updates automatically", |s| s.general.auto_install, |s, on| s.general.auto_install = on)
        .hint("Downloads in the background and installs the next time you quit")
        .effect(follow_updates),
    Setting::switch("whats-new", "Software update", "Show \u{201c}What\u{2019}s new\u{201d} after updating", |s| s.general.whats_new, |s, on| s.general.whats_new = on)
        .hint("A card in the sidebar links to the release notes, once")
        .effect(|d, cx| {
            if !d.store.general.whats_new {
                d.dismiss_whats_new(cx);
            }
        }),
    Setting::choice("check-every", "Software update", "Check for updates", &["Hourly", "Every 6 hours", "Daily", "Weekly"], Look::Dropdown, |s| index(&CHECK_HOURS, s.general.check_hours, 1), |s, i| {
        s.general.check_hours = CHECK_HOURS[i]
    })
    .hint("How often to look for a new version")
    .advanced()
    .effect(follow_updates),
    Setting::switch("open-at-login", "Startup & background", "Open at login", |s| s.general.open_at_login, |s, on| s.general.open_at_login = on)
        .hint("Opens Anywhere when you log in to the Mac")
        .effect(|d, cx| d.open_at_login(cx)),
    Setting::switch("resume-agents", "Startup & background", "Resume agents after restart", |s| s.general.resume_agents, |s, on| s.general.resume_agents = on)
        .hint("Reopens agent sessions when the service or the Mac restarts")
        .effect(|d, cx| d.set_host("restore.resumeAgents", d.store.general.resume_agents.to_string(), cx)),
    Setting::switch("restore-layouts", "Startup & background", "Restore tabs and splits", |s| s.general.restore_layouts, |s, on| s.general.restore_layouts = on)
        .hint("Reopens the last window layout instead of a fresh window"),
    Setting::custom("service", "Startup & background", "Background service", |d, _, cx| d.service_row(cx)).hint("Restart the service that hosts every terminal and agent").advanced(),
    Setting::choice("launch-layout", "Launch layout", "Layout at launch", &["Last used", "Sidebars", "Compact", "Focus"], Look::Segmented, |s| index(&LAUNCH_LAYOUTS, s.general.launch_layout, 0), |s, i| {
        s.general.launch_layout = LAUNCH_LAYOUTS[i]
    })
    .hint("Focus hides both sidebars so the terminal fills the window"),
    Setting::choice("panel-at-launch", "Launch layout", "Right panel at launch", &["Open", "Closed"], Look::Segmented, |s| usize::from(!s.general.panel_open), |s, i| s.general.panel_open = i == 0)
        .hint("Sessions, Explorer and Changes"),
    Setting::choice("confirm-quit", "Quitting & sleep", "Confirm before quitting", &["Never", "Always", "While agents are working"], Look::Dropdown, |s| index(&CONFIRM, s.general.confirm_quit, 1), |s, i| {
        s.general.confirm_quit = CONFIRM[i]
    })
    .hint("Agents keep going in the background after you quit"),
    Setting::switch("keep-awake", "Quitting & sleep", "Keep Mac awake while agents work", |s| s.general.keep_awake, |s, on| s.general.keep_awake = on)
        .hint("Stops idle sleep; the display can still turn off")
        .effect(|d, cx| d.set_host("awake.enabled", d.store.general.keep_awake.to_string(), cx)),
    Setting::choice("stay-awake", "Quitting & sleep", "Stay awake after agents stop", &["Off", "2 minutes", "10 minutes", "30 minutes"], Look::Dropdown, |s| index(&LINGER, s.general.linger_minutes, 1), |s, i| {
        s.general.linger_minutes = LINGER[i]
    })
    .hint("Gives you time to read the result before the Mac sleeps")
    .under(|s| s.general.keep_awake)
    .effect(|d, cx| d.set_host("awake.lingerMinutes", d.store.general.linger_minutes.to_string(), cx)),
    Setting::switch("battery-saver", "Performance", "Battery saver", |s| s.general.battery_saver, |s, on| s.general.battery_saver = on)
        .hint("Checks git every 10 s and pull requests every 5 min instead"),
    Setting::stepper("git-every", "Performance", "Refresh git status every", (2, 60, 1), " s", |s| s.general.git_secs as i32, |s, n| s.general.git_secs = n as u32)
        .hint("Branch, ahead/behind and changed files")
        .advanced()
        .under(|s| !s.general.battery_saver),
    Setting::choice("prs-every", "Performance", "Refresh pull requests every", &["15 s", "30 s", "1 min", "5 min"], Look::Dropdown, |s| index(&PR_SECS, s.general.pr_secs, 1), |s, i| s.general.pr_secs = PR_SECS[i])
        .hint("Checks and review status from GitHub")
        .advanced()
        .under(|s| !s.general.battery_saver),
];

fn follow_updates(d: &mut Desktop, _: &mut Context<Desktop>) {
    d.updates.follow(&d.store.general);
}

impl Desktop {
    fn open_at_login(&mut self, cx: &mut Context<Self>) {
        if !channel::is_release() {
            return;
        }
        let on = self.store.general.open_at_login;
        cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async move { daemon::service::open_at_login(on) }).await;
            this.update(cx, |d, cx| {
                if let Err(e) = result {
                    d.error = Some(format!("Couldn't change opening at login: {e}"));
                    d.store.general.open_at_login = !on;
                    d.save_soon(cx);
                    cx.notify();
                }
            })
        })
        .detach();
    }

    fn service_row(&mut self, cx: &mut Context<Self>) -> Div {
        let line = service_line(!self.terminals.link.is_down(), self.settings.host.status.as_ref());
        let title = div().flex().flex_col().gap(px(1.)).child("Background service").child(div().text_size(px(12.)).line_height(px(16.)).font_weight(FontWeight::NORMAL).text_color(TEXT_3).child(line));
        let restart = self.terminals.link.service.is_some().then(|| {
            action("settings-restart-service", "Restart service").on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.settings.host.status = None;
                this.restart_service(cx);
            }))
        });
        row(title, None, div().when_some(restart, |d, c| d.child(c)))
    }

    fn update_row(&mut self, cx: &mut Context<Self>) -> Div {
        let (dot, status) = update_status(&self.updates.update, self.updates.available());
        let title = div()
            .flex()
            .flex_col()
            .gap(px(1.))
            .child(format!("Anywhere {}", channel::version()))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .text_size(px(12.))
                    .line_height(px(16.))
                    .font_weight(FontWeight::NORMAL)
                    .text_color(TEXT_3)
                    .children(dot.map(|c| ui::dot(8., c)))
                    .child(status),
            );
        let control = match self.updates.update {
            Update::Ready { .. } => Some(
                action("settings-restart-update", "Restart to update")
                    .bg(TEXT)
                    .text_color(ON_TEXT)
                    .hover(|d| d.bg(TEXT))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.restart_to_update(cx))),
            ),
            _ if self.updates.available() => Some(action("settings-check-updates", "Check now").on_click(cx.listener(|this, _: &ClickEvent, _, _| this.updates.check()))),
            _ => None,
        };
        row(title, None, div().when_some(control, |d, c| d.child(c)))
    }
}

#[cfg(test)]
mod tests {
    use super::super::catalog::Control;
    use super::{ROWS, update_status};
    use crate::updates::Update;

    #[test]
    fn every_choice_reads_back_what_it_writes() {
        for row in ROWS {
            let Control::Choice { options, get, set, .. } = row.control else { continue };
            for i in 0..options.len() {
                let mut store = store::Store::default();
                set(&mut store, i);
                assert_eq!(get(&store), i, "{}", row.id);
            }
        }
    }

    #[test]
    fn the_choices_start_where_the_app_always_was() {
        let store = store::Store::default();
        let picked: Vec<_> = ROWS.iter().filter_map(|r| r.value(&store).map(|v| (r.id, v))).filter(|(id, _)| ["check-every", "launch-layout", "panel-at-launch", "confirm-quit", "stay-awake"].contains(id)).collect();
        assert_eq!(
            picked,
            [("check-every", "Every 6 hours".into()), ("launch-layout", "Last used".into()), ("panel-at-launch", "Closed".into()), ("confirm-quit", "Always".into()), ("stay-awake", "2 minutes".into())]
        );
    }

    #[test]
    fn a_downloaded_update_says_it_is_ready_to_install() {
        let (dot, text) = update_status(&Update::Ready { version: "1.5.0".into() }, true);
        assert_eq!((dot.is_some(), text.as_str()), (true, "Version 1.5.0 is ready to install"));
    }

    #[test]
    fn a_dev_build_says_updates_come_with_release_builds() {
        assert_eq!(update_status(&Update::Idle, false).1, "Updates come with release builds");
    }
}
