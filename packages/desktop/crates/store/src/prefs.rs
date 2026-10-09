//! What each Settings section stores; a section that fails to read falls back to its defaults alone.

pub mod general;
pub mod agents;
pub mod automations;
pub mod phone;
pub mod sidebar;
pub mod terminal;
pub mod files;
pub mod browser;
pub mod git;
pub mod diff;

pub use general::General;
pub use agents::Agents;
pub use automations::Automations;
pub use phone::Phone;
pub use sidebar::Sidebar;
pub use terminal::Terminal;
pub use files::Files;
pub use browser::Browser;
pub use git::Git;
pub use diff::Diff;
