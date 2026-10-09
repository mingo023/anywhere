use crate::lenient;
use serde::{Deserialize, Serialize};

/// Terminal text, colours and behaviour.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
#[serde(default)]
pub struct Terminal {
    /// `None` follows Appearance's code font.
    pub font: Option<String>,
    pub size: i32,
    /// Percent of the font size; the row height rounds to whole pixels.
    pub line_height: i32,
    /// Pixels beside the text; above and below get half, as rows already centre in what's left.
    pub padding: i32,
    pub ligatures: bool,
    #[serde(deserialize_with = "lenient")]
    pub scheme: Scheme,
    /// ANSI colours 0–15 as 0xRRGGBB, used while `scheme` is Custom.
    #[serde(deserialize_with = "lenient")]
    pub custom: [u32; 16],
    #[serde(deserialize_with = "lenient")]
    pub cursor: Cursor,
    pub blink: bool,
    /// Which Option keys send Esc before the key, as Meta, instead of typing its character.
    #[serde(deserialize_with = "meta")]
    pub option_as_meta: Meta,
    pub copy_on_select: bool,
    pub confirm_paste: bool,
    /// A plain click opens a link; otherwise it takes ⌘-click.
    pub click_opens_links: bool,
    pub system_browser: bool,
    pub confirm_close: bool,
    pub scrollback: i32,
    #[serde(deserialize_with = "lenient")]
    pub bell: Bell,
    #[serde(deserialize_with = "lenient")]
    pub shell: Shell,
    pub shell_path: String,
    pub shell_args: String,
}

impl Default for Terminal {
    fn default() -> Self {
        Self {
            font: Some("Geist Mono".into()),
            size: 13,
            line_height: 130,
            padding: 4,
            ligatures: false,
            scheme: Scheme::Default,
            custom: Scheme::Default.colors(),
            cursor: Cursor::Block,
            blink: true,
            option_as_meta: Meta::Both,
            copy_on_select: false,
            confirm_paste: true,
            click_opens_links: false,
            system_browser: false,
            confirm_close: true,
            scrollback: 10_000,
            bell: Bell::Off,
            shell: Shell::Login,
            shell_path: String::new(),
            shell_args: "--login".into(),
        }
    }
}

impl Terminal {
    /// The row height in whole pixels, so cells stay on the pixel grid.
    pub fn line(&self) -> f32 {
        (self.size as f32 * self.line_height as f32 / 100.).round()
    }

    /// ANSI colours 0–15 as 0xRRGGBB.
    pub fn colors(&self) -> [u32; 16] {
        if self.scheme == Scheme::Custom { self.custom } else { self.scheme.colors() }
    }

    /// Colour `i` of a custom scheme, which starts from the colours on show.
    pub fn set_color(&mut self, i: usize, color: u32) {
        if i >= self.custom.len() {
            return;
        }
        self.custom = self.colors();
        self.custom[i] = color & 0xffffff;
        self.scheme = Scheme::Custom;
    }

    /// What a new terminal runs: the custom shell while `runs` says it can, else the login shell.
    pub fn shell_command(&self, login: &str, runs: impl Fn(&str) -> bool) -> (String, Vec<String>) {
        let path = self.shell_path.trim();
        if self.shell == Shell::Custom && runs(path) {
            return (path.to_string(), self.shell_args.split_whitespace().map(str::to_string).collect());
        }
        (login.to_string(), vec!["-l".into()])
    }

    /// The custom shell a terminal that `shell_command` sent to `program` didn't open.
    pub fn skipped(&self, program: &str) -> Option<&str> {
        (self.shell == Shell::Custom && program != self.shell_path.trim()).then_some(self.shell_path.as_str())
    }
}

/// An absolute path to a file someone may execute.
pub fn runs(path: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::path::Path::new(path).is_absolute() && std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// Which Option keys act as Meta in a terminal.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Meta {
    Off,
    #[default]
    Both,
    Left,
    Right,
}

impl Meta {
    pub const ALL: [Meta; 4] = [Meta::Off, Meta::Both, Meta::Left, Meta::Right];

    /// Whether Option, held on these sides, is Meta; a side macOS didn't report counts as both.
    pub fn takes(self, left: bool, right: bool) -> bool {
        let (left, right) = if left || right { (left, right) } else { (true, true) };
        match self {
            Meta::Off => false,
            Meta::Both => true,
            Meta::Left => left,
            Meta::Right => right,
        }
    }
}

