use super::{HOST_ICONS, Reach, Space, colour, glyph, kept_name, space_of};
use crate::desktop::Desktop;
use crate::desktop::chrome::Confirm;
use crate::settings::ssh_hosts::link_color;
use crate::sidebar::rename::rename_field;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use workspace::Place;

/// A host's name, edited in the space header; leaving it saves, as Enter does.
struct HostRename {
    id: String,
    input: Entity<InputState>,
    _sub: Subscription,
}

pub(crate) struct Head {
    pub(crate) space: Space,
    pub(crate) name: String,
    pub(crate) glyph: &'static str,
    pub(crate) tint: Option<u32>,
    pub(crate) address: String,
    pub(crate) dot: Option<Token>,
    pub(crate) reach: Reach,
    pub(crate) terminal: bool,
}

pub(crate) enum HeaderEvent {
    OpenMenu,
    OpenTerminal,
    HostSettings,
    Reconnect(String),
    Rename { id: String, text: String },
    Icon { id: String, icon: &'static str },
    Colour { id: String, colour: u32 },
    Remove { id: String, name: String },
}

/// Above Projects: the space on screen, its link and address, opening the space menu.
pub(crate) struct SpaceHeader {
    head: Head,
    menu: bool,
    rename: Option<HostRename>,
}

impl EventEmitter<HeaderEvent> for SpaceHeader {}

/// An icon on a tile of a space's colour; This Mac's is plain.
pub(crate) fn tile(glyph: &str, tint: Option<u32>, size: f32) -> Div {
    let tile = div().size(px(size)).flex().flex_none().items_center().justify_center().rounded(px(size * 0.28));
    match tint {
        Some(c) => tile.bg(rgb(c)).child(icon(glyph, size * 0.6, WHITE)),
        None => tile.bg(FILL_4).child(icon(glyph, size * 0.6, TEXT_2)),
    }
}

impl SpaceHeader {
    pub(crate) fn new() -> Self {
        Self {
            head: Head {
                space: Space::Local,
                name: String::new(),
                glyph: "",
                tint: None,
                address: String::new(),
                dot: None,
                reach: Reach::Online,
                terminal: false,
            },
            menu: false,
            rename: None,
        }
    }

    pub(crate) fn set(&mut self, head: Head) {
        self.head = head;
    }

    pub(crate) fn menu_open(&self) -> bool {
        self.menu
    }

    pub(crate) fn open(&mut self, cx: &mut Context<Self>) {
        self.menu = true;
        cx.notify();
    }

    pub(crate) fn shut(&mut self) {
        self.menu = false;
    }

    pub(crate) fn cancel_rename(&mut self) -> bool {
        self.rename.take().is_some()
    }

    pub(crate) fn reset(&mut self) {
        (self.menu, self.rename) = (false, None);
    }

    fn start_rename(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        let (input, sub) = rename_field(self.head.name.clone(), window, cx, Self::save_rename);
        self.menu = false;
        self.rename = Some(HostRename { id, input, _sub: sub });
        cx.notify();
    }

    fn save_rename(&mut self, cx: &mut Context<Self>) {
        let Some(r) = self.rename.take() else { return };
        let text = r.input.read(cx).value().to_string();
        cx.emit(HeaderEvent::Rename { id: r.id, text });
        cx.notify();
    }

