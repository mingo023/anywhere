use super::CONTEXT;
use crate::actions::{Back, FocusAddress, Forward, PageEdit, Reload};
use crate::desktop::Desktop;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::rc::Rc;
use std::sync::Arc;
use theme::*;
use workspace::tree::PaneId;

/// A pane and what keeps its events coming.
pub(crate) struct Slot {
    view: Entity<BrowserPane>,
    _sub: Subscription,
}

/// What `Desktop` hands a pane each time it draws it.
struct Look {
    url: String,
    back: bool,
    forward: bool,
    picking: bool,
    can_pick: bool,
    /// The tab's page, while it may be placed.
    page: Option<Rc<web::Page>>,
    shot: Option<Arc<RenderImage>>,
    /// Set when the pane has the window's browser focus.
    focus: Option<FocusHandle>,
}

pub(crate) enum BrowserEvent {
    /// Enter pressed in the address bar over `text`.
    Browse(String),
    /// The pick button pressed.
    Pick,
}

/// One pane's browser tab: its toolbar and address bar over the page, which WebKit draws above GPUI.
pub(crate) struct BrowserPane {
    address: Entity<InputState>,
    look: Option<Look>,
    _sub: Subscription,
}

impl EventEmitter<BrowserEvent> for BrowserPane {}

impl BrowserPane {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let address = cx.new(|cx| InputState::new(window, cx).placeholder("Search or enter address"));
        let sub = cx.subscribe_in(&address, window, |_, address, ev: &InputEvent, _, cx| {
            let text = address.read(cx).value();
            if let InputEvent::PressEnter { secondary: false, .. } = ev
                && !text.trim().is_empty()
            {
                cx.emit(BrowserEvent::Browse(text.to_string()));
            }
        });
        Self { address, look: None, _sub: sub }
    }

    pub(crate) fn focus_address(&mut self, url: String, window: &mut Window, cx: &mut Context<Self>) {
        self.address.update(cx, |s, cx| {
            s.set_value(url, window, cx);
            s.focus(window, cx);
            s.select_all(window, cx);
        });
    }

    fn page(&self) -> Option<&web::Page> {
        self.look.as_ref()?.page.as_deref()
    }
}

impl Render for BrowserPane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(look) = self.look.take() else { return div().flex_1() };
        let address = self.address.clone();
        if !address.focus_handle(cx).is_focused(window) && *address.read(cx).value() != *look.url {
            address.update(cx, |s, cx| s.set_value(look.url.clone(), window, cx));
        }
        let nav = |id: &'static str, name: &str, on: bool| ui::icon_button(id, name).when(!on, |d| d.opacity(0.35));
        let url = look.url.clone();
        let bar = div()
            .h(px(40.))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(2.))
            .px(px(8.))
            .border_b(px(0.5))
            .border_color(SEPARATOR)
            .child(nav("browser-back", "back", look.back).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.back(&Back, window, cx))))
            .child(nav("browser-forward", "forward", look.forward).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.forward(&Forward, window, cx))))
            .child(ui::icon_button("browser-reload", "reload").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.reload(&Reload, window, cx))))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h(px(28.))
                    .mx(px(6.))
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .rounded(px(7.))
                    .bg(FILL_2)
                    .text_size(px(13.))
                    .child(Input::new(&address).appearance(false).p_0().text_size(px(13.))),
            )
            .child(
                ui::icon_button("browser-pick", "pointer")
                    .when(look.picking, |d| d.bg(FILL_3))
                    .when(!look.can_pick, |d| d.opacity(0.35))
                    .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(BrowserEvent::Pick))),
            )
            .child(ui::icon_button("browser-external", "external").on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                if !url.is_empty() {
                    cx.open_url(&url);
                }
            })));
        let (page, focus, focused) = (look.page.clone(), look.focus.clone(), look.focus.is_some());
        let shot = look.shot.clone();
        self.look = Some(look);
        let place = canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                let bounds = window.unzoomed(bounds);
                let rect =
                    web::Rect { x: bounds.origin.x.into(), y: bounds.origin.y.into(), width: bounds.size.width.into(), height: bounds.size.height.into() };
                if let Some(page) = &page
                    && page.place(rect)
                    && focused
                    && focus.as_ref().is_some_and(|f| f.is_focused(window))
                {
                    page.take_keys();
                }
            },
        )
        .absolute()
        .size_full();
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(SURFACE)
            .key_context(CONTEXT)
            .when_some(self.look.as_ref().and_then(|l| l.focus.clone()), |d, focus| d.track_focus(&focus))
            .on_action(cx.listener(Self::focus))
            .on_action(cx.listener(Self::reload))
            .on_action(cx.listener(Self::back))
            .on_action(cx.listener(Self::forward))
            .on_action(cx.listener(Self::page_edit))
            .child(bar)
            .child(div().flex_1().min_h_0().relative().child(place).when_some(shot, |d, shot| d.child(img(shot).absolute().inset_0().size_full().opacity(0.6))))
    }
}

