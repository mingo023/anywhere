use super::filter::{FIELD_H, Filter};
use super::sections::section_head;
use gpui_kit::component::input::{Input, InputEvent};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use store::sections::Sort;
use theme::*;
use ui::{self, icon_button_sized};

const FILTER_IN: std::time::Duration = std::time::Duration::from_millis(220);

/// A menu of the Projects header.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum HeaderMenu {
    Add,
    Organize,
}

/// What the Projects header asks `Desktop` for.
pub(crate) enum HeaderEvent {
    /// A menu's button was pressed: the other menus shut, then it opens.
    Menu(HeaderMenu),
    OpenFilter,
    /// Up, down or Enter in the filter's field.
    FilterKey(&'static str),
    /// The filter's text changed.
    Filtered,
    /// The filter closed, so the folds a query held open are to shut.
    FilterClosed,
    NewGroup,
    Sort(Sort),
    /// Fold every project, or unfold them.
    FoldAll(bool),
    OpenFolder,
    CloneProject,
}

impl EventEmitter<HeaderEvent> for ProjectsHeader {}

/// What `Desktop` hands the header each frame.
#[derive(PartialEq)]
pub(crate) struct HeaderFrame {
    pub(crate) sort: Sort,
    /// The name of the host the space on screen is on, if it is on one.
    pub(crate) host: Option<String>,
}

/// The Projects header: its name and search button, or the filter's field while it's open, then the organize and add menus.
pub(crate) struct ProjectsHeader {
    pub(crate) filter: Filter,
    add_menu: bool,
    organize_menu: bool,
    frame: Option<HeaderFrame>,
    _sub: Subscription,
}

impl ProjectsHeader {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let filter = Filter::new(window, cx);
        let _sub = cx.subscribe_in(&filter.input, window, |this, _, ev: &InputEvent, window, cx| match ev {
            InputEvent::Change => cx.emit(HeaderEvent::Filtered),
            // A typed query stays, so its rows can still be clicked.
            InputEvent::Blur if this.filter.query(cx).is_empty() => this.close_filter(window, cx),
            _ => {}
        });
        Self { filter, add_menu: false, organize_menu: false, frame: None, _sub }
    }

    pub(crate) fn set(&mut self, frame: HeaderFrame, cx: &mut Context<Self>) {
        if self.frame.as_ref() != Some(&frame) {
            self.frame = Some(frame);
            crate::region::notify_after_draw(cx);
        }
    }

    pub(crate) fn menu_open(&self) -> bool {
        self.add_menu || self.organize_menu
    }

    pub(crate) fn show(&mut self, menu: HeaderMenu, cx: &mut Context<Self>) {
        match menu {
            HeaderMenu::Add => self.add_menu = true,
            HeaderMenu::Organize => self.organize_menu = true,
        }
        cx.notify();
    }

    pub(crate) fn shut(&mut self, cx: &mut Context<Self>) {
        if self.menu_open() {
            (self.add_menu, self.organize_menu) = (false, false);
            cx.notify();
        }
    }

