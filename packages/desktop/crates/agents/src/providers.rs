use std::sync::LazyLock;

use serde::Deserialize;

use crate::catalogs;

/// What pocketd says one provider takes, from `agent.providers`.
#[derive(Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct ProviderInfo {
    pub id: String,
    /// Its CLI is installed where pocketd runs.
    pub available: bool,
    pub efforts: Vec<String>,
    pub plan: bool,
    /// `None` from a pocketd older than the field.
    pub chat: Option<ChatCaps>,
    /// `None` when pocketd doesn't say: it leaves out a false, and one older than the field says nothing.
    pub never_asks: Option<bool>,
}

impl ProviderInfo {
    /// Whether it runs every tool without asking, so it only runs with full access: pocketd's word, else this build's.
    pub fn never_asks(&self) -> bool {
        self.never_asks.unwrap_or_else(|| get(&self.id).is_some_and(|b| b.never_asks))
    }
}

/// What a provider's chat driver does, as pocketd says.
#[derive(Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct ChatCaps {
    pub interrupt: bool,
    pub compact: bool,
    pub send_while_working: bool,
    pub sub_agents: bool,
    pub send_now: bool,
}

/// Where a catalog's `input` takes the folder the CLI runs in, as a JSON string.
pub const CWD: &str = "{cwd}";

/// How a provider's CLI lists its models.
pub struct Catalog {
    /// Run instead of the provider's command; empty runs that.
    pub command: &'static str,
    /// Run after the command.
    pub args: &'static [&'static str],
    pub input: &'static str,
    /// The line that answers, from a server that runs on after it: `daemon::run_login_until` stops there. `None` when
    /// the CLI exits by itself, as `daemon::run_login` waits for.
    pub answer: Option<fn(&str) -> bool>,
    parse: fn(&str) -> Vec<crate::ListedModel>,
    /// Offered, as they are, until the CLI answers or when it can't.
    pub fallback: &'static [&'static str],
}

impl Catalog {
    const NONE: Self = Self { command: "", args: &[], input: "", answer: None, parse: |_| Vec::new(), fallback: &[] };

    /// The models `output` lists, or what a server answered instead of a list, as "Authentication required".
    pub fn read(&self, output: &str) -> Result<Vec<crate::ListedModel>, String> {
        let refused = self.answer.and_then(|answer| output.lines().find(|l| answer(l))).and_then(catalogs::refusal);
        refused.map_or_else(|| Ok((self.parse)(output)), Err)
    }

    /// `input` for a CLI run in `cwd`.
    pub fn input_in(&self, cwd: &str) -> String {
        self.input.replace(CWD, &serde_json::Value::from(cwd).to_string())
    }
}

/// How the AI commit message runs the agent once.
pub struct Oneshot {
    pub args: &'static [&'static str],
    pub model_flag: &'static str,
    pub model: &'static str,
}

/// How pocketd drives an agent's chat; the chat crate has an adapter for each.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transport {
    Claude,
    Codex,
    /// The Agent Client Protocol.
    Acp,
    /// Pi's RPC mode.
    Pi,
}

/// How to get an agent pocketd didn't find.
pub struct Install {
    /// The shell command that installs it; empty when this build knows none.
    pub command: &'static str,
    /// What is left to do once it's installed, in plain words, as "run agy once to sign in"; empty when nothing is.
    pub then: &'static str,
}

impl Install {
    const NONE: Self = Self { command: "", then: "" };
}

/// What this build knows of a provider without pocketd.
pub struct Builtin {
    pub id: &'static str,
    /// What runs it in a terminal when Settings names no command.
    pub command: &'static str,
    pub efforts: &'static [&'static str],
    /// What the agent calls its effort.
    pub effort_word: &'static str,
    pub plan: bool,
    pub models: Catalog,
    /// pocketd's defaults for the words before a first prompt, before the id it resumes, and that fork a session; its
    /// providers package holds the same. Empty where the CLI takes none.
    pub prompt_args: &'static str,
    pub resume_args: &'static str,
    pub fork_args: &'static str,
    pub status_via: &'static str,
    pub unattached: &'static str,
    pub pastes_code: bool,
    /// `None` for an agent that writes no commit messages.
    pub oneshot: Option<Oneshot>,
    pub install: Install,
    /// Every pocketd runs it, so it counts as installed until pocketd says what is. The others wait for that word: a
    /// pocketd older than them never gives it.
    pub assumed_installed: bool,
    /// Anywhere reads its status from a chat alone, so a session started with a prompt is always a chat.
    pub chat_only_status: bool,
    /// It runs every tool without asking, so it only runs with full access; `ProviderInfo::never_asks` has pocketd's word.
    pub never_asks: bool,
    /// Its models come from other makers, so a list groups them under who serves each: `ListedModel::upstream`.
    pub by_upstream: bool,
    pub transport: Transport,
    /// The slash commands of an agent driven over ACP or Pi's RPC that only its terminal runs, by name and alias: a
    /// chat would send them to the model as a prompt. Claude's and Codex's adapters hold their own.
    pub tui_only: &'static [&'static str],
}

