use serde_json::Value;

#[derive(Clone, Debug, PartialEq)]
pub struct Pr {
    pub number: u32,
    pub title: String,
    pub url: String,
    pub state: PrState,
    pub draft: bool,
    pub checks: Checks,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PrState {
    Open,
    Merged,
    Closed,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Checks {
    pub passed: u16,
    pub failed: u16,
    pub pending: u16,
}

/// Why `gh` can't be asked at all, as opposed to a branch having no PR.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GhProblem {
    Missing,
    LoggedOut,
}

/// The PR for the branch checked out in the working directory.
pub const VIEW: &[&str] = &["gh", "pr", "view", "--json", "number,title,url,state,isDraft,statusCheckRollup"];

/// The branch name on the remote, without `origin/`.
pub fn base_branch(base: &str) -> &str {
    base.strip_prefix("origin/").unwrap_or(base)
}

/// Opens a PR for the checked-out branch into `base`, titled and described from its commits.
pub fn create_argv(base: &str) -> Vec<&str> {
    vec!["gh", "pr", "create", "--fill", "--base", base_branch(base)]
}

/// Reads what [`VIEW`] returned through `daemon::run_login`. No PR, or any other failure, is `Ok(None)`.
pub fn view_result(out: Result<String, String>) -> Result<Option<Pr>, GhProblem> {
    match out {
        Ok(json) => Ok(parse(&json)),
        // The login shell reports a missing command: zsh and bash say "command not found", fish "Unknown command".
        Err(e) if e.contains("command not found") || e.contains("Unknown command") => Err(GhProblem::Missing),
        Err(e) if e.contains("gh auth login") => Err(GhProblem::LoggedOut),
        Err(_) => Ok(None),
    }
}

fn parse(json: &str) -> Option<Pr> {
    let v: Value = serde_json::from_str(json).ok()?;
    let state = match v["state"].as_str()? {
        "MERGED" => PrState::Merged,
        "CLOSED" => PrState::Closed,
        _ => PrState::Open,
    };
    Some(Pr {
        number: u32::try_from(v["number"].as_u64()?).ok()?,
        title: v["title"].as_str()?.to_string(),
        url: v["url"].as_str()?.to_string(),
        state,
        draft: v["isDraft"].as_bool().unwrap_or(false),
        checks: checks(&v["statusCheckRollup"]),
    })
}

fn checks(rollup: &Value) -> Checks {
    let mut c = Checks::default();
    for check in rollup.as_array().into_iter().flatten() {
        let field = |k: &str| check[k].as_str().unwrap_or("");
        // Commit statuses carry `state`; check runs carry `status` and `conclusion`.
        let (failed, pending) = if field("__typename") == "StatusContext" {
            (matches!(field("state"), "FAILURE" | "ERROR"), matches!(field("state"), "PENDING" | "EXPECTED"))
        } else {
            let failed = matches!(field("conclusion"), "FAILURE" | "TIMED_OUT" | "CANCELLED" | "ACTION_REQUIRED" | "STARTUP_FAILURE");
            (failed, field("status") != "COMPLETED")
        };
        if failed {
            c.failed += 1;
        } else if pending {
            c.pending += 1;
        } else {
            c.passed += 1;
        }
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    const OPEN: &str = r#"{"isDraft":false,"number":42,"state":"OPEN","title":"Fix the flaky snapshot","url":"https://github.com/acme/app/pull/42","statusCheckRollup":[
        {"__typename":"CheckRun","name":"build","status":"COMPLETED","conclusion":"SUCCESS"},
        {"__typename":"CheckRun","name":"label","status":"COMPLETED","conclusion":"SKIPPED"},
        {"__typename":"CheckRun","name":"test","status":"COMPLETED","conclusion":"FAILURE"},
        {"__typename":"CheckRun","name":"e2e","status":"COMPLETED","conclusion":"TIMED_OUT"},
        {"__typename":"CheckRun","name":"lint","status":"IN_PROGRESS","conclusion":""},
        {"__typename":"StatusContext","context":"ci/circleci","state":"PENDING"},
        {"__typename":"StatusContext","context":"deploy","state":"ERROR"},
        {"__typename":"StatusContext","context":"vercel","state":"SUCCESS"}
    ]}"#;

    #[test]
    fn an_open_pr_counts_its_checks_by_outcome() {
        let pr = Pr {
            number: 42,
            title: "Fix the flaky snapshot".into(),
            url: "https://github.com/acme/app/pull/42".into(),
            state: PrState::Open,
            draft: false,
            checks: Checks { passed: 3, failed: 3, pending: 2 },
        };
        assert_eq!(view_result(Ok(OPEN.into())), Ok(Some(pr)));
    }

    #[test]
    fn merged_closed_and_draft_prs_keep_their_state() {
        let pr = |json: &str| view_result(Ok(json.into())).unwrap().unwrap();
        let merged = pr(r#"{"isDraft":false,"number":7,"state":"MERGED","title":"t","url":"u","statusCheckRollup":[]}"#);
        let closed = pr(r#"{"isDraft":false,"number":8,"state":"CLOSED","title":"t","url":"u","statusCheckRollup":[]}"#);
        let draft = pr(r#"{"isDraft":true,"number":9,"state":"OPEN","title":"t","url":"u","statusCheckRollup":[]}"#);
        assert_eq!((merged.state, closed.state, draft.state, draft.draft), (PrState::Merged, PrState::Closed, PrState::Open, true));
        assert_eq!(merged.checks, Checks::default());
    }

    #[test]
    fn a_missing_gh_differs_from_a_logged_out_one() {
        for missing in ["zsh:1: command not found: gh", "bash: line 1: gh: command not found", "fish: Unknown command: gh"] {
            assert_eq!(view_result(Err(missing.into())), Err(GhProblem::Missing), "{missing}");
        }
        let logged_out = "To get started with GitHub CLI, please run:  gh auth login\nAlternatively, populate the GH_TOKEN environment variable with a GitHub API authentication token.";
        assert_eq!(view_result(Err(logged_out.into())), Err(GhProblem::LoggedOut));
    }

    #[test]
    fn a_branch_without_a_pr_or_a_failing_gh_has_none() {
        assert_eq!(view_result(Err("no pull requests found for branch \"fix/restore-handoff\"".into())), Ok(None));
        assert_eq!(view_result(Err("fatal: not a git repository (or any of the parent directories): .git".into())), Ok(None));
        assert_eq!(view_result(Ok("not json".into())), Ok(None));
    }

    #[test]
    fn create_argv_targets_the_base_without_its_remote() {
        assert_eq!(create_argv("origin/main"), ["gh", "pr", "create", "--fill", "--base", "main"]);
        assert_eq!(create_argv("develop"), ["gh", "pr", "create", "--fill", "--base", "develop"]);
    }
}
