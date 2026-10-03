//! The in-app browser's page, a WKWebView laid over the window's own view, and what turns typed or printed text into a URL.

mod address;
mod link;
mod page;

pub use address::{host, resolve};
pub use link::link_at;
pub use page::{Edit, Event, Page, Rect};
