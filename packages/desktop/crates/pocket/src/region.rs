use crate::desktop::Desktop;
use gpui_kit::*;

type Draw = Box<dyn Fn(&mut Desktop, &mut Window, &mut Context<Desktop>) -> AnyElement>;

/// A part of the window drawn as its own cached view: a redraw that starts in another part, as a chat's scroll
/// does, leaves it as last drawn. It redraws when anything it read is notified, `Desktop` included.
pub(crate) struct Region {
    desktop: WeakEntity<Desktop>,
    draw: Draw,
}

impl Region {
    pub(crate) fn new(cx: &mut Context<Desktop>, draw: impl Fn(&mut Desktop, &mut Window, &mut Context<Desktop>) -> AnyElement + 'static) -> Entity<Self> {
        let desktop = cx.weak_entity();
        cx.new(|_| Self { desktop, draw: Box::new(draw) })
    }
}

impl Render for Region {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let draw = &self.draw;
        div().size_full().flex().flex_col().children(self.desktop.update(cx, |d, cx| draw(d, window, cx)).ok())
    }
}

/// `region` in a box `style` sizes: a cached view is laid out without its content, so the box can't fit to it.
pub(crate) fn cached(region: &Entity<Region>, style: StyleRefinement) -> AnyElement {
    AnyView::from(region.clone()).cached(style).into_any_element()
}

/// A full-height column `width` wide.
pub(crate) fn column(width: f32) -> StyleRefinement {
    StyleRefinement::default().w(px(width)).flex_none().h_full()
}

/// The columns either side of the page.
pub(crate) struct Regions {
    pub(crate) aside: Entity<Region>,
    pub(crate) nav: Entity<Region>,
    pub(crate) panel: Entity<Region>,
    pub(crate) rail: Entity<Region>,
}

impl Regions {
    pub(crate) fn new(cx: &mut Context<Desktop>) -> Self {
        Self {
            aside: Region::new(cx, |d, _, cx| d.aside(cx).into_any_element()),
            nav: Region::new(cx, |d, _, cx| d.nav(cx).into_any_element()),
            panel: Region::new(cx, |d, _, cx| d.workspace_panel(cx).into_any_element()),
            rail: Region::new(cx, |d, _, cx| d.workspace_rail(cx).into_any_element()),
        }
    }
}
