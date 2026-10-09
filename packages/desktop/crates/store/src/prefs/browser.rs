use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    #[default]
    Google,
    DuckDuckGo,
    Bing,
    Kagi,
    /// `Browser::custom_search`.
    Custom,
}

impl Engine {
    /// The search URL, `%s` standing for the encoded query; Custom's lives in `Browser`, so it reads as Google.
    fn template(self) -> &'static str {
        match self {
            Engine::Google | Engine::Custom => "https://www.google.com/search?q=%s",
            Engine::DuckDuckGo => "https://duckduckgo.com/?q=%s",
            Engine::Bing => "https://www.bing.com/search?q=%s",
            Engine::Kagi => "https://kagi.com/search?q=%s",
        }
    }
}

/// Why a custom search URL can't be used, or `None` when it can.
pub fn search_problem(template: &str) -> Option<&'static str> {
    let t = template.trim();
    if !(t.starts_with("https://") || t.starts_with("http://")) {
        Some("Start it with https:// \u{b7} Google searches until then")
    } else if !t.contains("%s") {
        Some("Put %s where the search goes \u{b7} Google searches until then")
    } else {
        None
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub enum Home {
    #[default]
    DevServer,
    Blank,
    Custom,
}

/// The built-in browser.
#[derive(Serialize, Deserialize, Default, Debug, PartialEq, Clone)]
#[serde(default)]
pub struct Browser {
    #[serde(deserialize_with = "crate::lenient")]
    pub engine: Engine,
    #[serde(deserialize_with = "crate::lenient")]
    pub home: Home,
    pub custom_home: String,
    pub custom_search: String,
}

impl Browser {
    /// What a new tab loads: the project's dev server or the custom page, else nothing so the address bar takes focus.
    pub fn home_page(&self, dev_url: &str) -> Option<String> {
        let url = match self.home {
            Home::DevServer => dev_url,
            Home::Custom => &self.custom_home,
            Home::Blank => "",
        };
        Some(url.trim().to_string()).filter(|u| !u.is_empty())
    }

    /// Where the address bar searches, `%s` standing for the query: the custom URL once it's usable, else the engine's.
    pub fn search_url(&self) -> &str {
        match self.engine {
            Engine::Custom if search_problem(&self.custom_search).is_none() => self.custom_search.trim(),
            e => e.template(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_tab_opens_the_dev_server_or_custom_page_else_blank() {
        let at = |home, custom: &str| Browser { home, custom_home: custom.into(), ..Browser::default() };
        assert_eq!(at(Home::DevServer, "x.com").home_page("localhost:8081"), Some("localhost:8081".into()));
        assert_eq!(at(Home::DevServer, "x.com").home_page(" "), None);
        assert_eq!(at(Home::Custom, "x.com").home_page("localhost:8081"), Some("x.com".into()));
        assert_eq!(at(Home::Blank, "x.com").home_page("localhost:8081"), None);
    }

    #[test]
    fn a_browser_from_before_these_settings_searches_google_and_opens_blank_until_a_dev_server_is_set() {
        let b: Browser = serde_json::from_str(r#"{"engine":"altavista"}"#).unwrap();
        assert_eq!((b.engine, b.home, b.home_page("")), (Engine::Google, Home::DevServer, None));
    }

    #[test]
    fn a_custom_search_url_is_used_only_once_it_has_a_scheme_and_a_slot() {
        let custom = |url: &str| Browser { engine: Engine::Custom, custom_search: url.into(), ..Browser::default() };
        assert_eq!(custom(" https://search.brave.com/search?q=%s ").search_url(), "https://search.brave.com/search?q=%s");
        assert_eq!(custom("https://search.brave.com/search?q=").search_url(), Engine::Google.template());
        assert_eq!(custom("search.brave.com/?q=%s").search_url(), Engine::Google.template());
        assert_eq!(Browser { engine: Engine::Kagi, custom_search: "https://x.com/?q=%s".into(), ..Browser::default() }.search_url(), "https://kagi.com/search?q=%s");
    }
}
