/// What to load for text typed in the address bar: a URL as is, a host with a scheme added (`http` for a local one), anything else as a search.
pub fn resolve(input: &str) -> String {
    let input = input.trim();
    if input.contains("://") || input.starts_with("about:") {
        return input.to_string();
    }
    if !input.contains(char::is_whitespace) && is_host(authority(input)) {
        let scheme = if is_local(authority(input)) { "http" } else { "https" };
        return format!("{scheme}://{input}");
    }
    format!("https://www.google.com/search?q={}", encode(input))
}

/// The host and port of `url`, to name a page with no title yet.
pub fn host(url: &str) -> &str {
    authority(url.split_once("://").map_or(url, |(_, rest)| rest))
}

fn authority(s: &str) -> &str {
    s.split(['/', '?', '#']).next().unwrap_or(s)
}

fn split_port(authority: &str) -> (&str, &str) {
    authority.rsplit_once(':').unwrap_or((authority, ""))
}

fn is_host(authority: &str) -> bool {
    let (host, port) = split_port(authority);
    let named = host == "localhost" || (host.contains('.') && !host.starts_with('.') && !host.ends_with('.'));
    named && port.bytes().all(|b| b.is_ascii_digit())
}

fn is_local(authority: &str) -> bool {
    let (host, _) = split_port(authority);
    matches!(host, "localhost" | "127.0.0.1" | "0.0.0.0") || host.ends_with(".localhost")
}

fn encode(query: &str) -> String {
    query
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            b' ' => "+".into(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_url_with_a_scheme_loads_as_typed() {
        assert_eq!(resolve(" https://example.com/a?b=c "), "https://example.com/a?b=c");
        assert_eq!(resolve("about:blank"), "about:blank");
    }

    #[test]
    fn a_bare_host_gets_https_and_a_local_one_http() {
        assert_eq!(resolve("example.com/docs"), "https://example.com/docs");
        assert_eq!(resolve("localhost:3000"), "http://localhost:3000");
        assert_eq!(resolve("127.0.0.1:8080/api"), "http://127.0.0.1:8080/api");
        assert_eq!(resolve("app.localhost"), "http://app.localhost");
    }

    #[test]
    fn anything_else_is_searched() {
        assert_eq!(resolve("rust borrow checker"), "https://www.google.com/search?q=rust+borrow+checker");
        assert_eq!(resolve("gpui"), "https://www.google.com/search?q=gpui");
        assert_eq!(resolve("c++ & go"), "https://www.google.com/search?q=c%2B%2B+%26+go");
        assert_eq!(resolve("host.com:abc"), "https://www.google.com/search?q=host.com%3Aabc");
    }

    #[test]
    fn the_host_keeps_its_port_and_drops_the_path() {
        assert_eq!(host("http://localhost:3000/login?next=/"), "localhost:3000");
        assert_eq!(host("https://example.com"), "example.com");
    }
}