/// Pi's and omp's thinking levels.
pub(crate) const THINKING: &[&str] = &["off", "minimal", "low", "medium", "high", "xhigh", "max"];
pub(crate) const GROK_EFFORTS: &[&str] = &["low", "medium", "high", "xhigh"];

/// Opens an ACP agent and a session in `CWD`, whose reply lists its models.
const ACP_INPUT: &str = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":1,"clientCapabilities":{"fs":{"readTextFile":false,"writeTextFile":false},"terminal":false,"session":{"configOptions":{"boolean":{}}}},"clientInfo":{"name":"anywhere","version":"0"}}}
{"jsonrpc":"2.0","id":2,"method":"session/new","params":{"cwd":{cwd},"mcpServers":[]}}"#;

/// Antigravity's ACP server is a file of its own that `agy` installs, and it finds its resources from the folder it runs in.
const AGY_ACP: &str = r#"for p in ~/.local/bin/agy_acp_server.par ~/.local/share/agy-acp/agy_acp_server.par "$(command -v agy_acp_server.par)"; do [ -x "$p" ] && cd "$(dirname "$p")" && exec "$p" "$@"; done; exit 127"#;
#[cfg(not(target_os = "linux"))]
const AGY_ARGS: &[&str] = &["-c", AGY_ACP, "sh"];
#[cfg(target_os = "linux")]
const AGY_ARGS: &[&str] = &["-c", AGY_ACP, "sh", "--uid="];

/// What the agents after Claude and Codex share: Anywhere reads no hooks or session files of theirs, so only a chat reports status.
const NEWER: Builtin = Builtin {
    id: "",
    command: "",
    efforts: &[],
    effort_word: "Effort",
    plan: false,
    models: Catalog::NONE,
    prompt_args: "",
    resume_args: "",
    fork_args: "",
    status_via: "in a chat only, reads its events",
    unattached: "This agent reports status in a chat only, so Anywhere sees this terminal's activity but not its status.",
    pastes_code: false,
    oneshot: None,
    install: Install::NONE,
    assumed_installed: false,
    chat_only_status: true,
    never_asks: false,
    by_upstream: false,
    transport: Transport::Acp,
    tui_only: &[],
};

/// OpenCode 1.19's TUI commands; its ACP server lists only skills, `init` and `review`.
const OPENCODE_TUI_ONLY: &[&str] = &[
    "agents", "clear", "compact", "connect", "continue", "copy", "debug", "diff", "editor", "exit", "export", "fork", "help", "mcps", "mo", "models", "move", "org",
    "orgs", "q", "quit", "redo", "rename", "resume", "sessions", "skills", "slash", "status", "summarize", "switch-org", "themes", "thinking", "timeline",
    "timestamps", "toggle-thinking", "toggle-timestamps", "undo", "update", "variants", "warp", "workspaces",
];

/// Pi 1.1's built-in commands but the ones a chat runs itself (`BUILTIN_SLASH_COMMANDS`), and `llama`, which its own
/// extension answers only in its TUI. omp has none: it lists what its RPC runs.
const PI_TUI_ONLY: &[&str] = &[
    "settings", "tree", "scoped-models", "export", "import", "share", "bug", "copy", "name", "session", "changelog", "hotkeys", "fork", "clone", "trust", "login",
    "logout", "resume", "reload", "quit", "llama",
];

/// The commands Hermes marks `cli_only`, but `tools`, which its ACP server answers.
const HERMES_TUI_ONLY: &[&str] = &[
    "clear", "redraw", "history", "prompt", "compose", "handoff", "worktree", "snapshot", "snap", "export", "import", "journey", "learning", "memory-graph",
    "config", "statusbar", "sb", "battery", "timestamps", "ts", "verbose", "focus", "skin", "indicator", "wake", "toolsets", "skills", "pet", "hatch",
    "generate-pet", "cron", "reload", "browser", "plugins", "palette", "subscription", "upgrade", "platforms", "gateway", "copy", "paste", "image", "quit", "exit",
];

