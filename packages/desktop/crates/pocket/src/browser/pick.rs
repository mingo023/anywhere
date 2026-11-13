use super::Browser;
use crate::add_to_chat::default_target;
use crate::chat::Route;
use crate::composer::elements;
use crate::desktop::Desktop;
use gpui_kit::*;

/// The badges a page at `url` shows: its elements among `elements`, numbered as their chips are.
pub(crate) fn marks(url: &str, elements: &[web::Element]) -> Vec<(usize, String)> {
    elements.iter().enumerate().filter(|(_, e)| e.url == url).map(|(i, e)| (i + 1, e.selector.clone())).collect()
}

impl Desktop {
    pub(super) fn toggle_pick(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(b) = self.browsers.tabs.get_mut(&id) else { return };
        let Some(page) = &b.page else { return };
        let on = !b.picking;
        page.set_picking(on);
        if on {
            // Esc reaches the script only while the page holds the keys.
            page.take_keys();
        }
        b.picking = on;
        window.focus(&self.browsers.focus, cx);
        cx.notify();
    }

    /// While `b` picks, the elements of its page already in the chat its picks went to.
    pub(super) fn pick_marks(&self, b: &Browser, cx: &App) -> Vec<(usize, String)> {
        let draft = b.picked_into.as_deref().filter(|_| b.picking).and_then(|agent| self.chat_draft(agent, cx).1);
        draft.map_or_else(Vec::new, |d| marks(&b.url, &d.elements))
    }

    /// Puts an element picked on tab `id` in the chat a quote would go to; a TUI agent has it pasted.
    pub(super) fn pick_into_chat(&mut self, id: u64, element: web::Element, cx: &mut Context<Self>) {
        if self.agents.observe_only() {
            return;
        }
        let choices = self.chat_choices();
        let target = self.chat_target(&choices).or_else(|| default_target(None, self.session(), &choices));
        let Some((agent, terminal, provider)) = target.and_then(|t| self.agents.get(&t).map(|a| (t.clone(), a.terminal_id.clone(), a.provider.clone()))) else {
            self.error = Some("No agent here to send the element to.".into());
            return cx.notify();
        };
        match self.route(&terminal) {
            Route::Chat => {
                if !self.chat_draft_mut(&agent).pick_element(element) {
                    return;
                }
                if let Some(b) = self.browsers.tabs.get_mut(&id) {
                    b.picked_into = Some(agent);
                }
            }
            Route::Paste => self.send_input(&terminal, &term::paste_bytes(&elements::block(&[element]), true), cx),
            Route::Blocked => {
                self.error = Some("This agent can't take pasted text right now.".into());
                return cx.notify();
            }
        }
        self.flash_added(provider, cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::marks;

    fn at(url: &str, selector: &str) -> web::Element {
        web::Element { url: url.into(), selector: selector.into(), name: String::new(), text: String::new(), html: String::new(), width: 0, height: 0 }
    }

    #[test]
    fn a_page_badges_only_its_own_elements_numbered_as_their_chips() {
        let elements = [at("https://a.dev/", "#one"), at("https://b.dev/", "#two"), at("https://a.dev/", "#three")];
        assert_eq!(marks("https://a.dev/", &elements), vec![(1, "#one".to_string()), (3, "#three".to_string())]);
        assert_eq!(marks("https://c.dev/", &elements), vec![]);
    }
}