    fn space_menu(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let (space, name) = (&self.head.space, self.head.name.clone());
        let label = if *space == Space::Local { "Open terminal".to_string() } else { format!("Open terminal on {name}") };
        let terminal = ui::menu_row("space-terminal", "terminal", &label, None).when(!self.head.terminal, |d| d.opacity(0.5).cursor_default()).when(
            self.head.terminal,
            |d| {
                d.on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.menu = false;
                    cx.emit(HeaderEvent::OpenTerminal);
                }))
            },
        );
        let settings =
            ui::menu_row("space-settings", "settings", if *space == Space::Local { "Hosts settings\u{2026}" } else { "Host settings\u{2026}" }, None).on_click(
                cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.menu = false;
                    cx.emit(HeaderEvent::HostSettings);
                }),
            );
        let menu = ui::pop(div().id("aside-space-menu"))
            .w(px(250.))
            .p(px(6.))
            .flex()
            .flex_col()
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.menu = false;
                cx.notify();
            }))
            .child(terminal);
        let Some(id) = space.host().map(str::to_string) else { return menu.child(settings) };
        let reconnect = (self.head.reach == Reach::Offline).then(|| {
            let id = id.clone();
            ui::menu_row("space-reconnect", "reload", "Reconnect", None).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.menu = false;
                cx.emit(HeaderEvent::Reconnect(id.clone()));
                cx.notify();
            }))
        });
        let rename = {
            let id = id.clone();
            ui::menu_row("space-rename", "compose", "Rename\u{2026}", None)
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.start_rename(id.clone(), window, cx)))
        };
        let chosen_icon = self.head.glyph;
        let icons = HOST_ICONS.map(|g| {
            let id = id.clone();
            div()
                .id(SharedString::from(format!("space-icon-{g}")))
                .size(px(24.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .cursor_pointer()
                .hover(|d| d.bg(FILL_2))
                .when(chosen_icon == g, |d| d.bg(FILL_4))
                .child(icon(g, 14., if chosen_icon == g { TEXT } else { TEXT_2 }))
                .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(HeaderEvent::Icon { id: id.clone(), icon: g })))
        });
        let chosen = self.head.tint;
        let swatches = ACCENT_SWATCHES.map(|c| {
            let id = id.clone();
            div()
                .id(("space-colour", c as usize))
                .size(px(20.))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .cursor_pointer()
                .when(chosen == Some(c), |d| d.border_1().border_color(TEXT_3))
                .child(div().size(px(16.)).rounded_full().bg(rgb(c)))
                .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(HeaderEvent::Colour { id: id.clone(), colour: c })))
        });
        let remove = ui::danger_row("space-remove", "trash", "Remove host\u{2026}").on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
            this.menu = false;
            cx.emit(HeaderEvent::Remove { id: id.clone(), name: name.clone() });
        }));
        menu.children(reconnect)
            .child(rename)
            .child(settings)
            .child(ui::menu_divider())
            .child(ui::menu_label("Color"))
            .child(div().flex().gap(px(4.)).px(px(8.)).pb(px(6.)).children(swatches))
            .child(ui::menu_label("Icon"))
            .child(div().flex().gap(px(2.)).px(px(6.)).pb(px(6.)).children(icons))
            .child(ui::menu_divider())
            .child(remove)
    }
}

impl Render for SpaceHeader {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let open = self.menu;
        let host = self.head.space.host();
        let renaming = self.rename.as_ref().filter(|r| host == Some(r.id.as_str()));
        let name = match renaming {
            Some(r) => div()
                .flex_1()
                .min_w_0()
                .text_size(px(14.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(TEXT)
                .child(Input::new(&r.input).appearance(false).p_0().text_size(px(14.))),
            None => {
                div().min_w_0().max_w(px(120.)).truncate().text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(self.head.name.clone())
            }
        };
        let row = div()
            .id("aside-space")
            .h(px(34.))
            .mt(px(6.))
            .px(px(6.))
            .flex()
            .items_center()
            .gap(px(8.))
            .rounded(px(8.))
            .when(open, |d| d.bg(FILL_2))
            .child(tile(self.head.glyph, self.head.tint, 22.))
            .child(name)
            .children(self.head.dot.map(|dot| ui::dot(6., dot)));
        // While its name is edited, a press in the row places the caret rather than opening the menu.
        let row = match renaming {
            Some(_) => row,
            None => ui::popover_trigger(
                row.cursor_pointer()
                    .hover(|s| s.bg(FILL_2))
                    .child(div().flex_1().min_w_0().truncate().text_size(px(12.)).text_color(TEXT_3).child(self.head.address.clone()))
                    .child(icon("chevron-down", 12., TEXT_4)),
                open,
                cx.listener(|_, _: &MouseDownEvent, _, cx| cx.emit(HeaderEvent::OpenMenu)),
            ),
        };
        div().relative().flex_none().child(row).when(open, |d| d.child(ui::dropdown(40., ui::menu_in("aside-space-menu-in", self.space_menu(cx)))))
    }
}

impl Desktop {
    /// `space`'s colour: the one chosen for it, else the palette's by its place in the switcher.
    pub(crate) fn space_colour(&self, space: &Space, order: &[Space]) -> Option<u32> {
        colour(space, &self.store.sidebar.space_colours, order.iter().position(|s| s == space).unwrap_or_default())
    }

