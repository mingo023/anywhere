use crate::chat::selectable::selectable;
use gpui_kit::*;
use theme::*;

/// A block of a PR's Markdown, as much of it as a narrow panel shows.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Block {
    Heading(String),
    Para(String),
    Task(bool, String),
    Bullet(String),
    Quote(String),
    Code(String),
}

/// The blocks of `md`, leaving out HTML comments and lines that are only a tag, as templates and bots leave.
pub(crate) fn blocks(md: &str) -> Vec<Block> {
    let mut out: Vec<Block> = Vec::new();
    let (mut comment, mut code): (bool, Option<Vec<&str>>) = (false, None);
    // A paragraph runs on until a blank line.
    let mut joins = false;
    for line in md.lines() {
        let t = line.trim();
        let joined = std::mem::replace(&mut joins, false);
        if let Some(lines) = code.as_mut() {
            if t.starts_with("```") {
                out.push(Block::Code(lines.join("\n")));
                code = None;
            } else {
                lines.push(line);
            }
            continue;
        }
        if comment {
            comment = !t.contains("-->");
            continue;
        }
        if t.starts_with("<!--") {
            comment = !t.contains("-->");
            continue;
        }
        if t.starts_with("```") {
            code = Some(Vec::new());
            continue;
        }
        let block = match () {
            _ if t.is_empty() || (t.starts_with('<') && t.ends_with('>')) => None,
            _ if t.starts_with('#') => Some(Block::Heading(plain(t.trim_start_matches('#').trim()))),
            _ => match t.get(..6).map(str::to_ascii_lowercase).as_deref() {
                Some("- [x] " | "* [x] ") => Some(Block::Task(true, plain(&t[6..]))),
                Some("- [ ] " | "* [ ] ") => Some(Block::Task(false, plain(&t[6..]))),
                _ if t.starts_with("- ") || t.starts_with("* ") => Some(Block::Bullet(plain(&t[2..]))),
                _ if t.starts_with('>') => {
                    let said = plain(t.trim_start_matches('>').trim());
                    if let Some(Block::Quote(q)) = out.last_mut() {
                        q.push('\n');
                        q.push_str(&said);
                        continue;
                    }
                    Some(Block::Quote(said))
                }
                _ => {
                    joins = true;
                    if let (Some(Block::Para(p)), true) = (out.last_mut(), joined) {
                        p.push(' ');
                        p.push_str(&plain(t));
                        continue;
                    }
                    Some(Block::Para(plain(t)))
                }
            },
        };
        out.extend(block);
    }
    if let Some(lines) = code {
        out.push(Block::Code(lines.join("\n")));
    }
    out
}

/// `s` without the inline marks a panel can't style: code ticks, bold, and links reduced to their text.
fn plain(s: &str) -> String {
    let mut out = s.replace("**", "").replace('`', "");
    let mut from = 0;
    while let Some(open) = out[from..].find('[').map(|i| from + i) {
        let Some(mid) = out[open..].find(']').map(|i| open + i) else { break };
        from = mid;
        let Some(close) = out[mid..].strip_prefix("](").and(out[mid..].find(')')).map(|i| mid + i) else { continue };
        out.replace_range(mid..=close, "");
        out.remove(open);
        from = mid - 1;
    }
    out
}

/// `said` as a Markdown quote, ready to answer under.
pub(crate) fn quote(said: &str) -> String {
    let quoted: Vec<String> = said.trim().lines().map(|l| format!("> {l}").trim_end().to_string()).collect();
    format!("{}\n\n", quoted.join("\n"))
}

/// The room above `cur`, as the web's collapsing margins would leave it: a heading sits apart from what came before and close to what follows, list items sit together.
fn space(prev: Option<&Block>, cur: &Block) -> f32 {
    match (prev, cur) {
        (None, _) => 0.,
        (_, Block::Heading(_)) => 10.,
        (Some(Block::Heading(_)), _) => 4.,
        (Some(Block::Task(..) | Block::Bullet(_)), Block::Task(..) | Block::Bullet(_)) => 0.,
        _ => 8.,
    }
}

pub(crate) fn selectable_text(id: impl Into<ElementId>, text: impl Into<SharedString>) -> impl IntoElement {
    let text = text.into();
    let styled = StyledText::new(text.clone());
    let layout = styled.layout().clone();
    selectable(id, text, layout, styled)
}

