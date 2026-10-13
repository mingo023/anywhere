use gpui_kit::component::text::{MarkdownNode, MarkdownParseContext, MarkdownPlugin, markdown_ast as mdast};
use gpui_kit::*;
use theme::*;

#[derive(Clone, Debug, PartialEq)]
enum Value {
    Text(String),
    List(Vec<String>),
}

/// A YAML front matter's top-level fields, or `None` for anything but a mapping. Scalars, block scalars and sequences only; a nested mapping keeps its raw lines.
fn fields(yaml: &str) -> Option<Vec<(String, Value)>> {
    let mut fields = Vec::new();
    let mut lines = yaml.lines().peekable();
    while let Some(line) = lines.next() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, rest) = line.split_once(':').filter(|_| !line.starts_with([' ', '\t', '-']))?;
        let rest = rest.trim();
        let mut nested = Vec::new();
        while let Some(next) = lines.next_if(|l| l.trim().is_empty() || l.starts_with([' ', '\t']) || (rest.is_empty() && l.starts_with("- "))) {
            nested.push(next.trim());
        }
        while nested.last() == Some(&"") {
            nested.pop();
        }
        let value = match rest {
            "" if !nested.is_empty() && nested.iter().all(|l| l.starts_with('-')) => Value::List(nested.iter().map(|l| unquote(l[1..].trim())).collect()),
            "" => Value::Text(nested.join("\n")),
            "|" | "|-" | "|+" => Value::Text(nested.join("\n")),
            ">" | ">-" | ">+" => Value::Text(nested.join(" ")),
            _ if rest.starts_with('[') && rest.ends_with(']') => Value::List(rest[1..rest.len() - 1].split(',').map(|s| unquote(s.trim())).filter(|s| !s.is_empty()).collect()),
            _ => Value::Text(unquote(rest)),
        };
        fields.push((key.trim().to_string(), value));
    }
    Some(fields)
}

fn unquote(s: &str) -> String {
    let quoted = s.len() >= 2 && ((s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')));
    if quoted { s[1..s.len() - 1].to_string() } else { s.to_string() }
}

/// Renders a markdown file's YAML front matter as a table of its fields, like the reference preview.
pub struct Frontmatter;

impl MarkdownPlugin for Frontmatter {
    fn is_block(&self) -> bool {
        true
    }

    fn name(&self) -> &str {
        "frontmatter"
    }

    fn parse(&self, node: &mdast::Node, cx: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        let mdast::Node::Yaml(yaml) = node else { return None };
        let fields = fields(&yaml.value).filter(|f| !f.is_empty())?;
        Some(MarkdownNode::new("frontmatter", fields).text(yaml.value.clone()).markdown(cx.node_source(node).unwrap_or(&yaml.value).to_string()))
    }

    fn render(&self, node: &MarkdownNode, _: &mut Window, _: &mut App) -> impl IntoElement {
        let fields = node.data::<Vec<(String, Value)>>().cloned().unwrap_or_default();
        // em sizes from the reference, whose table is 0.92em of the 14px body.
        let em = 14. * 0.92;
        let value = |value: Value| match value {
            Value::Text(text) if text.is_empty() => div().text_color(MD_TEXT_2).child("—"),
            Value::Text(text) => div().child(text),
            Value::List(items) => div().flex().flex_wrap().children(items.into_iter().map(|item| {
                div()
                    .px(px(em * 0.95 * 0.7))
                    .py(px(em * 0.95 * 0.08))
                    .mr(px(em * 0.95 * 0.4))
                    .rounded_full()
                    .bg(Hsla::from(MD_LINK).opacity(0.12))
                    .text_color(MD_LINK)
                    .text_size(px(em * 0.95))
                    .child(item)
            })),
        };
        div().mb(px(16.8)).pb(px(14.)).border_b_1().border_color(MD_GRID).text_size(px(em)).line_height(relative(1.5)).children(fields.into_iter().map(|(key, v)| {
            div()
                .flex()
                .py(px(em * 0.28))
                .child(div().w(relative(0.26)).flex_none().pr(px(em * 1.4)).font_weight(FontWeight::MEDIUM).text_color(MD_TEXT_2).child(key))
                .child(value(v).flex_1().min_w_0())
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::{Value, fields};

    fn text(s: &str) -> Value {
        Value::Text(s.into())
    }

    #[test]
    fn scalars_lose_their_quotes() {
        assert_eq!(fields("title: \"Hello\"\nauthor: 'Ann'\ndraft: false").unwrap(), [("title".into(), text("Hello")), ("author".into(), text("Ann")), ("draft".into(), text("false"))]);
    }

    #[test]
    fn block_and_flow_sequences_become_lists() {
        let list = |items: &[&str]| Value::List(items.iter().map(|s| s.to_string()).collect());
        assert_eq!(fields("tags:\n  - layout\n  - \"regression\"\nother: [a, 'b']").unwrap(), [("tags".into(), list(&["layout", "regression"])), ("other".into(), list(&["a", "b"]))]);
        assert_eq!(fields("tags:\n- flush").unwrap(), [("tags".into(), list(&["flush"]))]);
    }

    #[test]
    fn an_empty_value_stays_empty() {
        assert_eq!(fields("summary:\nnext: 1").unwrap(), [("summary".into(), text("")), ("next".into(), text("1"))]);
    }

    #[test]
    fn block_scalars_keep_or_fold_their_lines() {
        assert_eq!(fields("a: |\n  one\n  two\nb: >-\n  one\n  two").unwrap(), [("a".into(), text("one\ntwo")), ("b".into(), text("one two"))]);
    }

    #[test]
    fn a_nested_mapping_keeps_its_lines() {
        assert_eq!(fields("author:\n  name: Ann\n  mail: a@b").unwrap(), [("author".into(), text("name: Ann\nmail: a@b"))]);
    }

    #[test]
    fn a_top_level_sequence_is_not_front_matter() {
        assert_eq!(fields("- name: example"), None);
    }
}
