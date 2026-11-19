use crate::hosts;
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

pub(super) static NEXT: AtomicU64 = AtomicU64::new(0);

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

/**
 * A new create's requestId. It starts with the clock because pocketd replays the receipt of a
 * requestId it has seen, even from before a restart. Each reconnect resends the create under the
 * same requestId until its final reply, which joins the first create.
 */
pub fn request_id() -> String {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    format!("{nanos:x}-{}", NEXT.fetch_add(1, Ordering::Relaxed))
}
