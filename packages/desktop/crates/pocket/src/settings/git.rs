use super::catalog::{Look, Setting};
use super::dropdown::dropdown;
use super::models::{model_label, model_options};
use super::{action, row};
use crate::desktop::Desktop;
use git::github::GhStatus;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use store::Store;
use store::prefs::git::Style;
use theme::*;
use workspace::Place;

const AGENTS: [&str; 2] = ["claude", "codex"];
const STYLES: [Style; 4] = [Style::Recent, Style::Conventional, Style::Plain, Style::Custom];

fn index<T: PartialEq>(all: &[T], value: &T) -> usize {
    all.iter().position(|v| v == value).unwrap_or(0)
}

pub(super) const ROWS: &[Setting] = &[
    Setting::custom("gh", "GitHub", "GitHub CLI", |d, _, cx| d.gh_row(cx)),
    Setting::custom("remote", "Repository", "Default remote", |d, s, cx| d.remote_row(s, cx))
        .resets(|s| s.git.remote.clone(), |s, d| s.git.remote = d.git.remote.clone()).hint("Where Push sends commits"),
    Setting::switch("untracked", "Repository", "Show untracked files", |s| s.git.untracked, |s, on| s.git.untracked = on)
        .hint("In Changes and in the file counts")
        .effect(|d, cx| d.refresh_git(cx)),
    Setting::choice("commit-all", "Committing", "If nothing is staged", &["Commit all changes", "Staged only"], Look::Segmented, |s| usize::from(!s.git.commit_all), |s, i| s.git.commit_all = i == 0)
        .hint("Staged only stops and asks you to stage first"),
    Setting::switch("force-with-lease", "Committing", "Force push with lease", |s| s.git.force_with_lease, |s, on| s.git.force_with_lease = on)
        .hint("Push replaces a rebased or amended branch, using --force-with-lease so you never overwrite commits you haven\u{2019}t fetched")
        .advanced(),
    Setting::switch("ai", "AI commit messages", "Write commit messages with AI", |s| s.git.ai, |s, on| s.git.ai = on)
        .hint("Drafts a message from the changes to commit. You can edit it before committing"),
    Setting::choice("ai-agent", "AI commit messages", "Agent", &["Claude Code", "Codex"], Look::Dropdown, |s| index(&AGENTS, &s.git.agent.as_str()), |s, i| s.git.set_agent(AGENTS[i]))
        .offers(super::agents::enabled_provider)
        .under(|s| s.git.ai),
    Setting::custom("ai-model", "AI commit messages", "Model", |d, s, cx| d.ai_model_row(s, cx))
        .resets(|s| model_label(&s.git.model), |s, d| s.git.model = d.git.model.clone())
        .under(|s| s.git.ai),
    Setting::choice("ai-style", "AI commit messages", "Style", &["Recent", "Conventional", "Plain", "Custom"], Look::Segmented, |s| index(&STYLES, &s.git.style), |s, i| s.git.style = STYLES[i])
        .hint("Recent follows this repo's latest subjects")
        .under(|s| s.git.ai),
    Setting::text("ai-prompt", "AI commit messages", "Custom prompt", "Write a commit message for the diff on stdin", |s| s.git.prompt.clone(), |s, t| s.git.prompt = t)
        .under(|s: &Store| s.git.ai && s.git.style == Style::Custom),
    Setting::switch("pr-draft", "Pull requests", "Create as draft", |s| s.git.draft, |s, on| s.git.draft = on),
    Setting::choice("pr-browser", "Pull requests", "Open pull requests in", &["In-app browser", "System browser"], Look::Segmented, |s| usize::from(!s.git.in_app), |s, i| s.git.in_app = i == 0),
];

/// The status line's dot, title and detail.
fn gh_line(status: Option<&GhStatus>) -> (Option<Token>, &'static str, String) {
    match status {
        None => (None, "Checking for GitHub CLI…", "Looking for gh on your PATH".into()),
        Some(GhStatus::Ready { version, login }) => (Some(SUCCESS), "GitHub CLI is ready", format!("gh {version} · signed in as {login} on github.com")),
        Some(GhStatus::SignedOut { version }) => (Some(WAITING_DOT), "GitHub CLI isn't signed in", format!("gh {version} · sign in once and Anywhere uses the same account")),
        Some(GhStatus::Missing) => (Some(FAILED), "GitHub CLI isn't installed", "Pull requests, checks and review status need gh. Git itself still works.".into()),
    }
}

/// A shell command to copy.
fn command(id: &'static str, text: &'static str, cx: &mut Context<Desktop>) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(26.))
        .px(px(9.))
        .flex()
        .items_center()
        .gap(px(8.))
        .rounded(px(7.))
        .bg(FILL_2)
        .cursor_pointer()
        .hover(|d| d.bg(FILL_3))
        .font_family(MONO)
        .text_size(px(12.))
        .child(div().text_color(TEXT_4).child("$"))
        .child(div().text_color(TEXT).child(text))
        .child(icon("copy", 12., TEXT_3))
        .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(text.into()))))
}

