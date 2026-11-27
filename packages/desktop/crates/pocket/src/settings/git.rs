use super::catalog::{Look, Setting};
use super::dropdown::{self, dropdown};
use super::models::model_rows;
use super::projects::{Entry, Line, lines, machine_heading};
use super::side::Machine;
use super::{Settings, SettingsEvent, action, row};
use crate::composer::model_menu::listed_name;
use crate::models::model_or_default;
use git::github::GhStatus;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::HashSet;
use store::Store;
use store::prefs::git::Style;
use theme::*;

pub(super) const ACCOUNT_MENU: &str = "gh-account";
const AGENTS: [&str; agents::providers::COUNT] = agents::providers::IDS;
const STYLES: [Style; 4] = [Style::Recent, Style::Conventional, Style::Plain, Style::Custom];

/// Whether the `i`th agent can write a commit message: offered, and run once without a session.
fn writes_commits(s: &Store, live: &agents::Agents, i: usize) -> bool {
    agents::providers::ALL[i].oneshot.is_some() && super::agents::offered_provider(s, live, i)
}

fn index<T: PartialEq>(all: &[T], value: &T) -> usize {
    all.iter().position(|v| v == value).unwrap_or(0)
}

pub(super) const ROWS: &[Setting] = &[
    Setting::custom("gh", "GitHub", "GitHub CLI", |d, s, cx| d.gh_row(s, cx)).bare(),
    Setting::custom("remote", "Repository", "Default remote", |d, s, cx| d.remote_row(s, cx))
        .resets(|s| s.git.remote.clone(), |s, d| s.git.remote = d.git.remote.clone())
        .hint("Where Push sends commits"),
    Setting::switch("untracked", "Repository", "Show untracked files", |s| s.git.untracked, |s, on| s.git.untracked = on)
        .hint("In Changes and in the file counts")
        .effect(|d, cx| d.refresh_git(cx)),
    Setting::choice(
        "commit-all",
        "Committing",
        "If nothing is staged",
        &["Commit all changes", "Staged only"],
        Look::Segmented,
        |s| usize::from(!s.git.commit_all),
        |s, i| s.git.commit_all = i == 0,
    )
    .hint("Staged only stops and asks you to stage first"),
    Setting::switch("force-with-lease", "Committing", "Force push with lease", |s| s.git.force_with_lease, |s, on| s.git.force_with_lease = on)
        .hint("Push replaces a rebased or amended branch, using --force-with-lease so you never overwrite commits you haven\u{2019}t fetched")
        .advanced(),
    Setting::switch("ai", "AI commit messages", "Write commit messages with AI", |s| s.git.ai, |s, on| s.git.ai = on)
        .hint("Drafts a message from the changes to commit. You can edit it before committing"),
    Setting::choice(
        "ai-agent",
        "AI commit messages",
        "Agent",
        &PROVIDER_NAMES,
        Look::Dropdown,
        |s| index(&AGENTS, &s.git.agent.as_str()),
        |s, i| s.git.set_agent(AGENTS[i]),
    )
    .offers(writes_commits)
    .under(|s| s.git.ai),
    Setting::custom("ai-model", "AI commit messages", "Model", |d, s, cx| d.ai_model_row(s, cx))
        .resets(|s| model_or_default(&s.git.model), |s, d| s.git.model = d.git.model.clone())
        .under(|s| s.git.ai),
    Setting::choice(
        "ai-style",
        "AI commit messages",
        "Style",
        &["Recent", "Conventional", "Plain", "Custom"],
        Look::Segmented,
        |s| index(&STYLES, &s.git.style),
        |s, i| s.git.style = STYLES[i],
    )
    .hint("Recent follows this repo's latest subjects")
    .under(|s| s.git.ai),
    Setting::text(
        "ai-prompt",
        "AI commit messages",
        "Custom prompt",
        "Write a commit message for the diff on stdin",
        |s| s.git.prompt.clone(),
        |s, t| s.git.prompt = t,
    )
    .under(|s: &Store| s.git.ai && s.git.style == Style::Custom),
    Setting::switch("pr-draft", "Pull requests", "Create as draft", |s| s.git.draft, |s, on| s.git.draft = on),
    Setting::choice(
        "pr-browser",
        "Pull requests",
        "Open pull requests in",
        &["In-app browser", "System browser"],
        Look::Segmented,
        |s| usize::from(!s.git.in_app),
        |s, i| s.git.in_app = i == 0,
    ),
];