    /// `space`'s icon: the one chosen for it, else This Mac's or a server.
    pub(crate) fn space_glyph(&self, space: &Space) -> &'static str {
        glyph(space, &self.store.sidebar.space_icons)
    }

    pub(crate) fn space_header_set(&self, space: &Space, order: &[Space], cx: &mut Context<Self>) {
        let host = space.host().and_then(|id| self.fleet.hosts.list.iter().find(|h| h.id == id));
        let link = space.host().and_then(|id| self.fleet.hosts.links.get(id));
        let head = Head {
            space: space.clone(),
            name: self.space_name(space),
            glyph: self.space_glyph(space),
            tint: self.space_colour(space, order),
            address: host.map(|h| h.address()).unwrap_or_default(),
            dot: host.map(|_| link_color(link)),
            reach: self.space_reach(space),
            terminal: self.space_terminal_target(space).is_some() && self.space_reach(space) == Reach::Online,
        };
        self.sidebar.read(cx).space_header.clone().update(cx, |h, _| h.set(head));
    }

    pub(crate) fn on_space_header_event(&mut self, _: &Entity<SpaceHeader>, ev: &HeaderEvent, window: &mut Window, cx: &mut Context<Self>) {
        match ev {
            HeaderEvent::OpenMenu => {
                self.close_menus(cx);
                self.sidebar.read(cx).space_header.clone().update(cx, |h, cx| h.open(cx));
            }
            HeaderEvent::OpenTerminal => self.open_space_terminal(cx),
            HeaderEvent::HostSettings => self.show_hosts(window, cx),
            HeaderEvent::Reconnect(id) => self.retry_host(id),
            HeaderEvent::Rename { id, text } => {
                let kept = kept_name(text, &self.added_name(id));
                let names = &mut self.store.sidebar.space_names;
                match kept {
                    Some(name) => names.insert(id.clone(), name),
                    None => names.remove(id),
                };
                self.save_soon(cx);
            }
            HeaderEvent::Icon { id, icon } => {
                self.store.sidebar.space_icons.insert(id.clone(), icon.to_string());
                self.save_soon(cx);
            }
            HeaderEvent::Colour { id, colour } => {
                self.store.sidebar.space_colours.insert(id.clone(), *colour);
                self.save_soon(cx);
            }
            HeaderEvent::Remove { id, name } => {
                self.ask(Confirm::RemoveHost { id: id.clone(), name: name.clone() }, cx);
            }
        }
        cx.notify();
    }

    /// The project a terminal for `space` opens in: the one on screen if it is there, else the space's first.
    fn space_terminal_target(&self, space: &Space) -> Option<String> {
        self.project.clone().filter(|p| space_of(p) == *space).or_else(|| self.space_projects(space).into_iter().next())
    }

    /// A shell in the space's project; a machine's terminals live in its projects.
    pub(crate) fn open_space_terminal(&mut self, cx: &mut Context<Self>) {
        let Some(project) = self.space_terminal_target(&self.space()) else { return };
        let tree = self.worktree.clone().filter(|_| self.project.as_deref() == Some(project.as_str()));
        self.select_tree(project, tree, cx);
        self.new_shell(Place::Pane(None), cx);
    }
}
