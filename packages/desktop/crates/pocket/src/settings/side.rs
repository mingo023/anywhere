use super::Section;
use crate::accounts::Key;
use crate::desktop::Desktop;
use crate::desktop::chrome::Overlay;
use crate::models::Models;
use crate::themes::Custom;
use crate::updates::Update;
use crate::util::initials;
use git::github::GhStatus;
use gpui_kit::App;
use gpui_kit::Div;
use gpui_kit::SharedString;
use gpui_kit::{Entity, component::input::InputState};
use hosts::checklist::Checklist;
use std::sync::Arc;

/// What `Desktop` hands Settings each frame besides the store.
pub(crate) struct Side {
    pub(crate) update: Update,
    pub(crate) updates_available: bool,
    /// The star card was answered with a star.
    pub(crate) thanked: bool,
    pub(crate) link_down: bool,
    /// pocketd's service can be restarted from here.
    pub(crate) service: bool,
    pub(crate) themes: Arc<Vec<Custom>>,
    pub(crate) theme_edits: usize,
    pub(crate) theme_error: Option<SharedString>,
    /// The projects but Scratch folders, in the sidebar's order.
    pub(crate) projects: Vec<ProjectCard>,
    pub(crate) models: Models,
    pub(crate) hosts: HostsSide,
    pub(crate) accounts: AccountsSide,
    pub(crate) automations: Arc<agents::automations::Automations>,
    pub(crate) can_open_automations: bool,
    /// Pairing a phone is on offer.
    pub(crate) can_pair: bool,
    /// Whether pocketd listens on a tailnet, once it has said.
    pub(crate) tailnet: Option<bool>,
    pub(crate) gh: GhSide,
}

impl Default for Side {
    fn default() -> Self {
        Self {
            update: Update::Idle,
            updates_available: false,
            thanked: false,
            link_down: false,
            service: false,
            themes: Arc::default(),
            theme_edits: 0,
            theme_error: None,
            projects: Vec::new(),
            models: Models::default(),
            hosts: HostsSide::default(),
            accounts: AccountsSide::default(),
            automations: Arc::default(),
            can_open_automations: false,
            can_pair: false,
            tailnet: None,
            gh: GhSide::default(),
        }
    }
}

/// A project as Settings lists it.
pub(crate) struct ProjectCard {
    pub(crate) path: String,
    pub(crate) name: String,
    pub(crate) mark: ui::Mark,
    pub(crate) color: u32,
    pub(crate) worktrees: Option<usize>,
    /// The account each provider's sessions in it use, where the project picks one.
    pub(crate) picks: Vec<(String, String)>,
}

impl ProjectCard {
    pub(crate) fn tile(&self, size: f32) -> Div {
        ui::colour_tile(&self.mark, self.color, size, size / 4.)
    }
}

/// What the Git page reads of the GitHub CLI and the repository on screen.
#[derive(Default)]
pub(crate) struct GhSide {
    /// `None` until `gh` has been looked for, then `Some(None)` while it is being checked.
    pub(crate) status: Option<Option<GhStatus>>,
    /// A folder is on screen to run `gh auth login` in.
    pub(crate) terminal: bool,
    /// One per project, once `gh` has more than one account to choose from.
    pub(crate) rows: Vec<GhAccountRow>,
    pub(crate) remotes: Vec<String>,
}

pub(crate) struct GhAccountRow {
    /// Where the project sits in `Side::projects`.
    pub(crate) card: usize,
    pub(crate) chosen: Option<String>,
    pub(crate) checking: bool,
    pub(crate) active: String,
    pub(crate) accounts: Vec<String>,
}

/// One SSH host as the Hosts page lists it.
pub(crate) struct HostRow {
    pub(crate) host: hosts::Host,
    pub(crate) link: Option<agents::hosts::Link>,
    /// The name its space goes by.
    pub(crate) shown: String,
    pub(crate) glyph: &'static str,
    pub(crate) colour: Option<u32>,
    pub(crate) install: Option<Checklist>,
    /// What the row says instead of its link, as while its service is uninstalled.
    pub(crate) notice: Option<String>,
}

