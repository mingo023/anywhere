use serde::{Deserialize, Serialize};

/// Diffs and the graph.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
#[serde(default)]
pub struct Diff {
    pub split: bool,
    pub tree: bool,
    pub context: usize,
    pub ignore_whitespace: bool,
    /// The graph follows every branch and remote, not only HEAD, its upstream and the base.
    pub all_branches: bool,
    pub page: usize,
}

impl Default for Diff {
    fn default() -> Self {
        Self { split: false, tree: false, context: 3, ignore_whitespace: false, all_branches: false, page: 50 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_diff_section_keeps_what_it_has_and_defaults_the_rest() {
        let diff: Diff = serde_json::from_str(r#"{"split":true,"page":200}"#).unwrap();
        assert_eq!(diff, Diff { split: true, page: 200, ..Diff::default() });
        assert_eq!(serde_json::from_str::<Diff>(&serde_json::to_string(&diff).unwrap()).unwrap(), diff);
    }
}