impl BrowserPane {
    fn focus(&mut self, _: &FocusAddress, window: &mut Window, cx: &mut Context<Self>) {
        let Some(url) = self.look.as_ref().map(|l| l.url.clone()) else { return };
        if let Some(page) = self.page() {
            page.give_keys();
        }
        self.focus_address(url, window, cx);
    }

    fn reload(&mut self, _: &Reload, _: &mut Window, _: &mut Context<Self>) {
        if let Some(page) = self.page() {
            page.reload();
        }
    }

    fn back(&mut self, _: &Back, _: &mut Window, _: &mut Context<Self>) {
        if let Some(page) = self.page() {
            page.back();
        }
    }

    fn forward(&mut self, _: &Forward, _: &mut Window, _: &mut Context<Self>) {
        if let Some(page) = self.page() {
            page.forward();
        }
    }

    fn page_edit(&mut self, edit: &PageEdit, _: &mut Window, _: &mut Context<Self>) {
        if let Some(page) = self.page() {
            page.edit(edit.0);
        }
    }
}

impl Desktop {
    pub(crate) fn browser_view(&mut self, pane: PaneId, id: u64, window: &mut Window, cx: &mut Context<Self>) -> Div {
        if !self.browsers.tabs.contains_key(&id) {
            return div().flex_1();
        }
        let view = self.browser_entity(pane, window, cx);
        let b = &self.browsers.tabs[&id];
        if let Some(page) = &b.page {
            page.set_marks(&self.pick_marks(b, cx));
        }
        let look = Look {
            url: b.url.clone(),
            back: b.page.as_ref().is_some_and(|p| p.can_go_back()),
            forward: b.page.as_ref().is_some_and(|p| p.can_go_forward()),
            picking: b.picking,
            can_pick: b.page.is_some(),
            page: b.page.clone().filter(|_| self.browsers.shown.contains(&id)),
            shot: b.shot.clone(),
            focus: (pane == self.focused_pane()).then(|| self.browsers.focus.clone()),
        };
        view.update(cx, |p, _| p.look = Some(look));
        div().flex_1().min_h_0().flex().flex_col().child(view)
    }

    pub(crate) fn browser_entity(&mut self, pane: PaneId, window: &mut Window, cx: &mut Context<Self>) -> Entity<BrowserPane> {
        if let Some(slot) = self.browsers.panes.get(&pane) {
            return slot.view.clone();
        }
        let view = cx.new(|cx| BrowserPane::new(window, cx));
        let sub = cx.subscribe_in(&view, window, move |this, _, ev, window, cx| this.on_browser_pane(pane, ev, window, cx));
        self.browsers.panes.insert(pane, Slot { view: view.clone(), _sub: sub });
        view
    }

    fn on_browser_pane(&mut self, pane: PaneId, ev: &BrowserEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.web_tab(pane, cx) else { return };
        match ev {
            BrowserEvent::Browse(text) => self.browse(id, web::resolve(text, self.store.browser.search_url()), window, cx),
            BrowserEvent::Pick => self.toggle_pick(id, window, cx),
        }
    }
}
