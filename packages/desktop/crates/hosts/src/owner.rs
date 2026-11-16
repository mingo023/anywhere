//! The owner token a Windows host's pocketd mints for this Mac. Its sockets can't tell who the owner is,
//! so every connection to its owner TCP port sends the token first. This Mac's pocketd pairs and saves it.

use serde::Deserialize;
use std::path::Path;

#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct Owner {
    pub token: String,
    /// pocketd's owner TCP port on the host's loopback.
    pub port: u16,
}

pub fn load(path: &Path) -> Option<Owner> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}