/// The status line's dot, title and detail.
fn gh_line(status: Option<&GhStatus>) -> (Option<Token>, &'static str, String) {
    match status {
        None => (None, "Checking for GitHub CLI…", "Looking for gh on your PATH".into()),
        Some(GhStatus::Ready { version, login, accounts }) if accounts.len() > 1 => {
            (Some(SUCCESS), "GitHub CLI is ready", format!("gh {version} · {} accounts on github.com, {login} active", accounts.len()))
        }
        Some(GhStatus::Ready { version, login, .. }) => (Some(SUCCESS), "GitHub CLI is ready", format!("gh {version} · signed in as {login} on github.com")),
        Some(GhStatus::SignedOut { version }) => {
            (Some(WAITING_DOT), "GitHub CLI isn't signed in", format!("gh {version} · sign in once and Zoes uses the same account"))
        }
        Some(GhStatus::Missing) => {
            (Some(FAILED), "GitHub CLI isn't installed", "Pull requests, checks and review status need gh. Git itself still works.".into())
        }
    }
}

fn account_lines(hosts: &[Option<&str>], machines: &[Machine]) -> Vec<Line> {
    let entries: Vec<Entry> = hosts.iter().map(|&host| Entry { name: "", path: "", host }).collect();
    let holds_a_project = |l: &Line| !matches!(l, Line::Header { total: 0, .. });
    lines(&entries, machines, &HashSet::new(), "").into_iter().filter(holds_a_project).collect()
}

