use gpui_kit::HighlightStyle;
use gpui_kit::component::highlighter::{LanguageConfig, LanguageRegistry, SyntaxHighlighter};
use gpui_kit::component::input::Rope;
use std::ops::Range;

pub type Spans = Vec<(Range<usize>, HighlightStyle)>;

/// tree-sitter-javascript's JSX query colours only lowercase tags, which leaves components like `<App>` and `<Foo.Bar>` bare.
const JSX_COMPONENTS: &str = r#"
(jsx_opening_element name: (identifier) @tag (#match? @tag "^[A-Z]"))
(jsx_closing_element name: (identifier) @tag (#match? @tag "^[A-Z]"))
(jsx_self_closing_element name: (identifier) @tag (#match? @tag "^[A-Z]"))
(jsx_opening_element name: (member_expression object: (identifier) @tag property: (property_identifier) @tag))
(jsx_closing_element name: (member_expression object: (identifier) @tag property: (property_identifier) @tag))
(jsx_self_closing_element name: (member_expression object: (identifier) @tag property: (property_identifier) @tag))
"#;

/// Gives tsx the full TypeScript highlights plus JSX tags; gpui-kit ships it with only tree-sitter-typescript's small TS add-on query.
/// Gives cpp C's highlights under its own, which tree-sitter-cpp's query extends rather than repeats.
/// Gives swift its grammar's own queries; gpui-kit ships it with none, so Swift came out uncoloured.
/// Adds the grammars gpui-kit lacks, and replaces the queries it ships empty or sparse, with the ones Zed's extensions use.
pub fn init() {
    let registry = LanguageRegistry::singleton();
    for (name, grammar, highlights, injections) in [
        ("vue", tree_sitter_vue_next::LANGUAGE.into(), include_str!("syntax/vue/highlights.scm"), include_str!("syntax/vue/injections.scm")),
        ("scss", tree_sitter_scss::LANGUAGE.into(), include_str!("syntax/scss/highlights.scm"), include_str!("syntax/scss/injections.scm")),
        ("dockerfile", tree_sitter_dockerfile_updated::language(), include_str!("syntax/dockerfile/highlights.scm"), include_str!("syntax/dockerfile/injections.scm")),
    ] {
        registry.register(name, &LanguageConfig::new(name, grammar, vec![], highlights, injections, ""));
    }
    for (name, highlights, injections) in [
        ("astro", include_str!("syntax/astro/highlights.scm"), include_str!("syntax/astro/injections.scm")),
        ("graphql", include_str!("syntax/graphql/highlights.scm"), ""),
        ("make", include_str!("syntax/make/highlights.scm"), include_str!("syntax/make/injections.scm")),
        ("proto", include_str!("syntax/proto/highlights.scm"), include_str!("syntax/proto/injections.scm")),
    ] {
        if let Some(mut config) = registry.language(name) {
            config.highlights = highlights.into();
            config.injections = injections.into();
            registry.register(name, &config);
        }
    }
    if let (Some(c), Some(mut cpp)) = (registry.language("c"), registry.language("cpp")) {
        cpp.highlights = format!("{}\n{}", c.highlights, cpp.highlights).into();
        registry.register("cpp", &cpp);
    }
    if let Some(mut swift) = registry.language("swift") {
        swift.highlights = tree_sitter_swift::HIGHLIGHTS_QUERY.into();
        swift.locals = tree_sitter_swift::LOCALS_QUERY.into();
        registry.register("swift", &swift);
    }
    let (Some(ts), Some(mut tsx)) = (registry.language("typescript"), registry.language("tsx")) else { return };
    tsx.highlights = format!("{}\n{}\n{JSX_COMPONENTS}", ts.highlights, tree_sitter_javascript::JSX_HIGHLIGHT_QUERY).into();
    tsx.injections = ts.injections;
    tsx.injection_languages = ts.injection_languages;
    registry.register("tsx", &tsx);
}

/// gpui-kit's grammar for a file, matched like Zed matches `path_suffixes`: the whole name, then what follows each dot. "text" when it has none.
pub fn language_for(path: &str) -> &'static str {
    let name = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
    if name.starts_with(".env") {
        return "bash";
    }
    let tails = std::iter::once(name.as_str()).chain(name.match_indices('.').map(|(i, _)| &name[i + 1..]));
    tails.map(grammar).find(|&g| g != "text").unwrap_or("text")
}

/// gpui-kit's grammar for a lowercase fence name, extension or file name, or "text" when it has none. The lists are Zed's `path_suffixes`.
pub fn grammar(name: &str) -> &'static str {
    match name {
        "astro" => "astro",
        "bash" | "sh" | "shell" | "console" | "zsh" | "bashrc" | "bash_profile" | "bash_aliases" | "bash_login" | "bash_logout" | "bats" | "envrc" | "profile" | "zshrc"
        | "zshenv" | "zsh_profile" | "zsh_aliases" | "zlogin" | "zprofile" | "pkgbuild" | "apkbuild" | "ebuild" => "bash",
        "c" => "c",
        "cpp" | "c++" | "cc" | "ccm" | "hh" | "cppm" | "h" | "hpp" | "cxx" | "cxxm" | "hxx" | "c++m" | "h++" | "hip" | "ipp" | "inl" | "ino" | "ixx" | "cu" | "cuh" => "cpp",
        "css" | "postcss" | "pcss" => "css",
        "diff" | "patch" => "diff",
        "dockerfile" | "containerfile" => "dockerfile",
        "go" | "golang" => "go",
        "graphql" | "gql" | "graphqls" => "graphql",
        "html" | "htm" | "shtml" => "html",
        "java" => "java",
        "javascript" | "js" | "jsx" | "mjs" | "cjs" => "javascript",
        "json" | "jsonc" | "json5" | "geojson" | "topojson" | "flake.lock" | "deno.lock" | "bun.lock" | "prettierrc" | "babelrc" | "eslintrc" | "stylelintrc" | "jshintrc"
        | "swcrc" | "luaurc" => "json",
        "kotlin" | "kt" | "kts" => "kotlin",
        "make" | "makefile" | "gnumakefile" | "ocamlmakefile" | "mk" | "mak" => "make",
        "markdown" | "md" | "mdx" | "mdwn" | "mdc" => "markdown",
        "php" | "phtml" | "phpt" => "php",
        "proto" | "protobuf" => "proto",
        "python" | "py" | "pyi" | "mpy" => "python",
        "ruby" | "rb" | "rbi" | "gemfile" | "guardfile" | "rake" | "rakefile" | "ru" | "thor" | "cap" | "capfile" | "jbuilder" | "rabl" | "rxml" | "builder" | "gemspec"
        | "irbrc" | "pryrc" | "simplecov" | "steepfile" | "podfile" | "brewfile" | "vagrantfile" | "puppetfile" | "fastfile" | "appfile" | "matchfile" | "cheffile"
        | "hobofile" | "appraisals" | "rantfile" | "berksfile" | "berksfile.lock" | "thorfile" | "dangerfile" | "deliverfile" | "scanfile" | "snapfile" | "gymfile" => "ruby",
        "rust" | "rs" => "rust",
        "scss" => "scss",
        "sql" => "sql",
        "svelte" => "svelte",
        "swift" | "swiftinterface" => "swift",
        "toml" | "cargo.lock" | "pipfile" | "uv.lock" => "toml",
        "tsx" => "tsx",
        "typescript" | "ts" | "mts" | "cts" => "typescript",
        "vue" => "vue",
        "yaml" | "yml" | "pixi.lock" | "clang-format" | "clangd" => "yaml",
        _ => "text",
    }
}

/// Syntax colours for each line of `text`, as byte ranges within that line. Parses the whole text, so run it off the UI thread.
pub fn line_spans(language: &str, text: &str) -> Vec<Spans> {
    let lines: Vec<&str> = text.lines().collect();
    let starts: Vec<usize> = lines.iter().map(|l| l.as_ptr() as usize - text.as_ptr() as usize).collect();
    let mut hl = SyntaxHighlighter::new(language);
    hl.update(None, &Rope::from(text), None);
    let mut out = vec![Vec::new(); lines.len()];
    for (r, style) in hl.styles(&(0..text.len()), &*theme::highlight_theme(theme::is_dark())) {
        if style.color.is_none() {
            continue;
        }
        let mut i = starts.partition_point(|&s| s <= r.start).saturating_sub(1);
        while i < lines.len() && starts[i] < r.end {
            let (a, b) = (r.start.max(starts[i]) - starts[i], r.end.min(starts[i] + lines[i].len()).saturating_sub(starts[i]));
            if a < b {
                out[i].push((a..b, style));
            }
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{Spans, grammar, language_for, line_spans};
    use std::ops::Range;
    use theme::{SYN_COMMENT, SYN_FN, SYN_KEYWORD, SYN_STRING, Token};

    /// Whether every byte of `word`'s first appearance on `line` is coloured `c`.
    fn coloured(spans: &[Spans], text: &str, line: usize, word: &str, c: Token) -> bool {
        let at = text.lines().nth(line).and_then(|l| l.find(word)).unwrap_or_else(|| panic!("{word:?} is on line {line}"));
        (at..at + word.len()).all(|b| spans[line].iter().any(|(r, s)| r.contains(&b) && s.color == Some(c.into())))
    }

    #[test]
    fn colours_each_line_by_its_own_offsets() {
        let spans = line_spans("rust", "fn main() {\n    // hi\n}\n");
        assert_eq!(spans.len(), 3);
        let has = |line: usize, range: Range<usize>, c: Token| spans[line].iter().any(|(r, s)| *r == range && s.color == Some(c.into()));
        assert!(has(0, 0..2, SYN_KEYWORD));
        assert!(has(0, 3..7, SYN_FN));
        assert!(has(1, 4..9, SYN_COMMENT));
    }

    #[test]
    fn colours_tsx_keywords_strings_and_jsx_tags() {
        super::init();
        let spans = line_spans("tsx", "const a = \"x\";\nreturn <div className=\"y\">{'z'}<App /></div>;\n<Foo.Bar />;\n");
        let has = |line: usize, range: Range<usize>, c: Token| spans[line].iter().any(|(r, s)| *r == range && s.color == Some(c.into()));
        assert!(has(0, 0..5, SYN_KEYWORD) && has(1, 0..6, SYN_KEYWORD), "const, return");
        assert!(has(0, 10..13, SYN_STRING) && has(1, 22..25, SYN_STRING) && has(1, 27..30, SYN_STRING), "strings");
        assert!(has(1, 8..11, SYN_FN) && has(1, 40..43, SYN_FN), "div tags");
        assert!(has(1, 12..21, SYN_KEYWORD), "className");
        assert!(has(1, 32..35, SYN_FN) && has(2, 1..4, SYN_FN) && has(2, 5..8, SYN_FN), "component tags");
    }

    #[test]
    fn colours_swift_keywords_strings_and_comments() {
        super::init();
        let spans = line_spans("swift", "import SwiftUI\nlet s = \"hi\" // note\n");
        let has = |line: usize, range: Range<usize>, c: Token| spans[line].iter().any(|(r, s)| *r == range && s.color == Some(c.into()));
        assert!(has(0, 0..6, SYN_KEYWORD) && has(1, 0..3, SYN_KEYWORD), "import, let");
        assert!(has(1, 9..11, SYN_STRING), "string");
        assert!(has(1, 13..20, SYN_COMMENT), "comment");
    }

    #[test]
    fn keeps_ranges_inside_crlf_lines_and_plain_text_bare() {
        let spans = line_spans("rust", "fn a() {}\r\nfn b() {}\r\n");
        assert!(spans.iter().flatten().all(|(r, _)| r.end <= 9));
        assert_eq!(line_spans("text", "a\nb"), vec![vec![], vec![]]);
    }

    #[test]
    fn colours_vue_templates_scripts_and_styles_each_in_their_own_language() {
        super::init();
        let text = "<template>\n  <div v-if=\"ok\" class=\"c\">{{ n + 1 }}</div>\n  <!-- hi -->\n</template>\n<script lang=\"ts\">\nconst s: string = \"x\" // note\n</script>\n<script>\nlet j = 'y'\n</script>\n<style lang=\"scss\">\n@use \"sass:math\"; // sc\n</style>\n<style>\np { color: red; }\n</style>\n";
        let spans = line_spans("vue", text);
        let is = |line, word, c| coloured(&spans, text, line, word, c);
        assert!(is(1, "div", SYN_FN) && is(1, "v-if", SYN_KEYWORD) && is(1, "class", SYN_KEYWORD) && is(1, "\"c\"", SYN_STRING), "template");
        assert!(is(1, "1", SYN_STRING) && is(2, "<!-- hi -->", SYN_COMMENT), "template interpolation and comment");
        assert!(is(5, "const", SYN_KEYWORD) && is(5, "string", SYN_FN) && is(5, "\"x\"", SYN_STRING) && is(5, "// note", SYN_COMMENT), "script lang=ts");
        assert!(is(8, "let", SYN_KEYWORD) && is(8, "'y'", SYN_STRING), "script");
        assert!(is(11, "@use", SYN_KEYWORD) && is(11, "\"sass:math\"", SYN_STRING) && is(11, "// sc", SYN_COMMENT), "style lang=scss");
        assert!(is(14, "p", SYN_FN), "style");
    }

    #[test]
    fn colours_svelte_scripts_blocks_and_styles() {
        super::init();
        let text = "<script lang=\"ts\">\n  let n: number = 1; // c\n</script>\n{#if n}<p class=\"x\">{n}</p>{/if}\n<style>p { color: red; }</style>\n";
        let spans = line_spans("svelte", text);
        let is = |line, word, c| coloured(&spans, text, line, word, c);
        assert!(is(1, "let", SYN_KEYWORD) && is(1, "1", SYN_STRING) && is(1, "// c", SYN_COMMENT), "script lang=ts");
        assert!(is(3, "if", SYN_KEYWORD) && is(3, "p", SYN_FN) && is(3, "class", SYN_KEYWORD) && is(3, "x", SYN_STRING), "markup");
        assert!(is(4, "style", SYN_FN) && is(4, "p", SYN_FN), "style");
    }

    #[test]
    fn colours_astro_frontmatter_markup_and_styles() {
        super::init();
        let text = "---\nconst t = \"hi\"; // c\n---\n<h1 class=\"x\">{t}</h1>\n<style>h1 { color: red; }</style>\n";
        let spans = line_spans("astro", text);
        let is = |line, word, c| coloured(&spans, text, line, word, c);
        assert!(is(1, "const", SYN_KEYWORD) && is(1, "\"hi\"", SYN_STRING) && is(1, "// c", SYN_COMMENT), "frontmatter");
        assert!(is(3, "h1", SYN_FN) && is(3, "class", SYN_KEYWORD) && is(3, "\"x\"", SYN_STRING), "markup");
        assert!(is(4, "style", SYN_FN) && is(4, "h1", SYN_FN), "style");
    }

    #[test]
    fn colours_keywords_strings_and_comments_in_each_added_language() {
        super::init();
        for (language, text, keyword, string, comment) in [
            ("scss", "@use \"sass:math\"; // c\n", "@use", "\"sass:math\"", "// c"),
            ("sql", "SELECT a FROM b WHERE c = 'x'; -- c\n", "SELECT", "'x'", "-- c"),
            ("php", "<?php function f() { return \"s\"; } // c\n", "function", "\"s\"", "// c"),
            ("ruby", "def f; \"s\" end # c\n", "def", "\"s\"", "# c"),
            ("c", "int main() { return \"s\"[0]; } // c\n", "return", "\"s\"", "// c"),
            ("cpp", "class A {}; auto s = \"x\"; // c\n", "class", "\"x\"", "// c"),
            ("dockerfile", "RUN echo \"hi\"\n", "RUN", "\"hi\"", ""),
            ("dockerfile", "# c\n", "", "", "# c"),
            ("graphql", "query Q { user(id: \"1\") { name } } # c\n", "query", "\"1\"", "# c"),
            ("proto", "message M { string a = 1; } // c\n", "message", "", "// c"),
            ("proto", "syntax = \"proto3\";\n", "syntax", "\"proto3\"", ""),
            ("make", "export A := b\n", "export", "", ""),
            ("make", "# c\n", "", "", "# c"),
        ] {
            let spans = line_spans(language, text);
            for (word, c) in [(keyword, SYN_KEYWORD), (string, SYN_STRING), (comment, SYN_COMMENT)] {
                assert!(word.is_empty() || coloured(&spans, text, 0, word, c), "{language}: {word}");
            }
        }
    }

    #[test]
    fn maps_paths_to_grammars_by_extension_or_name() {
        for (path, language) in [
            ("src/a.tsx", "tsx"),
            ("README.md", "markdown"),
            ("src/App.vue", "vue"),
            ("src/routes/+page.svelte", "svelte"),
            ("src/pages/index.astro", "astro"),
            ("styles/main.scss", "scss"),
            ("types/env.d.ts", "typescript"),
            ("tsconfig.jsonc", "json"),
            ("include/a.h", "cpp"),
            ("Dockerfile", "dockerfile"),
            ("Makefile", "make"),
            ("Gemfile", "ruby"),
            ("Cargo.lock", "toml"),
            ("/Users/me/.zshrc", "bash"),
            (".env.local", "bash"),
            ("notes.txt", "text"),
            (".gitignore", "text"),
        ] {
            assert_eq!(language_for(path), language, "{path}");
        }
    }

    #[test]
    fn resolves_fence_names_through_the_same_table() {
        for (fence, language) in
            [("js", "javascript"), ("ts", "typescript"), ("py", "python"), ("rb", "ruby"), ("yml", "yaml"), ("sh", "bash"), ("shell", "bash"), ("console", "bash"), ("diff", "diff"), ("vue", "vue"), ("svelte", "svelte"), ("brainfuck", "text")]
        {
            assert_eq!(grammar(fence), language, "{fence}");
        }
    }
}