    pub(crate) fn open_filter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.filter.open = true;
        self.filter.input.update(cx, |s, cx| s.focus(window, cx));
        cx.notify();
    }

    pub(crate) fn close_filter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.filter.open = false;
        self.filter.lit = None;
        self.filter.input.update(cx, |s, cx| s.set_value("", window, cx));
        window.blur(cx);
        cx.emit(HeaderEvent::FilterClosed);
        cx.notify();
    }

    fn on_filter_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        match ev.keystroke.key.as_str() {
            "up" => cx.emit(HeaderEvent::FilterKey("up")),
            "down" => cx.emit(HeaderEvent::FilterKey("down")),
            "enter" => cx.emit(HeaderEvent::FilterKey("enter")),
            "escape" if self.filter.query(cx).is_empty() => self.close_filter(window, cx),
            "escape" => {
                self.filter.input.update(cx, |s, cx| s.set_value("", window, cx));
                self.filter.lit = None;
            }
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }

    /// The filter's field, close button inside, growing leftwards out of the search button.
    fn filter_field(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let close = icon_button_sized("aside-filter-close", "x", 20., TEXT_3)
            .rounded(px(5.))
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.close_filter(window, cx)));
        div()
            .ml_auto()
            .mr(px(2.))
            .flex_1()
            .min_w_0()
            .h(px(FIELD_H))
            .pl(px(8.))
            .pr(px(4.))
            .flex()
            .items_center()
            .gap(px(6.))
            .rounded(px(8.))
            .bg(FILL_2)
            // A click on the field's padding or icon would otherwise focus the window's root, blurring and so closing an empty filter.
            .track_focus(&self.filter.input.focus_handle(cx))
            .capture_key_down(cx.listener(Self::on_filter_key))
            .child(icon("search", 13., TEXT_3))
            .child(Input::new(&self.filter.input).appearance(false).p_0().text_size(px(13.)))
            .child(close)
            .overflow_hidden()
            .with_animation("aside-filter-in", Animation::new(FILTER_IN).with_easing(|t: f32| 1. - (1. - t).powi(3)), |d, t| {
                d.min_w(px(28.)).max_w(relative(t)).opacity(t)
            })
    }

    /// The Projects add button and its menu: add a project, or clone one.
    fn add_menu(&self, cx: &mut Context<Self>) -> Div {
        let open = self.add_menu;
        let button = ui::popover_trigger(
            icon_button_sized("aside-add", "folder-plus", 22., TEXT_3).rounded(px(6.)),
            open,
            cx.listener(|_, _: &MouseDownEvent, _, cx| cx.emit(HeaderEvent::Menu(HeaderMenu::Add))),
        );
        div().relative().child(button).when(open, |d| {
            let host = self.frame.as_ref().and_then(|f| f.host.as_deref());
            let add = ui::menu_row("aside-add-project", "folder", &crate::spaces::on_host("Open folder", host), None)
                .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(HeaderEvent::OpenFolder)));
            let clone = ui::menu_row("aside-add-clone", "branch", &crate::spaces::on_host("Clone from URL", host), None)
                .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(HeaderEvent::CloneProject)));
            let menu = ui::pop(div().id("aside-add-menu"))
                .w(px(238.))
                .p(px(6.))
                .flex()
                .flex_col()
                .occlude()
                .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                    this.add_menu = false;
                    cx.notify();
                }))
                .child(add)
                .child(clone);
            d.child(ui::dropdown_right(26., ui::menu_in("aside-add-menu-in", menu)))
        })
    }

    /// The header's sort button and its menu: new group, the sort, folding every project.
    fn organize_menu(&self, cx: &mut Context<Self>) -> Div {
        let open = self.organize_menu;
        let button = ui::popover_trigger(
            icon_button_sized("aside-organize", "arrow-down-wide-narrow", 22., TEXT_3).rounded(px(6.)),
            open,
            cx.listener(|_, _: &MouseDownEvent, _, cx| cx.emit(HeaderEvent::Menu(HeaderMenu::Organize))),
        );
        div().relative().child(button).when(open, |d| {
            let new = ui::menu_row("aside-new-group", "folder-plus", "New group", None).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.organize_menu = false;
                cx.emit(HeaderEvent::NewGroup);
            }));
            let sort = self.frame.as_ref().map(|f| f.sort).unwrap_or_default();
            let sorts = [(Sort::Manual, "Manual"), (Sort::Name, "Name"), (Sort::Recent, "Recent activity")].map(|(s, label)| {
                ui::menu_pick(SharedString::from(format!("aside-sort-{label}")), div(), label, s == sort).on_click(cx.listener(
                    move |this, _: &ClickEvent, _, cx| {
                        this.organize_menu = false;
                        cx.emit(HeaderEvent::Sort(s));
                    },
                ))
            });
            let fold = |id: &'static str, label: &'static str, fold: bool| {
                ui::menu_row(id, if fold { "chevron-right" } else { "chevron-down" }, label, None).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.organize_menu = false;
                    cx.emit(HeaderEvent::FoldAll(fold));
                }))
            };
            let menu = ui::pop(div().id("aside-organize-menu"))
                .w(px(220.))
                .p(px(6.))
                .flex()
                .flex_col()
                .occlude()
                .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                    this.organize_menu = false;
                    cx.notify();
                }))
                .child(new)
                .child(ui::menu_divider())
                .child(ui::menu_label("Sort projects"))
                .children(sorts)
                .child(ui::menu_divider())
                .child(fold("aside-collapse-all", "Collapse all projects", true))
                .child(fold("aside-expand-all", "Expand all projects", false));
            d.child(ui::dropdown_right(26., ui::menu_in("aside-organize-menu-in", menu)))
        })
    }
}

impl Render for ProjectsHeader {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let header = section_head();
        let header = if self.filter.open {
            header.child(self.filter_field(cx))
        } else {
            let search = icon_button_sized("aside-filter", "search", 22., TEXT_3)
                .rounded(px(6.))
                .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(HeaderEvent::OpenFilter)));
            header.pl(px(6.)).child(div().flex_1().text_size(px(11.)).font_weight(FontWeight::BOLD).text_color(TEXT_3).child("PROJECTS")).child(search)
        };
        header.child(self.organize_menu(cx)).child(self.add_menu(cx))
    }
}