/// A shell command to copy.
fn command(id: &'static str, text: &'static str, cx: &mut Context<Settings>) -> Stateful<Div> {
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

impl Settings {
    /// The commit agent's models, as its CLI lists them.
    fn ai_model_row(&mut self, setting: &'static Setting, cx: &mut Context<Self>) -> Div {
        let model = self.store.git.model.clone();
        let (agent, offered) = (self.store.git.agent.as_str(), self.offered_models(&self.store.git.agent));
        let trigger = action(setting.id, "").child(listed_name(&model, offered)).gap(px(6.)).child(icon("chevron-down", 12., TEXT_3));
        let title = super::sub_title(setting.label);
        let from = (agent, self.side.models.revision, self.installed(agent), &model);
        row(
            title,
            setting.hint,
            dropdown(
                setting.id,
                trigger,
                from,
                || model_rows(agent, offered, &model),
                &model,
                self,
                move |d, v, cx| d.set(setting, cx, move |s| s.git.model = v.clone()),
                cx,
            ),
        )
    }

    fn gh_row(&mut self, setting: &'static Setting, cx: &mut Context<Self>) -> Div {
        let status = self.side.gh.status.clone().flatten();
        let (dot, title, detail) = gh_line(status.as_ref());
        let fix = match status {
            Some(GhStatus::Missing) => Some(
                div().flex().items_center().gap(px(10.)).child(command("gh-install", "brew install gh", cx)).child(
                    ui::link("gh-install-other", "Other ways to install")
                        .on_click(|_: &ClickEvent, _: &mut Window, cx: &mut App| cx.open_url("https://github.com/cli/cli#installation")),
                ),
            ),
            Some(GhStatus::SignedOut { .. }) => {
                Some(div().flex().items_center().gap(px(10.)).child(command("gh-login", "gh auth login", cx)).when(self.side.gh.terminal, |d| {
                    d.child(ui::link("gh-login-terminal", "Open in terminal").on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(SettingsEvent::GhLogin))))
                }))
            }
            _ => None,
        };
        let title = div()
            .flex()
            .flex_col()
            .gap(px(1.))
            .child(div().flex().items_center().gap(px(8.)).children(dot.map(|c| ui::dot(8., c))).child(title))
            .child(div().text_size(px(12.)).line_height(px(16.)).font_weight(FontWeight::NORMAL).text_color(TEXT_3).child(detail))
            .children(fix.map(|f| f.mt(px(8.))));
        let checking = self.side.gh.status == Some(None);
        let again = action("gh-check", "Check again")
            .when(checking, |d| d.opacity(0.5))
            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(SettingsEvent::CheckGh)));
        // The cards are opaque, so the search highlight `setting_row` paints behind them has to be on the row itself.
        let status = row(title, None, again).when(self.found == Some(setting.id), |d| d.bg(ACCENT_TINT));
        if self.side.gh.rows.is_empty() {
            return super::card(vec![status]);
        }
        let caption = div()
            .px(px(14.))
            .py(px(9.))
            .text_size(px(12.))
            .text_color(TEXT_3)
            .child("Each project's pull requests and new terminals use its account. Zoes picks the one with the most access to the repo.");
        let hosts: Vec<Option<&str>> = self.side.gh.rows.iter().map(|r| self.side.projects[r.card].host.as_deref()).collect();
        let lines = account_lines(&hosts, &self.side.machines);
        if !lines.iter().any(|l| matches!(l, Line::Header { .. })) {
            let rows: Vec<Div> = (0..self.side.gh.rows.len()).map(|i| self.gh_account_row(i, cx)).collect();
            let caption = caption.border_t(px(0.5)).border_b(px(0.5)).border_color(SEPARATOR);
            return super::card(vec![div().flex().flex_col().child(status).child(caption).children(rows)]);
        }
        let mut blocks = vec![super::card(vec![status, caption])];
        let mut at = 0;
        while let Some(Line::Header { host, total, .. }) = lines.get(at) {
            at += 1;
            let mut rows = Vec::new();
            while let Some(Line::Project { card }) = lines.get(at) {
                rows.push(self.gh_account_row(*card, cx));
                at += 1;
            }
            let header = div().h(px(24.)).ml(px(2.)).flex().items_center().child(machine_heading(host, self.machine(host), *total));
            blocks.push(div().flex().flex_col().gap(px(6.)).child(header).child(super::card(vec![div().flex().flex_col().children(rows)])));
        }
        div().flex().flex_col().gap(px(14.)).children(blocks)
    }

    fn gh_account_row(&mut self, i: usize, cx: &mut Context<Self>) -> Div {
        let row = &self.side.gh.rows[i];
        let card = &self.side.projects[row.card];
        let (project, chosen, checking) = (card.path.clone(), row.chosen.clone(), row.checking);
        let open = self.menu == Some(ACCOUNT_MENU) && self.account_menu.as_deref() == Some(project.as_str());
        let shown = match (&chosen, checking) {
            (Some(account), _) => div().font_family(MONO).text_size(px(12.)).text_color(TEXT).child(account.clone()),
            (None, true) => div().text_color(TEXT_3).child("Checking accounts…"),
            (None, false) => div().text_color(TEXT_3).child(format!("Active account ({})", row.active)),
        };
        let toggle = project.clone();
        let button = action(("gh-account", i), "").gap(px(6.)).child(shown).child(icon("chevron-down", 12., TEXT_3));
        let button = dropdown::trigger_with(button, open, move |this| (this.menu, this.account_menu) = (Some(ACCOUNT_MENU), Some(toggle.clone())), cx);
        let menu = open.then(|| {
            let options = row.accounts.clone().into_iter().enumerate().map(|(j, account)| {
                let project = project.clone();
                div()
                    .id(("gh-account-option", j))
                    .h(px(26.))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .rounded(px(5.))
                    .cursor_pointer()
                    .hover(|d| d.bg(FILL_2))
                    .text_color(TEXT)
                    .child(div().w(px(14.)).flex_none().when(chosen.as_ref() == Some(&account), |d| d.child(icon("check", 13., TEXT))))
                    .child(div().font_family(MONO).text_size(px(12.)).child(account.clone()))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        (this.menu, this.account_menu) = (None, None);
                        cx.emit(SettingsEvent::PickGhAccount { project: project.clone(), account: account.clone() });
                        cx.notify();
                    }))
            });
            let options: Vec<_> = options.collect();
            ui::menu_in(
                SharedString::from(format!("gh-account-menu-in-{i}")),
                dropdown::shuts(ui::pop(div()), ACCOUNT_MENU, cx).min_w(px(180.)).max_h(px(320.)).p(px(4.)).flex().flex_col().children(options),
            )
        });
        div()
            .px(px(14.))
            .py(px(7.))
            .flex()
            .items_center()
            .gap(px(10.))
            .border_b(px(0.5))
            .border_color(SEPARATOR)
            .child(card.tile(20.))
            .child(div().flex_1().min_w_0().truncate().text_size(px(13.)).text_color(TEXT).child(card.name.clone()))
            .child(div().relative().child(button).children(menu.map(|m| ui::dropdown_right(30., m))))
    }

    /// A dropdown of the remotes the repository on screen has.
    fn remote_row(&mut self, setting: &'static Setting, cx: &mut Context<Self>) -> Div {
        let chosen = self.store.git.remote.clone();
        let mut remotes = self.side.gh.remotes.clone();
        if !remotes.contains(&chosen) {
            remotes.insert(0, chosen.clone());
        }
        let open = self.menu == Some(setting.id);
        let button =
            action(setting.id, "").gap(px(6.)).child(div().font_family(MONO).text_size(px(12.)).child(chosen.clone())).child(icon("chevron-down", 12., TEXT_3));
        let button = dropdown::trigger(button, setting.id, open, cx);
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
                    this.menu = None;
                    let name = name.clone();
                    this.set(setting, cx, move |s| s.git.remote = name.clone());
                }))
        });
        let rows: Vec<_> = rows.collect();
        let menu = ui::menu_in("remote-menu-in", dropdown::shuts(ui::pop(div()), setting.id, cx).min_w(px(160.)).p(px(4.)).flex().flex_col().children(rows));
        row(setting.label, setting.hint, div().relative().child(button).when(open, |d| d.child(ui::dropdown_right(30., menu))))
    }
}