impl Desktop {
    /// The commit agent's models: Claude Code's aliases, or what Codex lists.
    fn ai_model_row(&mut self, setting: &'static Setting, cx: &mut Context<Self>) -> Div {
        let model = self.store.git.model.clone();
        let options = model_options(&self.store.git.agent, self.codex_models(), &model, "Agent default");
        let shown = options.iter().find(|(v, _)| *v == model).map_or_else(|| model_label(&model), |(_, l)| l.clone());
        let trigger = action(setting.id, "").child(shown).gap(px(6.)).child(icon("chevron-down", 12., TEXT_3));
        let title = div().flex().items_center().gap(px(10.)).child(div().w(px(16.)).text_color(TEXT_4).child("↳")).child(setting.label);
        row(title, setting.hint, dropdown(setting.id, trigger, options, &model, self, |d, v, _| d.store.git.model = v, cx))
    }

    fn gh_row(&mut self, cx: &mut Context<Self>) -> Div {
        let status = self.prs.gh.clone().flatten();
        let (dot, title, detail) = gh_line(status.as_ref());
        let fix = match status {
            Some(GhStatus::Missing) => Some(
                div().flex().items_center().gap(px(10.)).child(command("gh-install", "brew install gh", cx)).child(
                    ui::link("gh-install-other", "Other ways to install").on_click(|_: &ClickEvent, _: &mut Window, cx: &mut App| cx.open_url("https://github.com/cli/cli#installation")),
                ),
            ),
            Some(GhStatus::SignedOut { .. }) => Some(div().flex().items_center().gap(px(10.)).child(command("gh-login", "gh auth login", cx)).when(self.cwd().is_some(), |d| {
                d.child(ui::link("gh-login-terminal", "Open in terminal").on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                    let argv = ["gh", "auth", "login"].map(String::from);
                    this.close_settings(window, cx);
                    this.run_in_tree(Place::Pane(None), |cwd| daemon::agent_op(&argv, cwd), cx);
                })))
            })),
            _ => None,
        };
        let title = div()
            .flex()
            .flex_col()
            .gap(px(1.))
            .child(div().flex().items_center().gap(px(8.)).children(dot.map(|c| ui::dot(8., c))).child(title))
            .child(div().text_size(px(12.)).line_height(px(16.)).font_weight(FontWeight::NORMAL).text_color(TEXT_3).child(detail))
            .children(fix.map(|f| f.mt(px(8.))));
        let checking = self.prs.gh == Some(None);
        let again = action("gh-check", "Check again").when(checking, |d| d.opacity(0.5)).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.check_gh(cx)));
        row(title, None, again)
    }

    /// A dropdown of the remotes the repository on screen has.
    fn remote_row(&mut self, setting: &'static Setting, cx: &mut Context<Self>) -> Div {
        let chosen = self.store.git.remote.clone();
        let mut remotes = self.repo().map(|r| r.remotes.clone()).unwrap_or_default();
        if !remotes.contains(&chosen) {
            remotes.insert(0, chosen.clone());
        }
        let open = self.settings.menu == Some(setting.id);
        let button = action(setting.id, "").gap(px(6.)).child(div().font_family(MONO).text_size(px(12.)).child(chosen.clone())).child(icon("chevron-down", 12., TEXT_3)).on_click(cx.listener(
            move |this, _: &ClickEvent, _, cx| {
                this.settings.menu = (!open).then_some(setting.id);
                cx.notify();
            },
        ));
        let rows = remotes.into_iter().enumerate().map(|(i, name)| {
            div()
                .id(("remote-option", i))
                .h(px(26.))
                .px(px(8.))
                .flex()
                .items_center()
                .gap(px(6.))
                .rounded(px(5.))
                .cursor_pointer()
                .hover(|d| d.bg(FILL_2))
                .text_size(px(13.))
                .text_color(TEXT)
                .child(div().w(px(14.)).flex_none().when(name == chosen, |d| d.child(icon("check", 13., TEXT))))
                .child(div().font_family(MONO).text_size(px(12.)).child(name.clone()))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.settings.menu = None;
                    this.store.git.remote = name.clone();
                    this.save_soon(cx);
                    cx.notify();
                }))
        });
        let menu = ui::menu_in("remote-menu-in", ui::pop(div()).min_w(px(160.)).p(px(4.)).flex().flex_col().children(rows));
        row(setting.label, setting.hint, div().relative().child(button).when(open, |d| d.child(ui::dropdown_right(30., menu))))
    }
}

#[cfg(test)]
mod tests {
    use super::{gh_line, index, AGENTS, STYLES};
    use git::github::GhStatus;
    use store::prefs::git::Style;

    #[test]
    fn the_gh_line_names_the_account_or_what_is_missing() {
        let ready = GhStatus::Ready { version: "2.81.0".into(), login: "minh-ngo".into() };
        assert_eq!(gh_line(Some(&ready)).2, "gh 2.81.0 · signed in as minh-ngo on github.com");
        assert_eq!(gh_line(Some(&GhStatus::Missing)).1, "GitHub CLI isn't installed");
        assert_eq!(gh_line(None).1, "Checking for GitHub CLI…");
    }

    #[test]
    fn an_agent_or_style_the_list_lacks_reads_as_the_first_option() {
        assert_eq!((index(&AGENTS, &"codex"), index(&AGENTS, &"gpt-9")), (1, 0));
        assert_eq!(index(&STYLES, &Style::Plain), 2);
    }
}