/// Reads the switch it was before Left and Right: on is both keys.
fn meta<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Meta, D::Error> {
    Ok(match serde_json::Value::deserialize(d)? {
        serde_json::Value::Bool(on) => if on { Meta::Both } else { Meta::Off },
        v => serde_json::from_value(v).unwrap_or_default(),
    })
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Shell {
    #[default]
    Login,
    Custom,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Scheme {
    /// libghostty's own colours.
    #[default]
    Default,
    Graphite,
    Solarized,
    /// `Terminal::custom`.
    Custom,
}

impl Scheme {
    pub const ALL: [Scheme; 4] = [Scheme::Default, Scheme::Graphite, Scheme::Solarized, Scheme::Custom];

    /// A preset's ANSI colours 0–15 as 0xRRGGBB; Custom's live in `Terminal`, so it reads as Default.
    fn colors(self) -> [u32; 16] {
        match self {
            Scheme::Default | Scheme::Custom => [
                0x1d1f21, 0xcc6666, 0xb5bd68, 0xf0c674, 0x81a2be, 0xb294bb, 0x8abeb7, 0xc5c8c6, 0x666666, 0xd54e53, 0xb9ca4a, 0xe7c547, 0x7aa6da, 0xc397d8, 0x70c0b1, 0xeaeaea,
            ],
            Scheme::Graphite => [
                0x1c1c1f, 0xe5484d, 0x30a46c, 0xe8a317, 0x3e63dd, 0x8e4ec6, 0x0e7c86, 0xd4d4d8, 0x6f6f78, 0xff6369, 0x4cc38a, 0xffc53d, 0x8da4ff, 0xc18af5, 0x3dd6c8, 0xffffff,
            ],
            Scheme::Solarized => [
                0x073642, 0xdc322f, 0x859900, 0xb58900, 0x268bd2, 0xd33682, 0x2aa198, 0xeee8d5, 0x002b36, 0xcb4b16, 0x586e75, 0x657b83, 0x839496, 0x6c71c4, 0x93a1a1, 0xfdf6e3,
            ],
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Cursor {
    #[default]
    Block,
    Bar,
    Underline,
}

/// What a terminal does when a program rings its bell.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Bell {
    #[default]
    Off,
    Sound,
    Flash,
}

impl Bell {
    pub const ALL: [Bell; 3] = [Bell::Off, Bell::Sound, Bell::Flash];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_keep_todays_thirteen_point_seventeen_pixel_rows() {
        let t = Terminal::default();
        assert_eq!((t.font.as_deref(), t.size, t.line(), t.padding), (Some("Geist Mono"), 13, 17., 4));
    }

    #[test]
    fn rows_round_to_whole_pixels() {
        let t = Terminal { size: 15, line_height: 120, ..Terminal::default() };
        assert_eq!(t.line(), 18.);
    }

    #[test]
    fn an_unknown_scheme_falls_back_without_losing_the_rest() {
        let t: Terminal = serde_json::from_str(r#"{"scheme": "neon", "size": 15}"#).unwrap();
        assert_eq!((t.scheme, t.size, t.confirm_close), (Scheme::Default, 15, true));
    }

    #[test]
    fn the_option_switch_from_before_left_and_right_keeps_its_meaning() {
        let read = |json: &str| serde_json::from_str::<Terminal>(json).unwrap().option_as_meta;
        assert_eq!((read(r#"{"option_as_meta": true}"#), read(r#"{"option_as_meta": false}"#)), (Meta::Both, Meta::Off));
        assert_eq!((read(r#"{"option_as_meta": "right"}"#), read(r#"{"option_as_meta": "up"}"#)), (Meta::Right, Meta::Both));
    }

    #[test]
    fn one_sided_meta_leaves_the_other_option_key_typing() {
        assert!(Meta::Left.takes(true, false) && !Meta::Left.takes(false, true));
        assert!(Meta::Right.takes(false, true) && !Meta::Right.takes(true, false));
        assert!(Meta::Both.takes(false, true) && !Meta::Off.takes(true, true));
        assert!(Meta::Right.takes(false, false), "an unreported side counts as both");
    }

    #[test]
    fn editing_a_color_starts_a_custom_scheme_from_the_one_on_show() {
        let mut t = Terminal { scheme: Scheme::Graphite, ..Terminal::default() };
        let graphite = t.colors();
        t.set_color(1, 0xff00ff);
        assert_eq!((t.scheme, t.colors()[1], t.colors()[2]), (Scheme::Custom, 0xff00ff, graphite[2]));
        t.scheme = Scheme::Solarized;
        assert_ne!(t.colors()[1], 0xff00ff);
    }

    #[test]
    fn a_custom_shell_runs_with_its_arguments_and_falls_back_when_it_cannot() {
        let mut t = Terminal { shell: Shell::Custom, shell_path: " /opt/homebrew/bin/fish ".into(), shell_args: "--login  -i".into(), ..Terminal::default() };
        assert_eq!(t.shell_command("/bin/zsh", |_| true), ("/opt/homebrew/bin/fish".into(), vec!["--login".into(), "-i".into()]));
        assert_eq!(t.shell_command("/bin/zsh", |_| false), ("/bin/zsh".into(), vec!["-l".into()]));
        t.shell = Shell::Login;
        assert_eq!(t.shell_command("/bin/zsh", |_| true).0, "/bin/zsh");
    }

    #[test]
    fn a_terminal_names_the_custom_shell_it_could_not_open() {
        let mut t = Terminal { shell: Shell::Custom, shell_path: " /opt/homebrew/bin/fish ".into(), ..Terminal::default() };
        assert_eq!(t.skipped("/opt/homebrew/bin/fish"), None);
        assert_eq!(t.skipped("/bin/zsh"), Some(" /opt/homebrew/bin/fish "));
        t.shell = Shell::Login;
        assert_eq!(t.skipped("/bin/zsh"), None);
    }

    #[test]
    fn a_colour_beyond_the_sixteen_is_ignored() {
        let mut t = Terminal::default();
        t.set_color(16, 0xff00ff);
        assert_eq!(t, Terminal::default());
    }

    #[test]
    fn only_an_absolute_executable_file_runs() {
        assert!(runs("/bin/sh"));
        assert!(!runs("sh") && !runs("/bin") && !runs("/etc/hosts") && !runs("/no/such/shell"));
    }
}
