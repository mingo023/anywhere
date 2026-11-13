//! Picked page elements as a prompt carries them: the block the agent reads, and the chip headings read back from it.

const TITLE: &str = "# Selected elements\n\n";
const TITLE_CHARS: usize = 60;

/// Picked elements as the agent reads them, each headed by its chip: number, host and title.
pub(crate) fn block(elements: &[web::Element]) -> String {
    let sections = elements.iter().enumerate().map(|(i, e)| {
        let fence = fence_for(&e.html);
        let text = if e.text.is_empty() { String::new() } else { format!("Text: {}\n", e.text) };
        format!("## {} {} › {}\nURL: {}\nSelector: `{}`\nSize: {} × {}\n{text}{fence}html\n{}\n{fence}", i + 1, web::host(&e.url), title(e), e.url, e.selector, e.width, e.height, e.html)
    });
    format!("{TITLE}{}", sections.collect::<Vec<_>>().join("\n\n"))
}

/// What a chip calls an element: its words, else its tag and id or class; one line, as its heading needs, cut at a word.
pub(crate) fn title(e: &web::Element) -> String {
    let words = e.text.split_whitespace().collect::<Vec<_>>().join(" ");
    let title = if words.is_empty() { e.name.as_str() } else { words.as_str() };
    if title.chars().count() <= TITLE_CHARS {
        return title.to_string();
    }
    let cut: String = title.chars().take(TITLE_CHARS).collect();
    let kept = cut.rsplit_once(' ').map_or(cut.as_str(), |(head, _)| head);
    format!("{}…", kept.trim_end())
}

/// A fence longer than any run of backticks in `body`, which so can't close it.
fn fence_for(body: &str) -> String {
    let longest = body.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    "`".repeat(longest.max(2) + 1)
}

/// `prompt` without the picked elements that end it, and their headings; a `## ` inside a fence is HTML, not one.
pub(crate) fn split(prompt: &str) -> (&str, Vec<&str>) {
    let (end, start) = match prompt.rfind(&format!("\n\n{TITLE}")) {
        Some(i) => (i, i + 2),
        None if prompt.starts_with(TITLE) => (0, 0),
        None => return (prompt, Vec::new()),
    };
    let (mut fenced, mut headings) = (None, Vec::new());
    for line in prompt[start + TITLE.len()..].lines() {
        match fenced {
            Some(fence) if line == fence => fenced = None,
            Some(_) => {}
            None if line.starts_with("```") => fenced = Some(line.trim_end_matches(|c| c != '`')),
            None => headings.extend(line.strip_prefix("## ")),
        }
    }
    if headings.is_empty() { (prompt, Vec::new()) } else { (&prompt[..end], headings) }
}

/// A sent element's heading, `<n> <host> › <title>`, as its parts.
pub(crate) fn heading_parts(heading: &str) -> (&str, &str, &str) {
    let Some((number, rest)) = heading.split_once(' ') else { return ("", "", heading) };
    match rest.split_once(" › ") {
        Some((host, title)) => (number, host, title),
        None => ("", "", heading),
    }
}

#[cfg(test)]
mod tests {
    use super::{block, heading_parts, title};

    fn element(text: &str, html: &str) -> web::Element {
        web::Element { url: "https://a.dev/".into(), selector: "div".into(), name: "div.hero".into(), text: text.into(), html: html.into(), width: 1, height: 1 }
    }

    #[test]
    fn an_element_with_no_words_is_titled_by_its_tag_and_class_and_says_no_text() {
        let e = element("", "<div></div>");
        assert_eq!(title(&e), "div.hero");
        assert!(!block(&[e]).contains("Text:"));
    }

    #[test]
    fn a_long_title_is_cut_at_a_word() {
        let e = element("Review changes Direct the fix Check the results Approve and merge See it shipped", "");
        assert_eq!(title(&e), "Review changes Direct the fix Check the results Approve and…");
    }

    #[test]
    fn html_holding_backticks_gets_a_longer_fence() {
        let block = block(&[element("", "<code>```</code>")]);
        assert!(block.ends_with("````html\n<code>```</code>\n````"), "{block}");
    }

    #[test]
    fn a_sent_elements_heading_gives_back_its_number_host_and_title() {
        assert_eq!(heading_parts("1 github.com › Enter your email"), ("1", "github.com", "Enter your email"));
        assert_eq!(heading_parts("2 a.dev › a › b"), ("2", "a.dev", "a › b"));
        assert_eq!(heading_parts("odd"), ("", "", "odd"));
    }
}
