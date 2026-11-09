use serde::{Deserialize, Serialize};

/// Phone and remote access, as pocketd last reported it.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
#[serde(default)]
pub struct Phone {
    /// Listening on the tailnet as well as this Mac.
    pub tailnet: bool,
    /// 0 for pocketd's default.
    pub port: u32,
}

impl Default for Phone {
    fn default() -> Self {
        Self { tailnet: true, port: 0 }
    }
}
