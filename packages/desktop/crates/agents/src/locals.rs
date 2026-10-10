use serde::Deserialize;
use serde_json::{Value, json};
use std::time::{SystemTime, UNIX_EPOCH};

/// A Local the user added to a project, from `local.list`. The project's own one has no record.
#[derive(Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Local {
    pub id: String,
    pub project: String,
    pub name: String,
}

/// Whether `key`, a worktree's path or a Local's id, is a Local's id. Paths start with `/`; ids never do.
pub fn is_local(key: &str) -> bool {
    !key.starts_with('/')
}

/// The `agent.create` checkout that starts a session in the tree `key` as it is.
pub fn checkout(key: &str) -> Value {
    if is_local(key) { json!({"local": key}) } else { json!({"worktree": key}) }
}

/// A fresh Local id: pocketd takes letters, digits, `-` and `_`.
pub fn new_id() -> String {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    format!("local-{nanos:x}")
}

/// "Local 2", "Local 3", …: the first number none of `taken` uses.
pub fn next_name<'a>(taken: impl IntoIterator<Item = &'a str>) -> String {
    let taken: Vec<&str> = taken.into_iter().collect();
    (2..).map(|n| format!("Local {n}")).find(|name| !taken.contains(&name.as_str())).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_local_id_is_anything_but_a_path() {
        assert!(is_local(&new_id()));
        assert!(!is_local("/w/app"));
    }

    #[test]
    fn a_session_here_names_the_local_or_the_worktree() {
        assert_eq!(checkout("local-1"), json!({"local": "local-1"}));
        assert_eq!(checkout("/w/app"), json!({"worktree": "/w/app"}));
    }

    #[test]
    fn a_new_local_takes_the_first_free_number() {
        assert_eq!(next_name([]), "Local 2");
        assert_eq!(next_name(["Local", "Local 2", "Local 4"]), "Local 3");
    }
}
