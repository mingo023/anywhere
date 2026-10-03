const STARTS: [&str; 5] = ["https://", "http://", "localhost:", "127.0.0.1:", "0.0.0.0:"];

/// The link covering char `at` of `line`: an http(s) URL, or a local `host:port` given `http://`.
pub fn link_at(line: &str, at: usize) -> Option<String> {
    let chars: Vec<char> = line.chars().collect();
    let in_url = |c: &char| c.is_ascii_alphanumeric() || "-._~:/?#[]@!$&'()*+,;=%".contains(*c);
    if !chars.get(at).is_some_and(in_url) {
        return None;
    }
    let start = chars[..at].iter().rposition(|c| !in_url(c)).map_or(0, |i| i + 1);
    let end = chars[at..].iter().position(|c| !in_url(c)).map_or(chars.len(), |i| at + i);
    let word: String = chars[start..end].iter().collect();
    let at = at - start;
    let (from, prefix) = STARTS.iter().flat_map(|p| word.match_indices(p)).filter(|(i, _)| *i <= at).max_by_key(|(i, _)| *i)?;
    let link = trim(&word[from..]);
    let rest = &link[prefix.len()..];
    let local = !prefix.ends_with("//");
    if at >= from + link.len() || rest.is_empty() || (local && !rest.starts_with(|c: char| c.is_ascii_digit())) {
        return None;
    }
    Some(if local { format!("http://{link}") } else { link.to_string() })
}

/// Drops punctuation that ends the sentence rather than the URL, and a closing bracket the URL didn't open.
fn trim(mut url: &str) -> &str {
    while let Some(last) = url.chars().last() {
        let unopened = |open: char| url.matches(open).count() < url.matches(last).count();
        let drop = match last {
            ')' => unopened('('),
            ']' => unopened('['),
            _ => ".,;:!?'*".contains(last),
        };
        if !drop {
            break;
        }
        url = &url[..url.len() - 1];
    }
    url
}

#[cfg(test)]
mod tests {
    use super::link_at;

    fn at(line: &str, needle: &str) -> Option<String> {
        link_at(line, line.chars().count() - line.split_once(needle).unwrap().1.chars().count() - 1)
    }

    #[test]
    fn finds_the_url_under_any_of_its_chars() {
        let line = "  ➜  Local:   https://example.com/a?b=c#d  ";
        assert_eq!(at(line, "h").as_deref(), Some("https://example.com/a?b=c#d"));
        assert_eq!(at(line, "#").as_deref(), Some("https://example.com/a?b=c#d"));
        assert_eq!(at(line, "L"), None);
    }

    #[test]
    fn a_local_host_and_port_is_a_link() {
        assert_eq!(at("ready on localhost:5173/app", "5").as_deref(), Some("http://localhost:5173/app"));
        assert_eq!(at("bound 0.0.0.0:8080", "8").as_deref(), Some("http://0.0.0.0:8080"));
        assert_eq!(at("see localhost:docs", "d"), None);
    }

    #[test]
    fn sentence_punctuation_and_unopened_brackets_stay_out() {
        assert_eq!(at("Open https://example.com/x.", "x").as_deref(), Some("https://example.com/x"));
        assert_eq!(at("[docs](https://example.com/a_(b))", "a").as_deref(), Some("https://example.com/a_(b)"));
        assert_eq!(at("'http://example.com'", "e").as_deref(), Some("http://example.com"));
        assert_eq!(link_at("Open https://example.com/x.", 26), None);
    }

    #[test]
    fn text_before_a_url_in_the_same_word_is_not_a_link() {
        assert_eq!(at("url=https://example.com", "u"), None);
        assert_eq!(at("url=https://example.com", "e").as_deref(), Some("https://example.com"));
        assert_eq!(at("just https://", "s"), None);
    }
}