pub(crate) struct AddFormSide {
    pub(crate) open: bool,
    pub(crate) address: Entity<InputState>,
    pub(crate) port: Entity<InputState>,
    pub(crate) user: Entity<InputState>,
    pub(crate) password: Entity<InputState>,
    pub(crate) key_file: Option<String>,
    pub(crate) error: Option<String>,
    pub(crate) suggestions: Vec<String>,
    pub(crate) picked: Option<String>,
}

pub(crate) struct PasswordSide {
    pub(crate) host: Option<String>,
    pub(crate) input: Entity<InputState>,
    pub(crate) error: Option<String>,
    pub(crate) signing_in: bool,
}

/// What the Hosts page reads of the fleet.
#[derive(Default)]
pub(crate) struct HostsSide {
    /// An older pocketd runs here, which links no hosts.
    pub(crate) read_only: bool,
    /// The names of the hosts the desktop linked itself before pocketd did.
    pub(crate) saved: Vec<String>,
    pub(crate) rows: Vec<HostRow>,
    /// `None` until `Desktop` has handed the fleet down once.
    pub(crate) form: Option<AddFormSide>,
    pub(crate) password: Option<PasswordSide>,
}

/// What the Accounts page reads of the accounts pocketd lists.
#[derive(Default)]
pub(crate) struct AccountsSide {
    pub(crate) checking: bool,
    pub(crate) checked_at: i64,
    /// The account whose menu is open.
    pub(crate) menu: Option<Key>,
    pub(crate) rename: Option<(Key, Entity<InputState>)>,
    pub(crate) machines: Vec<MachineSide>,
}

pub(crate) struct MachineSide {
    pub(crate) host: crate::accounts::Host,
    /// "This Mac", else the host's name.
    pub(crate) title: String,
    /// "this Mac", else the host's name, to sit inside a sentence.
    pub(crate) name: String,
    /// `None` where the machine's pocketd is too old to switch.
    pub(crate) auto_switch: Option<bool>,
    pub(crate) list: Vec<daemon::Account>,
    pub(crate) signins: Vec<SignInSide>,
}

/// A sign-in running on a machine, with the lines its terminal shows.
pub(crate) struct SignInSide {
    pub(crate) provider: String,
    pub(crate) code: Entity<InputState>,
    pub(crate) output_open: bool,
    pub(crate) lines: Vec<String>,
}

impl Desktop {
    /// What Settings draws that the store doesn't hold; only the pages on screen, or all while searching, pay for their parts.
    pub(super) fn settings_side(&self, cx: &App) -> Side {
        let settings = self.settings.read(cx);
        let everything = !settings.query(cx).is_empty();
        let section = settings.section;
        let on = |s: Section| everything || section == s;
        let projects = if on(Section::Projects) || on(Section::Git) || on(Section::Browser) {
            self.store.without_scratch(self.store.projects.clone())
        } else {
            Vec::new()
        };
        Side {
            update: self.updates.update.clone(),
            updates_available: self.updates.available(),
            thanked: self.star.read(cx).thanked,
            link_down: self.terminals.read(cx).link.is_down(),
            service: self.terminals.read(cx).link.service.is_some(),
            themes: self.themes.custom.clone(),
            theme_edits: self.themes.edits,
            theme_error: self.themes.error.clone(),
            projects: projects.iter().map(|p| self.project_card(p, cx)).collect(),
            models: self.models.clone(),
            hosts: if on(Section::Hosts) { self.hosts_side() } else { HostsSide::default() },
            accounts: if on(Section::Accounts) { self.accounts_side(cx) } else { AccountsSide::default() },
            automations: self.agents.automations.clone(),
            can_open_automations: self.agents.automations_offered(),
            can_pair: crate::modals::may_open(Overlay::PairPhone, &self.agents),
            tailnet: self.agents.host.as_ref().map(|h| h.tailnet),
            gh: if on(Section::Git) { self.gh_side(&projects, cx) } else { GhSide::default() },
        }
    }

    fn project_card(&self, path: &str, cx: &App) -> ProjectCard {
        let name = self.repo_name(path);
        let picks = self
            .store
            .repos
            .get(path)
            .map(|r| crate::accounts::project_picks(self.accounts.list(&crate::accounts::host_of(Some(path))), &r.accounts))
            .unwrap_or_default();
        ProjectCard {
            path: path.to_string(),
            mark: self.project_mark(path, initials(&name)),
            name,
            color: self.repo_color(path, cx),
            worktrees: self.worktrees.get(path).map(Vec::len),
            picks,
        }
    }

