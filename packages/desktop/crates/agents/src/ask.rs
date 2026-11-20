use crate::Detail;
use serde::{Deserialize, Serialize};

/// One of AskUserQuestion's questions.
#[derive(Deserialize, Serialize, Default, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Question {
    pub question: String,
    pub header: String,
    pub options: Vec<Choice>,
    pub multi_select: bool,
}

#[derive(Deserialize, Serialize, Default, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Choice {
    pub label: String,
    pub description: String,
}

#[derive(Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Permission {
    pub request_id: String,
    pub agent_id: String,
    pub tool_name: String,
    pub detail: Detail,
    /// Whether a deny can carry a message back to the agent.
    pub feedback: bool,
}

impl Permission {
    pub fn ask(&self) -> String {
        let (verb, what) = self.parts();
        format!("{verb} {what}")
    }

    /// The ask split as ("Wants to run", "ls /tmp").
    pub fn parts(&self) -> (String, String) {
        let d = &self.detail;
        let (verb, what) = match d.kind.as_str() {
            "shell" => ("run".into(), &d.command),
            "edit" | "write" | "read" => (d.kind.clone(), &d.path),
            "search" => ("search".into(), &d.query),
            "task" => ("start".into(), &d.description),
            _ => ("use".into(), if d.name.is_empty() { &self.tool_name } else { &d.name }),
        };
        (format!("Wants to {verb}"), what.clone())
    }

    /// AskUserQuestion's questions; none for any other ask.
    pub fn questions(&self) -> Vec<Question> {
        if self.tool_name != "AskUserQuestion" {
            return vec![];
        }
        self.detail.questions()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::Frame;
    use serde_json::json;

    #[test]
    fn an_ask_user_question_request_carries_its_questions() {
        let f: Frame = serde_json::from_value(json!({"type": "permission.request", "request": {
            "requestId": "r1", "agentId": "a1", "toolName": "AskUserQuestion",
            "detail": {"kind": "other", "name": "AskUserQuestion", "input": {"questions": [
                {"question": "Which color?", "header": "Color", "options": [{"label": "Red", "description": "Warm"}, {"label": "Blue"}], "multiSelect": true}
            ]}}
        }}))
        .unwrap();
        let q = f.request.unwrap().questions();
        assert_eq!(q.len(), 1);
        assert_eq!((q[0].question.as_str(), q[0].header.as_str(), q[0].multi_select), ("Which color?", "Color", true));
        assert_eq!(q[0].options[0], Choice { label: "Red".into(), description: "Warm".into() });
        assert_eq!(q[0].options[1].description, "");
    }

    #[test]
    fn only_ask_user_question_has_questions() {
        let p = Permission { tool_name: "Bash".into(), detail: Detail { input: json!({"questions": [{}]}), ..Default::default() }, ..Default::default() };
        assert!(p.questions().is_empty());
    }

    #[test]
    fn a_permission_request_says_whether_it_takes_feedback() {
        let f: Frame = serde_json::from_str(include_str!("../../../../pocketd/internal/proto/testdata/golden/server/permission_request.json")).unwrap();
        let p = f.request.unwrap();
        assert_eq!((p.request_id.as_str(), p.feedback), ("r1", true));
    }
}
