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
    let (daemon, mut rx) = Daemon::connect(&path).unwrap_or_else(|e| {
        eprintln!("pocket-desktop: cannot reach pocketd at {}: {e}", path.display());
        std::process::exit(1)
    });
    let home = path.parent().unwrap_or(&path).to_path_buf();
    let (outbox, mut agent_rx) = agents::connect(&home);
    let store = Store::load(&home);
    gpui_kit::application().with_assets(theme::Assets).run(move |cx| {
        gpui_kit::init(cx);
        theme::init(cx);
        follow_reduce_motion(cx);
        cx.bind_keys(actions::bindings());
        cx.bind_keys(keys::bindings());
        let bounds = Bounds::centered(None, size(px(1440.), px(900.)), cx);
        let mut opts = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some("Coding Pocket".into()),
                appears_transparent: true,
                traffic_light_position: Some(point(px(14.), px(14.))),
            }),
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
                cx.spawn(async move |this, cx| {
                    while let Some(ev) = agent_rx.next().await {
                        if this.update(cx, |d: &mut Desktop, cx| d.on_agents(ev, cx)).is_err() {
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
                cx.on_system_notification_response(move |r, cx| {
                    cx.activate(true);
                    let _ = handle.update(cx, |_, window, cx| this.update(cx, |d, cx| d.focus_agent(&r.tag, window, cx)));
                });
                Desktop::new(daemon, outbox, store, window, cx)
            });
            cx.new(|cx| Root::new(view, window, cx))
        })
        .expect("open window");
        match capture {
            Some(c) => c.run(window, cx),
            None => cx.activate(true),
        }
    });
}
