// Release builds open no console window beside the app on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod accounts;
mod actions;
mod add_to_chat;
mod automations;
mod browser;
mod capture;
mod chat;
mod composer;
mod creating;
mod desktop;
mod explorer;
mod field;
mod fleet;
mod fs;
mod host_agents;
mod git_ui;
mod inbox;
mod modals;
mod notch;
mod palette;
mod panels;
mod region;
mod removal;
mod settings;
mod sidebar;
mod status;
mod syntax;
mod terminal_view;
mod terminals;
mod updates;
mod util;

use crate::desktop::geometry::{self, MIN_WINDOW};
use crate::desktop::{Desktop, follow_reduce_motion};
use daemon::Daemon;
use futures::StreamExt;
use gpui_kit::component::Root;
use gpui_kit::*;
use serde_json::json;
use std::time::Duration;
use store::Store;

fn main() {
    if channel::is_release() {
        std::thread::spawn(daemon::service::register_if_needed);
    }
    let capture = capture::Capture::from_args();
    let own = daemon::Endpoint::own();
    let (daemon, mut rx) = Daemon::spawn_to(own.clone());
    let daemon = hosts::session::Daemons::new(daemon);
    let path = &own.path;
    let home = path.parent().unwrap_or(path).to_path_buf();
    let (outbox, mut agent_rx) = agents::connect_with(path, own.token.clone());
    let store = Store::load(&home);
    if channel::is_release() && store.general.open_at_login {
        std::thread::spawn(|| daemon::service::open_at_login(true));
    }
    let hitch_log = home.join("logs/chat-scroll.log");
    gpui_kit::application().with_assets(theme::Assets).run(move |cx| {
        gpui_kit::init(cx);
        gpui_kit::gpui::hitch::enable(hitch_log, format!("version={} arch={}", channel::version(), std::env::consts::ARCH));
        let capturing = capture.is_some();
        // Captures stay light (capture.rs forces it per screen).
        let forced = settings::forced(store.appearance.mode).filter(|_| !capturing);
        if forced.is_some() {
            cx.set_window_appearance(forced);
        }
        let appearance = forced.unwrap_or_else(|| cx.window_appearance());
        theme::init(appearance, cx);
        theme::set_blue_orange(store.appearance.diff_colors == store::DiffColors::BlueOrange);
        theme::set_diff_colors(store.appearance.added, store.appearance.removed);
        theme::set_glass(store.appearance.glass());
        theme::set_syntax(store.appearance.syntax as usize, cx);
        theme::set_fonts(store.appearance.ui_font.as_deref(), store.appearance.code_font.as_deref(), cx);
        theme::set_accent(store.appearance.accent, cx);
        syntax::init();
        follow_reduce_motion(store.appearance.reduce_motion, cx);
        cx.bind_keys(actions::keymap(&store.keys));
        cx.bind_keys(keys::bindings());
        cx.on_action(|_: &actions::Quit, cx| cx.quit());
        let check = channel::is_release().then(|| [MenuItem::action("Check for Updates…", actions::CheckForUpdates), MenuItem::separator()]);
        cx.set_menus([Menu::new("Anywhere").items(
            check.into_iter().flatten().chain([MenuItem::os_submenu("Services", SystemMenuType::Services), MenuItem::separator(), MenuItem::action("Quit Anywhere", actions::Quit)]),
        )]);
        let primary = cx.primary_display().map(|d| d.id());
        let mut displays = cx.displays();
        displays.sort_by_key(|d| Some(d.id()) != primary);
        let rects: Vec<_> = displays.iter().map(|d| (d.uuid().ok().map(|u| u.to_string()), d.bounds())).collect();
        let (display, bounds) = if capturing { geometry::restore(None, &[]) } else { geometry::restore(store.window.as_ref(), &rects) };
        let mut opts = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            display_id: displays.get(display).filter(|_| !capturing).map(|d| d.id()),
            window_min_size: Some(MIN_WINDOW),
            titlebar: Some(TitlebarOptions {
                title: Some("Anywhere".into()),
                appears_transparent: true,
                traffic_light_position: Some(point(px(desktop::chrome::LIGHTS_AT), px(desktop::chrome::LIGHTS_AT))),
            }),
            window_background: theme::window_background(),
            // Else AppKit drags the window from the titlebar band before a tab there sees the press; drag_area moves it instead.
            app_owns_titlebar_drag: true,
            ..Default::default()
        };
        if capture.is_some() {
            capture::Capture::hide(&mut opts);
        }
        let window = cx.open_window(opts, move |window, cx| {
            let view = cx.new(|cx| {
                cx.spawn_in(window, async move |this, cx| {
                    while let Some(m) = rx.next().await {
                        if this.update_in(cx, |d: &mut Desktop, window, cx| d.on_msg(m, window, cx)).is_err() {
                            break;
                        }
                    }
                })
                .detach();
                cx.spawn_in(window, async move |this, cx| {
                    while let Some(ev) = agent_rx.next().await {
                        let mut events = vec![ev];
                        while let Ok(ev) = agent_rx.try_recv() {
                            events.push(ev);
                        }
                        let events = agents::coalesce(events);
                        if this.update_in(cx, |d: &mut Desktop, window, cx| events.into_iter().for_each(|ev| d.on_agents(ev, window, cx))).is_err() {
                            break;
                        }
                    }
                })
                .detach();
                let poll = daemon.clone();
                cx.spawn(async move |this, cx| {
                    for tick in 0u64.. {
                        poll.send(json!({"op": "list"}));
                        let refresh = |d: &mut Desktop, cx: &mut Context<Desktop>| {
                            if tick % d.store.general.git_every() == 0 && d.git_done == d.git_run {
                                d.refresh_git(cx);
                            }
                            if tick % accounts::CHECK_EVERY == 0 {
                                d.refresh_accounts(cx);
                            }
                        };
                        if this.update(cx, refresh).is_err() {
                            break;
                        }
                        cx.background_executor().timer(Duration::from_secs(1)).await;
                    }
                })
                .detach();
                let handle = window.window_handle();
                let this = cx.weak_entity();
                cx.on_system_notification_response(move |r, cx| match r.action_id {
                    Some(action) => {
                        let _ = this.update(cx, |d, cx| d.answer_banner(&r.tag, &action, cx));
                    }
                    None => {
                        cx.activate(true);
                        let _ = handle.update(cx, |_, window, cx| this.update(cx, |d, cx| d.focus_agent(&r.tag, window, cx)));
                    }
                });
                let mut desktop = Desktop::new(daemon, hosts::session::Outboxes::new(outbox), store, window, cx);
                desktop.list_worktrees(cx);
                if !capturing {
                    desktop.connect_hosts(window, cx);
                }
                desktop
            });
            // Root paints gpui-kit's opaque background, which would hide the blurred desktop in dark.
            cx.new(|cx| Root::new(view, window, cx).bg(transparent_black()))
        })
        .expect("open window");
        match capture {
            Some(c) => c.run(window, cx),
            None => cx.activate(true),
        }
    });
}
