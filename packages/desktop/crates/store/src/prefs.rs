//! What each Settings section stores; a section that fails to read falls back to its defaults alone.

pub mod agents;
pub mod automations;
pub mod browser;
pub mod diff;
pub mod files;
pub mod general;
pub mod git;
pub mod phone;
pub mod sidebar;
pub mod terminal;

pub use agents::Agents;
pub use automations::Automations;
pub use browser::Browser;
pub use diff::Diff;
pub use files::Files;
pub use general::General;
pub use git::Git;
pub use phone::Phone;
pub use sidebar::Sidebar;
pub use terminal::Terminal;