pub(crate) fn body_view(selection_scope: &str, md: &str) -> Div {
    let blocks = blocks(md);
    let views = blocks.iter().enumerate().map(|(i, b)| {
        let text = |t: String| selectable_text(SharedString::from(format!("{selection_scope}:{i}")), t);
        let view = match b.clone() {
            Block::Heading(t) => div().text_size(px(13.5)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(text(t)),
            Block::Para(t) => div().child(text(t)),
            Block::Task(done, t) => {
                div().py(px(2.)).flex().items_start().gap(px(8.)).child(ui::checkbox(done).mt(px(3.))).child(div().flex_1().min_w_0().child(text(t)))
            }
            Block::Bullet(t) => div()
                .py(px(2.))
                .flex()
                .items_start()
                .gap(px(8.))
                .child(div().flex_none().text_color(TEXT_4).child("•"))
                .child(div().flex_1().min_w_0().child(text(t))),
            Block::Quote(t) => div().pl(px(8.)).border_l_2().border_color(SEPARATOR_STRONG).text_color(TEXT_3).child(text(t)),
            Block::Code(t) => div().px(px(8.)).py(px(6.)).rounded(px(7.)).bg(FILL_2).font_family(MONO).text_size(px(11.5)).text_color(TEXT_2).child(text(t)),
        };
        view.mt(px(space(i.checked_sub(1).map(|k| &blocks[k]), b)))
    });
    div().flex().flex_col().text_size(px(13.)).line_height(px(19.)).text_color(TEXT_BODY).children(views)
}

#[cfg(test)]
mod tests {
    use super::{Block, blocks, quote, space};

    #[test]
    fn a_description_reads_as_headings_paragraphs_and_tasks() {
        let md = "## Summary\nA member invites a teammate\nby email.\n\nSecond `para`.\n\n### Testing\n- [x] Accepted once\n- [ ] Expired link\n* plain bullet";
        assert_eq!(
            blocks(md),
            [
                Block::Heading("Summary".into()),
                Block::Para("A member invites a teammate by email.".into()),
                Block::Para("Second para.".into()),
                Block::Heading("Testing".into()),
                Block::Task(true, "Accepted once".into()),
                Block::Task(false, "Expired link".into()),
                Block::Bullet("plain bullet".into()),
            ]
        );
    }

    #[test]
    fn template_comments_and_bare_tags_are_left_out() {
        let md = "<!--\nThank you for contributing!\n-->\n<details>\n<!-- one line -->\nKept **bold** and [a link](https://x.y).\n</details>";
        assert_eq!(blocks(md), [Block::Para("Kept bold and a link.".into())]);
    }

    #[test]
    fn quotes_join_and_code_keeps_its_lines() {
        let md = "> one\n> two\n\n```rust\nfn a() {}\n  b();\n```\n```\nunclosed";
        assert_eq!(blocks(md), [Block::Quote("one\ntwo".into()), Block::Code("fn a() {}\n  b();".into()), Block::Code("unclosed".into())]);
    }

    #[test]
    fn a_bracket_that_is_not_a_link_stays_as_written() {
        assert_eq!(blocks("See [1] and [the docs](https://x.y), or [x]."), [Block::Para("See [1] and the docs, or [x].".into())]);
    }

    #[test]
    fn an_empty_description_has_no_blocks() {
        assert!(blocks("").is_empty());
        assert!(blocks("\n  \n<!-- -->").is_empty());
    }

    #[test]
    fn a_quote_reply_quotes_every_line_then_leaves_room_to_answer() {
        assert_eq!(quote("Does it remember?\n\nOr only the scroll?\n"), "> Does it remember?\n>\n> Or only the scroll?\n\n");
    }

    #[test]
    fn a_heading_sits_apart_from_what_came_before_and_list_items_sit_together() {
        let (h, p, t) = (Block::Heading(String::new()), Block::Para(String::new()), Block::Task(false, String::new()));
        assert_eq!([space(None, &h), space(Some(&p), &h), space(Some(&h), &p), space(Some(&p), &t), space(Some(&t), &t)], [0., 10., 4., 8., 0.]);
    }
}