pub const ALL: [Builtin; 10] = [
    Builtin {
        id: "claude",
        command: "claude",
        efforts: &["low", "medium", "high", "xhigh", "max"],
        effort_word: "Effort",
        plan: true,
        // Asks without a prompt, so it costs no tokens; no hooks, so the user's don't take it for a session.
        models: Catalog {
            args: &["-p", "--input-format", "stream-json", "--output-format", "stream-json", "--verbose", "--no-session-persistence", "--settings", r#"{"disableAllHooks":true}"#],
            input: r#"{"type":"control_request","request_id":"models","request":{"subtype":"initialize"}}"#,
            parse: crate::claude_catalog,
            fallback: &["haiku", "sonnet", "opus"],
            ..Catalog::NONE
        },
        prompt_args: "--",
        resume_args: "--resume",
        fork_args: "--resume {sessionId} --fork-session",
        status_via: "uses Claude Code hooks",
        unattached: "Claude skips hooks in folders it doesn't trust. Trust this folder in Claude to see status.",
        pastes_code: true,
        oneshot: Some(Oneshot { args: &["-p"], model_flag: "--model", model: "haiku" }),
        install: Install::NONE,
        assumed_installed: true,
        chat_only_status: false,
        never_asks: false,
        by_upstream: false,
        transport: Transport::Claude,
        tui_only: &[],
    },
    Builtin {
        id: "codex",
        command: "codex",
        efforts: &[],
        effort_word: "Reasoning",
        plan: false,
        models: Catalog { args: &["debug", "models"], parse: crate::codex_catalog, ..Catalog::NONE },
        prompt_args: "--",
        resume_args: "resume",
        fork_args: "fork {sessionId}",
        status_via: "reads Codex session events",
        unattached: "This codex runs without the app-server, so Anywhere can't see its status.",
        pastes_code: false,
        oneshot: Some(Oneshot { args: &["exec", "-s", "read-only", "--skip-git-repo-check", "--color", "never"], model_flag: "-m", model: "" }),
        install: Install::NONE,
        assumed_installed: true,
        chat_only_status: false,
        never_asks: false,
        by_upstream: false,
        transport: Transport::Codex,
        tui_only: &[],
    },
    Builtin {
        id: "cursor",
        command: "cursor-agent",
        models: Catalog { args: &["--list-models"], parse: catalogs::cursor, fallback: &["composer-2.5", "gpt-5.4", "claude-sonnet-4-6", "grok-4.6"], ..Catalog::NONE },
        resume_args: "--resume",
        install: Install { command: "curl https://cursor.com/install -fsS | bash", ..Install::NONE },
        ..NEWER
    },
    Builtin {
        id: "grok",
        command: "grok",
        efforts: GROK_EFFORTS,
        effort_word: "Reasoning",
        plan: true,
        models: Catalog { args: &["models"], parse: catalogs::grok, fallback: &["grok-4.6", "grok-4.5"], ..Catalog::NONE },
        install: Install { command: "curl -fsSL https://x.ai/cli/install.sh | bash", ..Install::NONE },
        ..NEWER
    },
    Builtin {
        id: "opencode",
        command: "opencode",
        effort_word: "Variant",
        plan: true,
        models: Catalog { args: &["models", "--verbose"], parse: catalogs::opencode, ..Catalog::NONE },
        prompt_args: "--prompt",
        resume_args: "-s",
        fork_args: "-s {sessionId} --fork",
        install: Install { command: "npm i -g opencode-ai", ..Install::NONE },
        by_upstream: true,
        tui_only: OPENCODE_TUI_ONLY,
        ..NEWER
    },
    Builtin {
        id: "pi",
        command: "pi",
        efforts: THINKING,
        effort_word: "Thinking",
        plan: true,
        models: Catalog { args: &["--mode", "rpc", "--no-session"], ..PI_MODELS },
        prompt_args: "--",
        resume_args: "--session",
        fork_args: "--fork {sessionId}",
        install: Install { command: "npm i -g @earendil-works/pi-coding-agent", ..Install::NONE },
        never_asks: true,
        by_upstream: true,
        transport: Transport::Pi,
        tui_only: PI_TUI_ONLY,
        ..NEWER
    },
    Builtin {
        id: "omp",
        command: "omp",
        efforts: THINKING,
        effort_word: "Thinking",
        plan: true,
        // Pi's extensions can add models, so its list loads them; omp's only run for a session.
        models: Catalog { args: &["--mode", "rpc", "--no-session", "--no-extensions"], ..PI_MODELS },
        resume_args: "--resume",
        install: Install { command: "curl -fsSL https://omp.sh/install | sh", ..Install::NONE },
        never_asks: true,
        by_upstream: true,
        transport: Transport::Pi,
        ..NEWER
    },
    Builtin {
        id: "fx",
        command: "fx",
        models: Catalog { args: &["models", "--json"], parse: catalogs::fx, fallback: &["zai/glm-5.2-fast"], ..Catalog::NONE },
        install: Install { command: "curl -fsSL https://fx.sh/setup.sh | bash", ..Install::NONE },
        ..NEWER
    },
    Builtin {
        id: "hermes",
        command: "hermes",
        models: Catalog { args: &["acp"], ..ACP_MODELS },
        // `hermes chat` is what takes a model and a first prompt.
        prompt_args: "-q",
        resume_args: "chat --resume",
        install: Install { command: "curl -fsSL https://hermes-agent.nousresearch.com/install.sh | bash", then: "run hermes model" },
        by_upstream: true,
        tui_only: HERMES_TUI_ONLY,
        ..NEWER
    },
    Builtin {
        id: "antigravity",
        command: "agy",
        models: Catalog { command: "sh", args: AGY_ARGS, fallback: &["gemini-3.8-flash-high"], ..ACP_MODELS },
        install: Install { command: "curl -fsSL https://antigravity.google/cli/install.sh | bash", then: "run agy once to sign in" },
        ..NEWER
    },
];

const PI_MODELS: Catalog = Catalog { input: r#"{"type":"get_available_models"}"#, answer: Some(catalogs::pi_answered), parse: catalogs::pi, ..Catalog::NONE };
const ACP_MODELS: Catalog = Catalog { input: ACP_INPUT, answer: Some(catalogs::acp_answered), parse: catalogs::acp, ..Catalog::NONE };

pub const COUNT: usize = ALL.len();

pub const IDS: [&str; COUNT] = {
    let mut ids = [""; COUNT];
    let mut i = 0;
    while i < COUNT {
        ids[i] = ALL[i].id;
        i += 1;
    }
    ids
};

pub fn get(id: &str) -> Option<&'static Builtin> {
    ALL.iter().find(|b| b.id == id)
}

/// The entry for `id`, else Claude's: a pick naming no known agent starts Claude.
pub fn known(id: &str) -> &'static Builtin {
    get(id).unwrap_or(&ALL[0])
}

