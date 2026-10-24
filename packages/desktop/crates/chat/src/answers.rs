use crate::Question;

/// What the user answered each question, read off AskUserQuestion's result:
/// `User has answered your questions: "Q"="A", "Q2"="A2". You can now continue…`, an answer maybe followed by its notes.
pub fn answers<'a>(questions: &'a [Question], output: &str) -> Vec<(&'a Question, String)> {
    // An answer can hold quotes and commas, so it ends only at a quote the result's own wording follows.
    let closes = |rest: &str| {
        rest.is_empty()
            || [". You can", " user notes:", " selected preview:"].iter().any(|s| rest.starts_with(s))
            || questions.iter().any(|q| rest.strip_prefix(", \"").is_some_and(|r| r.starts_with(&format!("{}\"=\"", q.question))))
    };
    questions
        .iter()
        .filter_map(|q| {
            let open = format!("\"{}\"=\"", q.question);
            let tail = &output[output.find(&open)? + open.len()..];
            let end = tail.match_indices('"').map(|(p, _)| p).find(|&p| closes(&tail[p + 1..])).or_else(|| tail.rfind('"'))?;
            Some((q, tail[..end].to_string()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::answers;
    use crate::Question;

    fn asked(texts: &[&str]) -> Vec<Question> {
        texts.iter().map(|t| Question { question: t.to_string(), ..Default::default() }).collect()
    }

    fn read(qs: &[Question], output: &str) -> Vec<(String, String)> {
        answers(qs, output).into_iter().map(|(q, a)| (q.question.clone(), a)).collect()
    }

    #[test]
    fn each_question_reads_its_own_answer() {
        let qs = asked(&["Which tests?", "Which parser?"]);
        let out = r#"User has answered your questions: "Which tests?"="Only chat_view", "Which parser?"="Tree-sitter". You can now continue with the user's answers in mind."#;
        assert_eq!(read(&qs, out), [("Which tests?".into(), "Only chat_view".into()), ("Which parser?".into(), "Tree-sitter".into())]);
    }

    #[test]
    fn an_answer_keeps_its_quotes_and_commas_and_drops_its_notes() {
        let qs = asked(&["Name?", "Style?"]);
        let out = r#"User has answered your questions: "Name?"="Say "hi", then go" user notes: keep it short, "Style?"="Bold, Italic". You can now continue."#;
        assert_eq!(read(&qs, out), [("Name?".into(), r#"Say "hi", then go"#.into()), ("Style?".into(), "Bold, Italic".into())]);
    }

    #[test]
    fn a_question_left_out_of_the_result_has_no_answer() {
        let qs = asked(&["Which tests?", "Which parser?"]);
        assert_eq!(read(&qs, r#"User has answered your questions: "Which parser?"="Pest". You can now continue."#), [("Which parser?".into(), "Pest".into())]);
        assert!(read(&qs, "The user doesn't want to proceed with this tool use.").is_empty());
    }
}
