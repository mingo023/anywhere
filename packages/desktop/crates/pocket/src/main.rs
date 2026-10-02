mod actions;
mod capture;
mod desktop;
mod explorer;
mod git_ui;
mod inbox;
mod modals;
mod palette;
mod sidebar;
mod status;
mod syntax;
mod terminal_view;
mod terminals;
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
    let capture = capture::Capture::from_args();
    let path = daemon::sock_path();
    let (daemon, mut rx) = Daemon::spawn(&path);
    let home = path.parent().unwrap_or(&path).to_path_buf();
    let (outbox, mut agent_rx) = agents::connect(&path);
    let store = Store::load(&home);
    gpui_kit::application().with_assets(theme::Assets).run(move |cx| {
        gpui_kit::init(cx);
        theme::init(cx);
        syntax::init();
        follow_reduce_motion(cx);
        cx.bind_keys(actions::bindings());
        cx.bind_keys(keys::bindings());
        cx.on_action(|_: &actions::Quit, cx| cx.quit());
        let primary = cx.primary_display().map(|d| d.id());
        let mut displays = cx.displays();
        displays.sort_by_key(|d| Some(d.id()) != primary);
        let rects: Vec<_> = displays.iter().map(|d| (d.uuid().ok().map(|u| u.to_string()), d.bounds())).collect();
        let capturing = capture.is_some();
        let (display, bounds) = if capturing { geometry::restore(None, &[]) } else { geometry::restore(store.window.as_ref(), &rects) };
        let mut opts = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            display_id: displays.get(display).filter(|_| !capturing).map(|d| d.id()),
            window_min_size: Some(MIN_WINDOW),
            titlebar: Some(TitlebarOptions {
                title: Some("Anywhere".into()),
                appears_transparent: true,
                traffic_light_position: Some(point(px(14.), px(14.))),
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
                        if this.update_in(cx, |d: &mut Desktop, window, cx| d.on_agents(ev, window, cx)).is_err() {
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
                            if d.git_done == d.git_run {
                                d.refresh_git(cx);
                            }
                        };
                        if tick % 2 == 0 && this.update(cx, refresh).is_err() {
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
                Desktop::new(daemon, outbox, store, window, cx)
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