static BUILTIN: LazyLock<Vec<ProviderInfo>> = LazyLock::new(|| {
    ALL.iter().map(|b| ProviderInfo { id: b.id.into(), available: b.assumed_installed, efforts: b.efforts.iter().map(|e| e.to_string()).collect(), plan: b.plan, chat: None, never_asks: None }).collect()
});
static OTHER: LazyLock<ProviderInfo> = LazyLock::new(ProviderInfo::default);

/// What this build knows of `id` before or without pocketd's reply.
pub fn builtin(id: &str) -> &'static ProviderInfo {
    BUILTIN.iter().find(|p| p.id == id).unwrap_or(&OTHER)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_agent_takes_the_efforts_and_plan_mode_pocketd_lists_for_it() {
        let thinking: &[&str] = &["off", "minimal", "low", "medium", "high", "xhigh", "max"];
        let expected: [(&str, &[&str], bool); 11] = [
            ("claude", &["low", "medium", "high", "xhigh", "max"], true),
            ("codex", &[], false),
            ("cursor", &[], false),
            ("grok", &["low", "medium", "high", "xhigh"], true),
            ("opencode", &[], true),
            ("pi", thinking, true),
            ("omp", thinking, true),
            ("fx", &[], false),
            ("hermes", &[], false),
            ("antigravity", &[], false),
            ("gemini", &[], false),
        ];
        for (id, efforts, plan) in expected {
            let info = builtin(id);
            assert_eq!((info.efforts.iter().map(String::as_str).collect::<Vec<_>>(), info.plan, info.chat), (efforts.to_vec(), plan, None), "{id}");
        }
    }

    #[test]
    fn an_older_pocketd_s_providers_have_no_chat() {
        let info: ProviderInfo = serde_json::from_str(r#"{"id":"claude","available":true,"efforts":["low"],"plan":true}"#).unwrap();
        assert_eq!((info.available, info.efforts, info.plan, info.chat), (true, vec!["low".to_string()], true, None));
    }

    #[test]
    fn before_pocketd_says_what_is_installed_only_claude_and_codex_are() {
        let installed: Vec<&str> = IDS.into_iter().filter(|id| builtin(id).available).collect();
        assert_eq!(installed, ["claude", "codex"]);
        assert!(!builtin("gemini").available);
        let missing: ProviderInfo = serde_json::from_str(r#"{"id":"pi","efforts":[],"plan":true}"#).unwrap();
        assert!(!missing.available);
    }

    #[test]
    fn claude_comes_first_so_an_unknown_pick_starts_it() {
        assert_eq!(known("gemini").id, "claude");
        assert_eq!(IDS, ["claude", "codex", "cursor", "grok", "opencode", "pi", "omp", "fx", "hermes", "antigravity"]);
    }

    #[test]
    fn default_args_match_pocketd() {
        let args = |id| (known(id).prompt_args, known(id).resume_args, known(id).fork_args);
        assert_eq!(args("claude"), ("--", "--resume", "--resume {sessionId} --fork-session"));
        assert_eq!(args("codex"), ("--", "resume", "fork {sessionId}"));
        assert_eq!(args("cursor"), ("", "--resume", ""));
        assert_eq!(args("opencode"), ("--prompt", "-s", "-s {sessionId} --fork"));
        assert_eq!(args("pi"), ("--", "--session", "--fork {sessionId}"));
        assert_eq!(args("omp"), ("", "--resume", ""));
        assert_eq!(args("hermes"), ("-q", "chat --resume", ""));
        for id in ["grok", "fx", "antigravity"] {
            assert_eq!(args(id), ("", "", ""), "{id}");
        }
    }

    #[test]
    fn only_claude_and_codex_write_commit_messages_and_the_agents_after_them_say_how_to_install() {
        for b in &ALL {
            let first = b.assumed_installed;
            assert_eq!((b.oneshot.is_some(), b.install.command.is_empty(), b.chat_only_status), (first, first, !first), "{}", b.id);
        }
    }

    #[test]
    fn an_install_is_a_command_to_run_and_then_what_is_left_to_do_in_words() {
        for b in &ALL {
            let i = &b.install;
            assert!(!i.command.contains("then") && !i.command.starts_with(char::is_uppercase), "{} gives a sentence as its command", b.id);
            assert!(i.command.is_empty() || i.command.split_whitespace().next() != Some(b.command), "{} is installed by running itself", b.id);
        }
        let then = |id| (known(id).install.command, known(id).install.then);
        assert_eq!(then("antigravity"), ("curl -fsSL https://antigravity.google/cli/install.sh | bash", "run agy once to sign in"));
        assert_eq!(then("hermes"), ("curl -fsSL https://hermes-agent.nousresearch.com/install.sh | bash", "run hermes model"));
        assert_eq!(then("pi"), ("npm i -g @earendil-works/pi-coding-agent", ""));
    }

    #[test]
    fn pi_and_omp_never_ask_and_pocketd_s_word_on_it_wins() {
        let unasked: Vec<&str> = IDS.into_iter().filter(|id| builtin(id).never_asks()).collect();
        assert_eq!(unasked, ["pi", "omp"]);
        let said = |json: &str| serde_json::from_str::<ProviderInfo>(json).unwrap().never_asks();
        assert!(said(r#"{"id":"pi","available":true}"#), "a pocketd older than the field");
        assert!(said(r#"{"id":"cursor","neverAsks":true}"#));
        assert!(!said(r#"{"id":"pi","neverAsks":false}"#));
        assert!(!said(r#"{"id":"claude","available":true}"#) && !said(r#"{"id":"gemini"}"#));
    }

    #[test]
    fn the_agents_that_run_other_makers_models_list_them_under_who_serves_each() {
        let grouped: Vec<&str> = ALL.iter().filter(|b| b.by_upstream).map(|b| b.id).collect();
        assert_eq!(grouped, ["opencode", "pi", "omp", "hermes"]);
        let pi = r#"{"type":"response","command":"get_available_models","success":true,"data":{"models":[{"id":"m","name":"M","provider":"p"}]}}"#;
        let hermes = r#"{"jsonrpc":"2.0","id":2,"result":{"models":{"availableModels":[{"modelId":"p:m","name":"P · m"}]}}}"#;
        let opencode = "p/m\n{ \"name\": \"M\" }\n";
        for (id, output) in [("opencode", opencode), ("pi", pi), ("omp", pi), ("hermes", hermes)] {
            let models = known(id).models.read(output).unwrap();
            assert_eq!(models.iter().map(|m| m.upstream.as_str()).collect::<Vec<_>>(), [if id == "hermes" { "P" } else { "p" }], "{id}");
        }
    }

    #[test]
    fn a_server_that_answers_with_an_error_lists_no_models_and_says_why() {
        let refused = r#"{"jsonrpc":"2.0","id":1,"result":{}}
{"jsonrpc":"2.0","id":2,"error":{"code":-32000,"message":"Authentication required"}}"#;
        assert_eq!(known("hermes").models.read(refused), Err("Authentication required".to_string()));
        assert_eq!(known("antigravity").models.read(r#"{"jsonrpc":"2.0","id":2,"error":{"code":-32603}}"#), Err(String::new()));
        let pi = r#"{"type":"response","command":"get_available_models","success":false,"error":"No models configured"}"#;
        assert_eq!(known("pi").models.read(pi), Err("No models configured".to_string()));
        assert_eq!(known("hermes").models.read(r#"{"jsonrpc":"2.0","id":2,"result":{}}"#), Ok(Vec::new()), "an answer that lists none is no error");
        assert_eq!(known("cursor").models.read("Error: not logged in"), Ok(Vec::new()), "a CLI that exits has no answer to read an error from");
    }

    #[test]
    fn a_server_s_catalog_says_which_line_answers_and_a_cli_that_exits_does_not() {
        let servers: Vec<&str> = ALL.iter().filter(|b| b.models.answer.is_some()).map(|b| b.id).collect();
        assert_eq!(servers, ["pi", "omp", "hermes", "antigravity"]);
        let models = &known("pi").models;
        let answer = r#"{"type":"response","command":"get_available_models","success":true,"data":{"models":[{"id":"m","name":"M","provider":"p","reasoning":false}]}}"#;
        assert!(models.answer.unwrap()(answer));
        assert_eq!((models.parse)(answer)[0].value, "p/m");
        assert_eq!(known("omp").models.args, ["--mode", "rpc", "--no-session", "--no-extensions"]);
    }

    #[test]
    fn an_acp_catalog_opens_its_session_in_the_folder_it_runs_in() {
        let hermes = &known("hermes").models;
        let lines: Vec<serde_json::Value> = hermes.input_in("/Users/me/a \"b\"").lines().map(|l| serde_json::from_str(l).unwrap()).collect();
        assert_eq!(lines.iter().map(|l| l["method"].as_str().unwrap()).collect::<Vec<_>>(), ["initialize", "session/new"]);
        assert_eq!(lines[1]["params"], serde_json::json!({"cwd": "/Users/me/a \"b\"", "mcpServers": []}));
        assert!(hermes.answer.unwrap()(r#"{"jsonrpc":"2.0","id":2,"result":{}}"#));
        let claude = &known("claude").models;
        assert_eq!(claude.input_in("/w"), claude.input);
    }

    #[test]
    #[cfg(unix)]
    fn antigravity_s_acp_server_runs_from_its_own_folder_wherever_agy_put_it() {
        let home = std::env::temp_dir().join(format!("pocket-agy-{}", std::process::id()));
        let dir = home.join(".local/share/agy-acp");
        std::fs::create_dir_all(&dir).unwrap();
        let server = dir.join("agy_acp_server.par");
        std::fs::write(&server, "#!/bin/sh\necho \"$PWD $*\"\n").unwrap();
        std::fs::set_permissions(&server, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        let models = &known("antigravity").models;
        let run = |home: &std::path::Path| std::process::Command::new(models.command).args(models.args).current_dir("/").env_clear().env("HOME", home).env("PATH", "/usr/bin:/bin").output().unwrap();
        let out = run(&home);
        let extra = models.args[3..].join(" ");
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), format!("{} {extra}", dir.display()).trim());
        assert_eq!(run(&home.join("none")).status.code(), Some(127));
        std::fs::remove_dir_all(&home).unwrap();
    }
}
