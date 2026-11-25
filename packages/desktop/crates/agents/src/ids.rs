use crate::hosts;
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::sync::LazyLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) const PAIR: &str = "pair";
pub(super) const CREATE: &str = "create-";
pub(super) const SETTINGS: &str = "settings";
pub(super) const AUTO: &str = "auto-";
pub(super) const BOARD: &str = "board-";
pub(super) const PROMPT: &str = "prompt-";
pub(super) const COMMANDS: &str = "commands-";
pub(super) const BTW: &str = "btw-";
pub(super) const REMOTE_CONTROL: &str = "remotecontrol-";
pub(super) const MCP: &str = "mcp-";
pub(super) const MCP_ACT: &str = "mcpact-";
pub(super) const DELEGATIONS: &str = "delegations-";
pub(super) const QUEUE: &str = "queue-";
pub(super) const INTERRUPT: &str = "interrupt-";
pub(super) const REGISTRY: &str = "registry-";

pub(super) static NEXT: AtomicU64 = AtomicU64::new(0);

/// pocketd keeps a prompt's id as its receipt, so the counter alone would replay an older launch's prompt.
static PROMPT_PREFIX: LazyLock<String> = LazyLock::new(launch_prefix);

fn launch_prefix() -> String {
    format!("{:x}", RandomState::new().build_hasher().finish())
}

fn prompt_id_in(launch: &str, n: u64) -> String {
    format!("{PROMPT}{launch}-{n}")
}

pub(super) fn prompt_id() -> String {
    prompt_id_in(&PROMPT_PREFIX, NEXT.fetch_add(1, Ordering::Relaxed))
}

pub(super) fn auto_id() -> String {
    format!("{AUTO}{}", NEXT.fetch_add(1, Ordering::Relaxed))
}

pub(super) fn host_id() -> String {
    format!("{}{}", hosts::HOST, NEXT.fetch_add(1, Ordering::Relaxed))
}

pub(super) fn queue_id() -> String {
    format!("{QUEUE}{}", NEXT.fetch_add(1, Ordering::Relaxed))
}

pub(super) fn board_id() -> String {
    format!("{BOARD}{}", NEXT.fetch_add(1, Ordering::Relaxed))
}

pub(super) fn registry_id() -> String {
    format!("{REGISTRY}{}", NEXT.fetch_add(1, Ordering::Relaxed))
}

/**
 * A new create's requestId. It starts with the clock because pocketd replays the receipt of a
 * requestId it has seen, even from before a restart. Each reconnect resends the create under the
 * same requestId until its final reply, which joins the first create.
 */
pub fn request_id() -> String {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    format!("{nanos:x}-{}", NEXT.fetch_add(1, Ordering::Relaxed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launches_get_different_prefixes() {
        assert_ne!(launch_prefix(), launch_prefix());
    }

    #[test]
    fn a_prompt_id_is_the_launch_prefix_then_the_counter() {
        assert_eq!(prompt_id_in("k3x9", 7), "prompt-k3x9-7");
        let (a, b) = (prompt_id(), prompt_id());
        assert_ne!(a, b);
        assert!(a.starts_with(&format!("prompt-{}-", *PROMPT_PREFIX)));
    }
}