    fn gh_side(&self, projects: &[String], cx: &App) -> GhSide {
        let rows = if self.gh_accounts.any_choice() {
            projects
                .iter()
                .enumerate()
                .map(|(card, project)| {
                    let chosen = self.gh_accounts.chosen(&self.store.repos, project).map(str::to_string);
                    let signed = self.gh_accounts.accounts_of(project);
                    GhAccountRow {
                        card,
                        checking: chosen.is_none() && self.gh_accounts.detecting(project),
                        chosen,
                        active: signed.map_or("", |s| s.active.as_str()).to_string(),
                        accounts: signed.map(|s| s.accounts.clone()).unwrap_or_default(),
                    }
                })
                .collect()
        } else {
            Vec::new()
        };
        GhSide { status: self.prs.gh.clone(), terminal: self.cwd(cx).is_some(), rows, remotes: self.repo(cx).map(|r| r.remotes.clone()).unwrap_or_default() }
    }

    fn hosts_side(&self) -> HostsSide {
        let order = self.space_order();
        let rows = self
            .fleet
            .hosts
            .list
            .iter()
            .map(|h| {
                let space = crate::spaces::Space::Host(h.id.clone());
                HostRow {
                    link: self.fleet.hosts.links.get(&h.id).cloned(),
                    shown: self.host_name(&h.id),
                    glyph: self.space_glyph(&space),
                    colour: self.space_colour(&space, &order),
                    install: self.fleet.installs.get(&h.id).cloned(),
                    notice: self.fleet.notices.get(&h.id).cloned(),
                    host: h.clone(),
                }
            })
            .collect();
        let form = &self.fleet.form;
        let password = &self.fleet.password;
        HostsSide {
            read_only: self.hosts_read_only(),
            saved: self.store.hosts.iter().map(|h| h["name"].as_str().unwrap_or_default().to_string()).collect(),
            rows,
            form: Some(AddFormSide {
                open: form.open,
                address: form.address.clone(),
                port: form.port.clone(),
                user: form.user.clone(),
                password: form.password.clone(),
                key_file: form.key_file.clone(),
                error: form.error.clone(),
                suggestions: form.suggestions.clone(),
                picked: form.picked.clone(),
            }),
            password: Some(PasswordSide {
                host: password.host.clone(),
                input: password.input.clone(),
                error: password.error.clone(),
                signing_in: password.signing_in,
            }),
        }
    }

    fn accounts_side(&self, cx: &App) -> AccountsSide {
        let accounts = &self.accounts;
        let machines = std::iter::once(None)
            .chain(self.fleet.hosts.list.iter().map(|h| Some(h.id.clone())))
            .filter(|h| accounts.lists.contains_key(h))
            .map(|host| MachineSide {
                title: host.as_deref().map_or_else(|| "This Mac".into(), |id| self.host_name(id)),
                name: self.machine_name(&host),
                auto_switch: accounts.auto_switch.get(&host).copied(),
                list: accounts.list(&host).to_vec(),
                signins: accounts.signins.iter().filter(|s| s.key.host == host).map(|s| self.sign_in_side(s, cx)).collect(),
                host,
            })
            .collect();
        AccountsSide {
            checking: !accounts.checking.is_empty(),
            checked_at: accounts.checked_at,
            menu: accounts.menu.clone(),
            rename: accounts.rename.as_ref().map(|r| (r.key.clone(), r.input.clone())),
            machines,
        }
    }

    fn sign_in_side(&self, s: &crate::accounts::SignIn, cx: &App) -> SignInSide {
        let lines = s
            .terminal
            .as_deref()
            .and_then(|id| {
                self.terminals.read(cx).sessions.term(id).map(|mut t| {
                    let (f, cells) = t.frame();
                    crate::accounts::screen_lines(cells, f.cols)
                })
            })
            .unwrap_or_default();
        SignInSide { provider: s.key.provider.clone(), code: s.code.clone(), output_open: s.output_open, lines }
    }
}
