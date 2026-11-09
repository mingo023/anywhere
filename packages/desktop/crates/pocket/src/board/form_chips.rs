use super::Menu;
use crate::desktop::Desktop;
use agents::providers;
use gpui_kit::*;
use theme::*;

/// A chip's three element ids: its own, its popup's and its popup's entrance.
type ChipIds = [&'static str; 3];

/// What a chip shows: its mark and its value.
fn face(lead: AnyElement, value: String) -> [AnyElement; 2] {
    [lead, div().font_weight(FontWeight::MEDIUM).child(value).into_any_element()]
}

impl Desktop {
    fn form_pick(&self, [chip_id, popup_id, in_id]: ChipIds, menu: Menu, face: [AnyElement; 2], width: f32, rows: Vec<Stateful<Div>>, cx: &mut Context<Self>) -> Div {
        let open = self.board.menu.as_ref() == Some(&menu);
        let chip = ui::menu_chip(chip_id, open).children(face).child(icon("chevron-down", 12., TEXT_4)).capture_any_mouse_down(cx.listener(move |this, _: &MouseDownEvent, _, cx| {
            cx.stop_propagation();
            this.toggle_board_menu(menu.clone(), cx);
        }));
        let popup = open.then(|| ui::dropdown(36., ui::menu_in(in_id, self.popup(popup_id, width, cx).max_h(px(320.)).overflow_y_scroll().children(rows))));
        div().relative().child(chip).children(popup)
    }

    /// The project of a new ticket; an existing one stays where it was made.
    pub(super) fn form_project_chip(&self, cx: &mut Context<Self>) -> AnyElement {
        let f = &self.board.form;
        let tile = self.repo_tile_for(&f.v.folder, 16.).into_any_element();
        if f.v.editing() {
            return div().flex().items_center().gap(px(6.)).children(face(tile, self.repo_name(&f.v.folder))).into_any_element();
        }
        let rows = self
            .projects()
            .into_iter()
            .enumerate()
            .map(|(i, p)| {
                ui::menu_pick(("ticket-project-row", i), self.repo_tile_for(&p, 14.), &self.repo_name(&p), p == f.v.folder).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.board.form.v.set_folder(p.clone());
                    this.close_menus();
                    this.load_ticket_branches(p.clone(), cx);
                    cx.notify();
                }))
            })
            .collect();
        self.form_pick(["ticket-project", "ticket-project-menu", "ticket-project-in"], Menu::FormProject, face(tile, self.repo_name(&f.v.folder)), 260., rows, cx).into_any_element()
    }

    pub(super) fn form_agent_chip(&self, cx: &mut Context<Self>) -> Div {
        let f = &self.board.form;
        let rows = providers::IDS
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let p = p.to_string();
                ui::menu_pick(("ticket-agent-row", i), theme::provider_icon(&p, 13., TEXT_2), &self.agent_name(&p), p == f.v.provider).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.board.form.v.provider = p.clone();
                    this.close_menus();
                    cx.notify();
                }))
            })
            .collect();
        let lead = theme::provider_icon(&f.v.provider, 13., TEXT_2).into_any_element();
        self.form_pick(["ticket-agent", "ticket-agent-menu", "ticket-agent-in"], Menu::FormAgent, face(lead, self.agent_name(&f.v.provider)), 200., rows, cx)
    }

    pub(super) fn form_base_chip(&self, cx: &mut Context<Self>) -> Div {
        let f = &self.board.form;
        let current = f.v.base_label(f.branches());
        let rows = f
            .branches()
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let b = b.clone();
                ui::menu_pick(("ticket-base-row", i), div(), &b, b == current).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.board.form.v.base = b.clone();
                    this.close_menus();
                    cx.notify();
                }))
            })
            .collect();
        self.form_pick(["ticket-base", "ticket-base-menu", "ticket-base-in"], Menu::FormBase, face(icon("branch", 13., TEXT_3).into_any_element(), current), 260., rows, cx)
    }
}