#[cfg(test)]
mod tests {
    use super::{AGENTS, STYLES, account_lines, gh_line, index, writes_commits};
    use crate::settings::projects::{Line, MachineState};
    use crate::settings::side::Machine;
    use git::github::GhStatus;
    use store::prefs::git::Style;

    #[test]
    fn only_an_offered_agent_that_runs_once_writes_commit_messages() {
        let mut store = store::Store::default();
        let installed = AGENTS.map(|id| agents::providers::ProviderInfo { id: id.into(), available: true, ..Default::default() }).to_vec();
        let live = agents::Agents { providers: installed, ..Default::default() };
        let listed = |s: &store::Store| AGENTS.into_iter().enumerate().filter(|(i, _)| writes_commits(s, &live, *i)).map(|(_, id)| id).collect::<Vec<_>>();
        assert_eq!(listed(&store), ["claude", "codex"]);
        store.agents.disable("codex", &live);
        assert_eq!(listed(&store), ["claude"]);
    }

    #[test]
    fn the_gh_line_names_the_account_or_what_is_missing() {
        let ready = |accounts: &[&str]| GhStatus::Ready {
            version: "2.81.0".into(),
            login: "minh-ngo".into(),
            accounts: accounts.iter().map(|a| a.to_string()).collect(),
        };
        assert_eq!(gh_line(Some(&ready(&["minh-ngo"]))).2, "gh 2.81.0 · signed in as minh-ngo on github.com");
        assert_eq!(gh_line(Some(&ready(&["minh-ngo", "work"]))).2, "gh 2.81.0 · 2 accounts on github.com, minh-ngo active");
        assert_eq!(gh_line(Some(&GhStatus::Missing)).1, "GitHub CLI isn't installed");
        assert_eq!(gh_line(None).1, "Checking for GitHub CLI…");
    }

    fn machines() -> Vec<Machine> {
        let machine = |host: Option<&str>, state| Machine {
            host: host.map(str::to_string),
            name: host.unwrap_or("This Mac").into(),
            glyph: "server",
            colour: None,
            state,
        };
        vec![machine(None, MachineState::Local), machine(Some("build"), MachineState::Connected), machine(Some("mini"), MachineState::Offline)]
    }

    fn header(host: Option<&str>, total: usize) -> Line {
        Line::Header { host: host.map(str::to_string), open: true, total }
    }

    #[test]
    fn accounts_on_one_machine_stay_a_flat_list() {
        assert_eq!(account_lines(&[None, None], &machines()), [Line::Project { card: 0 }, Line::Project { card: 1 }]);
    }

    #[test]
    fn accounts_on_several_machines_group_under_each_machine_in_its_order_skipping_machines_without_projects() {
        assert_eq!(
            account_lines(&[Some("mini"), None, Some("mini"), None], &machines()),
            [
                header(None, 2),
                Line::Project { card: 1 },
                Line::Project { card: 3 },
                header(Some("mini"), 2),
                Line::Project { card: 0 },
                Line::Project { card: 2 }
            ]
        );
    }

    #[test]
    fn an_account_on_a_host_pocketd_no_longer_lists_still_gets_a_group() {
        assert_eq!(
            account_lines(&[None, Some("old")], &machines()),
            [header(None, 1), Line::Project { card: 0 }, header(Some("old"), 1), Line::Project { card: 1 }]
        );
    }

    #[test]
    fn an_agent_or_style_the_list_lacks_reads_as_the_first_option() {
        assert_eq!((index(&AGENTS, &"codex"), index(&AGENTS, &"gpt-9")), (1, 0));
        assert_eq!(index(&STYLES, &Style::Plain), 2);
    }
}
