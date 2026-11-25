use super::nav::root_of;
use crate::desktop::Desktop;
use crate::util::folder;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;

impl Desktop {
    /// Home, Root and the projects on this host, as `projects` lists their keys.
    pub(super) fn finder_places(&self, projects: &[String], cx: &mut Context<Self>) -> Div {
        let Some(f) = self.host_finder.finder.as_ref() else { return div() };
        let home = f.home.clone().unwrap_or_default();
        let projects: Vec<&str> = projects.iter().map(|k| hosts::split(k).1).collect();
        let place = |id: (&'static str, usize), glyph: &'static str, label: String, dir: String| {
            let on = f.dir == dir && f.file.is_none();
            div()
                .id(id)
                .h(px(28.))
                .px(px(8.))
                .flex()
                .items_center()
                .gap(px(8.))
                .rounded(px(7.))
                .text_size(px(13.))
                .cursor_pointer()
                .hover(|s| s.bg(FILL_2))
                .when(on, |d| d.bg(FILL_4))
                .child(icon(glyph, 14., TEXT_3))
                .child(div().truncate().child(label))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.finder_go(dir.clone(), cx)))
        };
        let heading = |label: String| {
            div().px(px(8.)).pt(px(10.)).pb(px(4.)).text_size(px(11.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT_3).child(label.to_uppercase())
        };
        div()
            .w(px(180.))
            .flex_none()
            .p(px(8.))
            .flex()
            .flex_col()
            .bg(FILL_1)
            .border_r(px(0.5))
            .border_color(SEPARATOR)
            .child(heading(self.host_name(&f.host)))
            .child(place(("finder-place", 0), "laptop", "Home".into(), home.clone()))
            .child(place(("finder-place", 1), "server", "Root".into(), root_of(&home)))
            .when(!projects.is_empty(), |d| d.child(heading("Projects here".into())))
            .children(projects.into_iter().enumerate().map(|(i, p)| place(("finder-project", i), "folder", folder(p).to_string(), p.to_string())))
    }
}
