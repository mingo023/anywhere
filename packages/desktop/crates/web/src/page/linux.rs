//! wry's Linux webview is WebKitGTK, which needs GTK's loop run beside GPUI's and works on X11 only,
//! so pages don't open here yet.

use super::{Edit, Event, Rect};
use std::convert::Infallible;

pub struct Page(Infallible);

#[derive(Debug)]
pub struct Unsupported;

impl std::fmt::Display for Unsupported {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the in-app browser isn't available on Linux yet")
    }
}

impl Page {
    pub fn new<P: ?Sized>(_url: &str, _parent: &P, _sink: impl Fn(Event) + 'static) -> Result<Self, Unsupported> {
        Err(Unsupported)
    }

    pub fn place(&self, _: Rect) -> bool {
        match self.0 {}
    }

    pub fn park(&self) {
        match self.0 {}
    }

    pub fn snapshot(&self, _: impl FnOnce(Option<Vec<u8>>) + 'static) {
        match self.0 {}
    }

    pub fn load(&self, _: &str) {
        match self.0 {}
    }

    pub fn reload(&self) {
        match self.0 {}
    }

    pub fn back(&self) {
        match self.0 {}
    }

    pub fn forward(&self) {
        match self.0 {}
    }

    pub fn can_go_back(&self) -> bool {
        match self.0 {}
    }

    pub fn can_go_forward(&self) -> bool {
        match self.0 {}
    }

    pub fn url(&self) -> Option<String> {
        match self.0 {}
    }

    pub fn take_keys(&self) {
        match self.0 {}
    }

    pub fn give_keys(&self) {
        match self.0 {}
    }

    pub fn edit(&self, _: Edit) {
        match self.0 {}
    }
}
