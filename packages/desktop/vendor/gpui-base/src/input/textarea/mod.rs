use std::rc::Rc;

use gpui::{App, Entity, IntoElement, RenderOnce, Window};

use super::{InputBaseState, TextDecoration, TextareaMode};

/// State for editing ordinary multi-line text.
///
/// This is the shared editing engine in its multi-line kind. Code-editor
/// facilities such as languages, diagnostics, folding, and LSP do not exist on
/// this type — those methods live on [`super::EditorState`].
pub type TextareaState = InputBaseState<TextareaMode>;

impl TextareaState {
    /// Paint the decorations `decorate` finds in the text, after every edit.
    pub fn decorate(mut self, decorate: impl Fn(&str) -> Vec<TextDecoration> + 'static) -> Self {
        self.extras.decorations = decorate(&self.text.to_string());
        self.extras.decorate = Some(Rc::new(decorate));
        self
    }
}

/// An unstyled ordinary multi-line text input.
#[derive(IntoElement)]
pub struct Textarea {
    state: Entity<TextareaState>,
}

impl Textarea {
    pub fn new(state: &Entity<TextareaState>) -> Self {
        Self {
            state: state.clone(),
        }
    }
}

impl RenderOnce for Textarea {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        self.state
    }
}
