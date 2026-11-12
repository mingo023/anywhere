use super::Menu;
use super::logic::{Surface, Field, LABELS, PRIORITIES, priority_name};
use super::parts::{label_dot, priority_glyph, prop_chip};
use super::values::Picks;
use crate::desktop::Desktop;
use crate::models::listed_agents;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;

impl Desktop {
    pub(super) fn picks(&self, surface: Surface) -> &Picks {
        match surface {
            Surface::Composer => &self.board.composer.picks,
            Surface::Page => &self.board.form.picks,
        }
    }

    fn picks_mut(&mut self, surface: Surface) -> &mut Picks {
        match surface {
            Surface::Composer => &mut self.board.composer.picks,
            Surface::Page => &mut self.board.form.picks,
        }
    }

    fn field_face(&self, surface: Surface, field: Field) -> (AnyElement, String) {
        let p = self.picks(surface);
        match field {
            Field::Project => (self.repo_tile_for(&p.values.folder, 14.).into_any_element(), self.repo_name(&p.values.folder)),
            Field::Agent => (theme::provider_icon(&p.values.provider, 13., TEXT_2).into_any_element(), self.agent_name(&p.values.provider)),
            Field::Priority => (priority_glyph(p.values.priority).into_any_element(), if p.values.priority == 0 && surface == Surface::Composer { "Priority".to_string() } else { priority_name(p.values.priority).to_string() }),
            Field::Labels => {
                let dots = div().flex().flex_none().items_center().gap(px(2.)).when(p.values.labels.is_empty(), |d| d.child(label_dot("Feature"))).children(p.values.labels.iter().map(|l| label_dot(l)));
                let empty = if surface == Surface::Composer { "Label" } else { "Add label" };
                (dots.into_any_element(), if p.values.labels.is_empty() { empty.to_string() } else { p.values.labels.join(", ") })
            }
            Field::Base => (icon("branch", 12., TEXT_3).into_any_element(), p.values.base_label(p.branches())),
        }
    }

    fn pick_project(&mut self, surface: Surface, project: String, cx: &mut Context<Self>) {
        self.picks_mut(surface).values.set_folder(project.clone());
        self.close_menus();
        self.load_ticket_branches(surface, project, cx);
        self.picked(surface, cx);
    }

    fn field_rows(&self, surface: Surface, field: Field, cx: &mut Context<Self>) -> Vec<Stateful<Div>> {
        let p = self.picks(surface);
        let pick = move |this: &mut Self, cx: &mut Context<Self>| {
            this.close_menus();
            this.picked(surface, cx);
        };
        match field {
            Field::Project => self
                .board_projects()
                .into_iter()
                .enumerate()
                .map(|(i, folder)| {
                    let chosen = folder == p.values.folder;
                    ui::menu_pick(("ticket-project-row", i), self.repo_tile_for(&folder, 14.), &self.repo_name(&folder), chosen).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.pick_project(surface, folder.clone(), cx)))
                })
                .collect(),
            Field::Agent => listed_agents(&self.offered(), &[&p.values.provider])
                .into_iter()
                .enumerate()
                .map(|(i, id)| {
                    let id = id.to_string();
                    self.agent_row(&id, ui::menu_pick(("ticket-agent-row", i), theme::provider_icon(&id, 13., TEXT_2), &self.agent_name(&id), id == p.values.provider)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.picks_mut(surface).values.provider = id.clone();
                        pick(this, cx);
                    }))
                })
                .collect(),
            Field::Priority => PRIORITIES
                .into_iter()
                .enumerate()
                .map(|(i, n)| {
                    ui::menu_pick(("ticket-priority-row", i), priority_glyph(n), priority_name(n), n == p.values.priority).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.picks_mut(surface).values.priority = n;
                        pick(this, cx);
                    }))
                })
                .collect(),
            Field::Labels => LABELS
                .into_iter()
                .enumerate()
                .map(|(i, label)| {
                    ui::menu_pick(("ticket-label-row", i), label_dot(label), label, p.values.labels.iter().any(|l| l == label)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.picks_mut(surface).values.toggle_label(label);
                        this.picked(surface, cx);
                    }))
                })
                .collect(),
            Field::Base => {
                let current = p.values.base_label(p.branches());
                p.branches()
                    .iter()
                    .enumerate()
                    .map(|(i, b)| {
                        let b = b.clone();
                        ui::menu_pick(("ticket-base-row", i), div(), &b, b == current).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.picks_mut(surface).values.base = b.clone();
                            pick(this, cx);
                        }))
                    })
                    .collect()
            }
        }
    }

    fn picked(&mut self, surface: Surface, cx: &mut Context<Self>) {
        if surface == Surface::Page {
            self.save_ticket_page_soon(cx);
        }
        cx.notify();
    }

    /// The chip that picks `field`, with its menu while open. The page's chip is outlined, the composer's plain.
    pub(super) fn field_chip(&self, surface: Surface, field: Field, cx: &mut Context<Self>) -> Div {
        let menu = Menu::Pick(surface, field);
        let open = self.board.menu.as_ref() == Some(&menu);
        let (lead, value) = self.field_face(surface, field);
        let id = SharedString::from(format!("ticket-chip-{surface:?}-{field:?}"));
        let chip = match surface {
            Surface::Composer => div().h(px(24.)).px(px(7.)).flex().flex_none().items_center().gap(px(5.)).max_w(px(170.)).rounded(px(7.)).text_size(px(12.)).text_color(TEXT_2).whitespace_nowrap(),
            Surface::Page => prop_chip(),
        };
        let chip = chip.id(id).cursor_pointer().hover(|s| s.bg(FILL_2).text_color(TEXT)).when(open, |d| d.bg(FILL_2).text_color(TEXT)).child(lead).child(div().min_w_0().truncate().child(value)).capture_any_mouse_down(cx.listener(move |this, _: &MouseDownEvent, _, cx| {
            cx.stop_propagation();
            this.toggle_board_menu(menu.clone(), cx);
        }));
        let popup = open.then(|| ui::dropdown(30., ui::menu_in("ticket-pick-in", self.popup("ticket-pick-menu", 240., cx).max_h(px(320.)).overflow_y_scroll().children(self.field_rows(surface, field, cx)))));
        div().relative().child(chip).children(popup)
    }
}
