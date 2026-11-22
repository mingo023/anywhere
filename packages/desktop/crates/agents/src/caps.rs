pub(super) const PAIR_CAP: &str = "pair.v1";
pub(super) const OPEN_CAP: &str = "open.v1";
pub(super) const NAMES_CAP: &str = "names.v1";
pub(super) const AUTOMATIONS_CAP: &str = "automations.v1";
pub(super) const BOARD_CAP: &str = "board.v1";
pub(super) const LOCALS_CAP: &str = "locals.v1";
pub(super) const FORK_CAP: &str = "fork.v1";
pub(super) const ACCOUNTS_CAP: &str = "accounts.v1";
pub(super) const SETTINGS_CAP: &str = "settings.v1";
pub(super) const ACCESS_CAP: &str = "access.v1";
pub(super) const ORCHESTRATOR_CAP: &str = "orchestrator.v1";
pub(super) const HOSTS_CAP: &str = "hosts.v1";
pub(super) const REGISTRY_CAP: &str = "registry.v2";

/// What this client understands beyond protocol 3.
pub(super) const CAPS: [&str; 19] = [
    PAIR_CAP,
    "scopes.v1",
    "summary.v2",
    "host.v1",
    OPEN_CAP,
    NAMES_CAP,
    AUTOMATIONS_CAP,
    BOARD_CAP,
    LOCALS_CAP,
    FORK_CAP,
    ACCOUNTS_CAP,
    "subagents.v1",
    SETTINGS_CAP,
    "commands.v1",
    ACCESS_CAP,
    "skills.v1",
    ORCHESTRATOR_CAP,
    HOSTS_CAP,
    REGISTRY_CAP,
];
