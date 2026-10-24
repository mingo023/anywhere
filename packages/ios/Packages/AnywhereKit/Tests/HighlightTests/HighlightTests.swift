import Testing
@testable import Highlight

struct Token: Equatable, CustomStringConvertible {
    let text: String
    let kind: TokenKind
    init(_ text: String, _ kind: TokenKind) {
        self.text = text
        self.kind = kind
    }

    var description: String { "\(kind):\(text.debugDescription)" }
}

func tokens(_ code: String, _ language: String) -> [Token] {
    let units = Array(code.utf16)
    return Highlighter.spans(code, language: language).map { Token(String(decoding: units[$0.range], as: UTF16.self), $0.kind) }
}

/// Every span of `code` reads as `expected` says, in order; spans not listed may sit between them.
func expectTokens(_ code: String, _ language: String, _ expected: [Token], sourceLocation: SourceLocation = #_sourceLocation) {
    let found = tokens(code, language)
    var at = 0
    for token in expected {
        guard let k = found[at...].firstIndex(of: token) else {
            Issue.record("\(token) not found after \(at) in \(found)", sourceLocation: sourceLocation)
            return
        }
        at = k + 1
    }
}

func expectPlain(_ code: String, _ language: String, _ text: String, sourceLocation: SourceLocation = #_sourceLocation) {
    let found = tokens(code, language)
    #expect(!found.contains { $0.text.contains(text) }, "\(text) should be plain in \(found)", sourceLocation: sourceLocation)
}

@Suite struct Languages {
    @Test("a path or fence label names its language", arguments: [
        ("sh", "bash"), ("shell", "bash"), ("zsh", "bash"), ("console", "bash"), ("Bash", "bash"), ("deploy.sh", "bash"),
        ("js", "javascript"), ("mjs", "javascript"), ("cjs", "javascript"), ("jsx", "javascript"), ("App.jsx", "javascript"),
        ("ts", "typescript"), ("mts", "typescript"), ("cts", "typescript"), ("src/index.ts", "typescript"), ("tsx", "tsx"), ("View.tsx", "tsx"),
        ("yml", "yaml"), (".github/workflows/ci.yml", "yaml"), ("kt", "kotlin"), ("build.gradle.kts", "kotlin"), ("md", "markdown"),
        ("README.md", "markdown"), ("rs", "rust"), ("crates/pocket/src/main.rs", "rust"), ("py", "python"), ("Swift", "swift"),
        ("Package.swift", "swift"), ("Cargo.toml", "toml"), ("Cargo.lock", "toml"), ("package.json", "json"), ("go", "go"),
        ("main.go", "go"), ("index.html", "html"), ("Main.java", "java"), ("style.css", "css"), ("Dockerfile", "dockerfile"),
        ("Dockerfile.dev", "dockerfile"), ("Makefile", "makefile"), ("rules.mk", "makefile"), ("mermaid", "text"), ("notes.txt", "text"),
        ("", "text"),
    ])
    func language(input: String, expected: String) {
        #expect(Highlighter.language(for: input) == expected)
    }

    @Test("an unknown language renders unhighlighted")
    func unknown() {
        #expect(Highlighter.spans("let a = 1", language: "brainfuck").isEmpty)
        #expect(Highlighter.spans("", language: "rust").isEmpty)
    }

    @Test("the palette is Xcode's default light and dark colours")
    func palette() {
        let light: [TokenKind: UInt32] = [.keyword: 0x9b2393, .string: 0xc41a16, .comment: 0x5d6c79, .number: 0x1c00cf, .type: 0x3900a0, .title: 0x0b4f79, .function: 0x0f68a0, .property: 0x326d74, .preproc: 0x643820, .attribute: 0x815f03, .url: 0x0e0eff]
        let dark: [TokenKind: UInt32] = [.keyword: 0xfc5fa3, .string: 0xfc6a5d, .comment: 0x6c7986, .number: 0xd0bf69, .type: 0xd0a8ff, .title: 0x5dd8ff, .function: 0x41a1c0, .property: 0x67b7a4, .preproc: 0xfd8f3f, .attribute: 0xbf8555, .url: 0x5482ff]
        for kind in TokenKind.allCases where kind != .plain {
            #expect(SyntaxPalette.rgb(kind, dark: false) == light[kind])
            #expect(SyntaxPalette.rgb(kind, dark: true) == dark[kind])
        }
        #expect(SyntaxPalette.rgb(.plain, dark: false) == nil)
    }
}

@Suite struct Rust {
    static let code = """
        #[derive(Debug, Clone)]
        pub struct Point<'a> { x: i32, name: &'a str }
        /* outer /* nested */ still comment */
        fn main() {
            let s = r#"raw "quoted" string"#;
            let c = 'x'; let b = b'\\n'; let t = "multi
        line";
            println!("{} {}", s, c);
            let n = 0xFF_u8 + 1_000 + 2.5e-3;
            // see https://doc.rust-lang.org/std.
            self.items.len();
            MAX_SIZE
        }
        """

    @Test("rust reads attributes, lifetimes, raw strings, macros and nested comments")
    func rust() {
        expectTokens(Self.code, "rs", [
            Token("#[derive(Debug, Clone)]", .attribute), Token("pub", .keyword), Token("struct", .keyword), Token("Point", .type),
            Token("i32", .type), Token("str", .type), Token("/* outer /* nested */ still comment */", .comment), Token("fn", .keyword),
            Token("main", .function), Token("let", .keyword), Token("r#\"raw \"quoted\" string\"#", .string), Token("'x'", .string),
            Token("b'\\n'", .string), Token("\"multi\nline\"", .string), Token("println!", .function), Token("\"{} {}\"", .string),
            Token("0xFF_u8", .number), Token("1_000", .number), Token("2.5e-3", .number), Token("// see ", .comment),
            Token("https://doc.rust-lang.org/std", .url), Token(".", .comment), Token("self", .keyword), Token("items", .property),
            Token("len", .function), Token("MAX_SIZE", .property),
        ])
        expectPlain(Self.code, "rust", "'a")
    }

    @Test("a raw identifier and a range aren't strings or properties")
    func rawIdent() {
        expectPlain("let r#type = 1;", "rust", "type")
        expectPlain("for i in 0..len {}", "rust", "len")
    }
}

@Suite struct Swift {
    @Test("swift reads attributes, directives, interpolation and multi-line and raw strings")
    func swift() {
        let code = """
            @MainActor
            final class Model: ObservableObject {
                #if os(iOS)
                let title = "Hi \\(name.count + 1) there"
                #endif
                let raw = #"a "b" \\(c)"#
                let block = \"\"\"
                    line \\(x)
                    \"\"\"
                /* a /* b */ c */
                func load(_ id: Int) async throws -> [String] { try await fetch(id) }
            }
            """
        expectTokens(code, "swift", [
            Token("@MainActor", .attribute), Token("final", .keyword), Token("class", .keyword), Token("Model", .type),
            Token("ObservableObject", .type), Token("#if", .preproc), Token("os", .function), Token("let", .keyword),
            Token("\"Hi ", .string), Token("count", .property), Token("1", .number), Token(" there\"", .string), Token("#endif", .preproc),
            Token("#\"a \"b\" \\(c)\"#", .string), Token("\"\"\"\n        line ", .string), Token("\n        \"\"\"", .string),
            Token("/* a /* b */ c */", .comment), Token("func", .keyword), Token("load", .function), Token("Int", .type),
            Token("async", .keyword), Token("throws", .keyword), Token("String", .type), Token("try", .keyword), Token("await", .keyword),
            Token("fetch", .function),
        ])
    }

    @Test("swift keeps SCREAMING names as types, not constants")
    func noConstants() {
        expectTokens("let u = URL.init", "swift", [Token("URL", .type)])
    }
}

@Suite struct Go {
    @Test("go reads raw strings across lines, runes and builtin types")
    func go() {
        let code = """
            package main

            import "fmt"

            type Server struct{ Addr string }

            func (s *Server) Start(port int) error {
                q := `select *
            from t`
                r := 'x'
                fmt.Println(q, r, 42, nil)
                return http.ErrServerClosed
            }
            """
        expectTokens(code, "go", [
            Token("package", .keyword), Token("import", .keyword), Token("\"fmt\"", .string), Token("type", .keyword), Token("Server", .type),
            Token("struct", .keyword), Token("string", .type), Token("func", .keyword), Token("Start", .function), Token("int", .type),
            Token("error", .type), Token("`select *\nfrom t`", .string), Token("'x'", .string), Token("Println", .function),
            Token("42", .number), Token("nil", .keyword), Token("return", .keyword), Token("ErrServerClosed", .type),
        ])
    }
}

@Suite struct JavaScript {
    @Test("javascript reads template literals with nested code, regexes and object keys")
    func templates() {
        let code = """
            const re = /ab+c\\/[a-z]/gi;
            const total = a / b / c;
            const msg = `Hello ${user.name + `!${n}`} and
            more`;
            export default { port: 8080, host: "x" };
            async function load() { return fetch(url) }
            """
        expectTokens(code, "javascript", [
            Token("const", .keyword), Token("/ab+c\\/[a-z]/gi", .string), Token("const", .keyword), Token("`Hello ", .string),
            Token("name", .property), Token("`!", .string), Token("`", .string), Token(" and\nmore`", .string), Token("export", .keyword),
            Token("default", .keyword), Token("port", .property), Token("8080", .number), Token("host", .property), Token("\"x\"", .string),
            Token("async", .keyword), Token("function", .keyword), Token("load", .function), Token("fetch", .function),
        ])
        #expect(!tokens(code, "js").contains { $0.text.hasPrefix("/ b") })
    }

    @Test("jsx reads tags, components, attributes and embedded code")
    func jsx() {
        let code = """
            const App = () => (
              <div className="app" onClick={() => go(1)}>
                <Header title={`t`} />
                text {count}
              </div>
            );
            """
        expectTokens(code, "jsx", [
            Token("div", .keyword), Token("className", .attribute), Token("\"app\"", .string), Token("onClick", .attribute),
            Token("go", .function), Token("1", .number), Token("Header", .type), Token("title", .attribute), Token("`t`", .string),
            Token("div", .keyword),
        ])
        expectPlain(code, "jsx", "text")
    }
}

@Suite struct TypeScript {
    @Test("typescript reads types, interfaces, decorators and generics")
    func typescript() {
        let code = """
            @Component({ selector: "app" })
            export interface User<T> { id: number; tags: string[] }
            type Id = string | number;
            const xs = new Map<string, User<T>>();
            function f<T>(a: T): Promise<void> { return a < b ? x : y }
            """
        expectTokens(code, "ts", [
            Token("@Component", .attribute), Token("selector", .property), Token("\"app\"", .string), Token("export", .keyword),
            Token("interface", .keyword), Token("User", .type), Token("T", .type), Token("number", .type), Token("string", .type),
            Token("type", .keyword), Token("Id", .type), Token("new", .keyword), Token("Map", .type), Token("function", .keyword),
            Token("f", .function), Token("Promise", .type), Token("void", .keyword),
        ])
    }

    @Test("a plain typescript file reads `<` as a comparison, not a tag")
    func noJSX() {
        expectPlain("if (a <b) { c }", "typescript", "b")
    }

    @Test("tsx tells type parameters from elements")
    func tsx() {
        let code = "const f = <T,>(x: T) => <Box value={x} />;\nconst g = <T extends object>(y: T) => y;"
        expectTokens(code, "tsx", [Token("T", .type), Token("Box", .type), Token("value", .attribute), Token("T", .type), Token("extends", .keyword)])
    }
}

@Suite struct Python {
    @Test("python reads decorators, f-strings, triple quotes and prefixes")
    func python() {
        let code = """
            @app.route("/")
            def index(self, n: int = 3) -> str:
                \"\"\"Docs
                over lines\"\"\"
                path = rb'C:\\x'
                x = f"{n + 1} items {{literal}}"
                return None if n @ m else print(x)  # done
            class Model(Base):
                pass
            """
        expectTokens(code, "python", [
            Token("@app.route", .attribute), Token("\"/\"", .string), Token("def", .keyword), Token("index", .function),
            Token("self", .keyword), Token("int", .type), Token("3", .number), Token("str", .type), Token("\"\"\"Docs\n    over lines\"\"\"", .string),
            Token("rb'C:\\x'", .string), Token("f\"", .string), Token("1", .number), Token(" items {{literal}}\"", .string),
            Token("return", .keyword), Token("None", .keyword), Token("print", .function), Token("# done", .comment), Token("class", .keyword),
            Token("Model", .type), Token("Base", .type), Token("pass", .keyword),
        ])
        expectPlain(code, "python", "@ m")
    }
}

@Suite struct JVM {
    @Test("java reads annotations, text blocks and primitive types")
    func java() {
        let code = """
            @Override
            public static void main(String[] args) {
                String q = \"\"\"
                    SELECT 1
                    \"\"\";
                char c = 'a'; long n = 10L;
                System.out.println(q);
            }
            """
        expectTokens(code, "java", [
            Token("@Override", .attribute), Token("public", .keyword), Token("static", .keyword), Token("void", .type), Token("main", .function),
            Token("String", .type), Token("\"\"\"\n        SELECT 1\n        \"\"\"", .string), Token("char", .type), Token("'a'", .string),
            Token("long", .type), Token("10L", .number), Token("System", .type), Token("out", .property), Token("println", .function),
        ])
    }

    @Test("kotlin reads string templates, raw strings and nested comments")
    func kotlin() {
        let code = """
            @JvmStatic
            fun greet(name: String): String = "Hi $name, ${name.length} chars"
            val raw = \"\"\"
                path: C:\\dir $home
            \"\"\"
            /* a /* b */ c */
            data class P(val x: Int)
            """
        expectTokens(code, "kotlin", [
            Token("@JvmStatic", .attribute), Token("fun", .keyword), Token("greet", .function), Token("String", .type),
            Token("\"Hi ", .string), Token(", ", .string), Token("length", .property), Token(" chars\"", .string), Token("val", .keyword),
            Token("\"\"\"\n    path: C:\\dir ", .string), Token("\n\"\"\"", .string), Token("/* a /* b */ c */", .comment),
            Token("data", .keyword), Token("class", .keyword), Token("P", .type), Token("Int", .type),
        ])
    }
}

@Suite struct Shell {
    @Test("bash reads commands, variables, strings, substitutions and comments")
    func bash() {
        let code = """
            #!/usr/bin/env bash
            set -euo pipefail
            NAME="world" ./run.sh --flag "$NAME ${HOME}/x" 'lit $x' $(date +%s) # trailing
            if [ -f "$f" ]; then echo ok | grep -q o; fi
            for x in a b; do printf '%s' "$x"; done
            exit 1
            """
        expectTokens(code, "sh", [
            Token("#!/usr/bin/env bash", .comment), Token("set", .function), Token("NAME", .property), Token("\"world\"", .string),
            Token("./run.sh", .function), Token("\"", .string), Token("$NAME", .property), Token(" ", .string), Token("${HOME}", .property),
            Token("/x\"", .string), Token("'lit $x'", .string), Token("date", .function), Token("# trailing", .comment), Token("if", .keyword),
            Token("$f", .property), Token("then", .keyword), Token("echo", .function), Token("grep", .function),
            Token("fi", .keyword), Token("for", .keyword), Token("in", .keyword), Token("do", .keyword), Token("printf", .function),
            Token("'%s'", .string), Token("done", .keyword), Token("exit", .keyword), Token("1", .number),
        ])
        expectPlain(code, "bash", "pipefail")
    }

    @Test("a heredoc's body is a string through its delimiter line")
    func heredoc() {
        let code = "cat <<-'EOF' > out.txt\n\techo $NOT_A_VAR\n\tEOF\necho done"
        expectTokens(code, "bash", [Token("cat", .function), Token("'EOF'", .string), Token("\techo $NOT_A_VAR\n\tEOF", .string), Token("echo", .function)])
    }

    @Test("a console session reads its commands after the prompt")
    func console() {
        expectTokens("$ npm install\n$ make app", "console", [Token("npm", .function), Token("make", .function)])
    }

    @Test("a dockerfile reads instructions and shell commands")
    func dockerfile() {
        let code = "FROM node:22 AS build\n# deps\nRUN npm ci && npm run build\nENV PORT=3000\nCMD [\"node\", \"server.js\"]"
        expectTokens(code, "Dockerfile", [
            Token("FROM", .keyword), Token("AS", .keyword), Token("# deps", .comment), Token("RUN", .keyword), Token("npm", .function),
            Token("npm", .function), Token("ENV", .keyword), Token("CMD", .keyword), Token("\"node\"", .string),
        ])
    }

    @Test("a makefile reads targets, variables and recipes")
    func makefile() {
        let code = ".PHONY: app test\nCC ?= clang\nAPP := pocket\n\napp: build\n\t@cargo build --release -p $(APP)\n# done"
        expectTokens(code, "Makefile", [
            Token(".PHONY", .keyword), Token("CC", .property), Token("APP", .property), Token("app", .function), Token("cargo", .function),
            Token("$(APP)", .property), Token("# done", .comment),
        ])
    }
}

@Suite struct Markup {
    @Test("html reads tags, attributes, comments and embedded script and style")
    func html() {
        let code = """
            <!DOCTYPE html>
            <!-- a
            comment -->
            <div class="a" hidden>Text &amp; more</div>
            <script>const x = 1;</script>
            <style>.a { color: red; }</style>
            """
        expectTokens(code, "html", [
            Token("<!DOCTYPE html>", .keyword), Token("<!-- a\ncomment -->", .comment), Token("div", .keyword), Token("class", .attribute),
            Token("\"a\"", .string), Token("hidden", .attribute), Token("div", .keyword), Token("script", .keyword), Token("const", .keyword),
            Token("1", .number), Token("script", .keyword), Token("style", .keyword), Token(".a", .property), Token("color", .property),
            Token("style", .keyword),
        ])
        expectPlain(code, "html", "Text")
    }

    @Test("markdown reads headings, code, links and fenced code in its own language")
    func markdown() {
        let code = """
            # Title
            Some `code` and a [link](https://x.dev) <https://y.dev>.

            - item
            1. first
            > quote

            ```rust
            fn main() {}
            ```
            """
        expectTokens(code, "md", [
            Token("# Title", .title), Token("`code`", .string), Token("https://x.dev", .url), Token("https://y.dev", .url),
            Token("-", .keyword), Token("1.", .keyword), Token(">", .keyword), Token("```rust", .string), Token("fn", .keyword),
            Token("main", .function), Token("```", .string),
        ])
    }

    @Test("markdown's unclosed fence highlights to the end")
    func unclosed() {
        expectTokens("```python\ndef f(): pass", "markdown", [Token("```python", .string), Token("def", .keyword), Token("f", .function)])
    }
}

@Suite struct Data {
    @Test("json tells keys from strings")
    func json() {
        expectTokens(#"{"name": "x", "n": -1.5e3, "ok": true, "none": null, "list": ["a"]} // c"#, "json", [
            Token("\"name\"", .property), Token("\"x\"", .string), Token("\"n\"", .property), Token("-1.5e3", .number), Token("\"ok\"", .property),
            Token("true", .keyword), Token("null", .keyword), Token("\"a\"", .string), Token("// c", .comment),
        ])
    }

    @Test("yaml reads keys, scalars, anchors, block scalars and comments")
    func yaml() {
        let code = """
            # config
            name: anywhere
            on:
              push:
                branches: [main, "release/*"]
            steps:
              - name: Build
                run: |
                  make app
                  make test
                retries: 3
                enabled: true # yes
              - &anchor key: *ref
            """
        expectTokens(code, "yaml", [
            Token("# config", .comment), Token("name", .property), Token("anywhere", .string), Token("on", .property), Token("push", .property),
            Token("branches", .property), Token("main", .string), Token("\"release/*\"", .string), Token("steps", .property), Token("name", .property),
            Token("Build", .string), Token("run", .property), Token("\n      make app\n      make test", .string), Token("retries", .property),
            Token("3", .number), Token("enabled", .property), Token("true", .keyword), Token("# yes", .comment),
        ])
    }

    @Test("toml reads tables, keys, strings, multi-line strings and values")
    func toml() {
        let code = """
            [package]
            name = "pocket" # app
            version = "0.1.13"
            [[bin]]
            path = 'src/main.rs'
            doc = \"\"\"
            many
            lines\"\"\"
            debug = true
            opt-level = 2
            deps = { serde = "1", tokio.workspace = true }
            """
        expectTokens(code, "toml", [
            Token("package", .property), Token("name", .property), Token("\"pocket\"", .string), Token("# app", .comment), Token("version", .property),
            Token("bin", .property), Token("path", .property), Token("'src/main.rs'", .string), Token("doc", .property),
            Token("\"\"\"\nmany\nlines\"\"\"", .string), Token("debug", .property), Token("true", .keyword), Token("opt-level", .property), Token("2", .number),
            Token("serde", .property), Token("\"1\"", .string), Token("tokio.workspace", .property), Token("true", .keyword),
        ])
    }

    @Test("css reads selectors, properties, values and at-rules")
    func css() {
        let code = """
            /* theme
               colours */
            @media (max-width: 600px) {
              .card > a:hover, #main { color: #fff; margin: 0 -2px 1.5em; background: url(img.png) !important; }
              &:focus { outline: none }
            }
            """
        expectTokens(code, "css", [
            Token("/* theme\n   colours */", .comment), Token("@media", .keyword), Token("max-width", .property), Token("600px", .number),
            Token(".card", .property), Token("a", .keyword), Token(":hover", .attribute), Token("#main", .property), Token("color", .property),
            Token("#fff", .number), Token("margin", .property), Token("0", .number), Token("-2px", .number), Token("1.5em", .number),
            Token("background", .property), Token("url", .function), Token("img.png", .string), Token("!important", .keyword), Token(":focus", .attribute),
            Token("outline", .property),
        ])
    }
}

@Suite struct Invariants {
    static let samples: [(String, String)] = [
        ("rust", Rust.code), ("swift", "let a = \"é😀\\(b)\" // 👍 https://x.dev/é\nlet 变量 = 1"), ("javascript", "const s = `${a`${b}`}`; /x/.test('😀')"),
        ("python", "s = f'{a!r:>{w}}' # 中文"), ("bash", "echo \"$(ls \"$d\")\" <<EOF\nbody 😀\nEOF\n"), ("html", "<p title='é'>😀<script>let x='é'</script>"),
        ("markdown", "# 😀\n```js\nlet é = `x`\n```\n"), ("yaml", "k: 'é'\n- [a, {b: c}]"), ("toml", "a = \"é\"\n[t]"), ("json", "{\"é\": [1, \"😀\"]}"),
        ("css", "a::after { content: \"😀\" }"), ("go", "s := `é`"), ("java", "char c = '😀';"), ("kotlin", "\"$é ${x}\""), ("tsx", "<A b={<C/>}>é</A>"),
        ("typescript", "let x: Map<string, T> = new Map()"), ("dockerfile", "RUN echo é"), ("makefile", "a:\n\techo $(B)"),
    ]

    @Test("spans are sorted, apart, inside the text and never split a character, whatever is cut off")
    func invariants() {
        for (language, sample) in Self.samples {
            var cut = sample.unicodeScalars.startIndex
            while true {
                let code = String(sample.unicodeScalars[..<cut])
                let spans = Highlighter.spans(code, language: language)
                let units = Array(code.utf16)
                var last = 0
                for span in spans {
                    #expect(span.range.lowerBound >= last && span.range.lowerBound < span.range.upperBound && span.range.upperBound <= units.count, "\(language) \(code.debugDescription) \(span)")
                    #expect(!UTF16.isTrailSurrogate(units[span.range.lowerBound]), "\(language) splits a character at \(span)")
                    if span.range.upperBound < units.count { #expect(!UTF16.isTrailSurrogate(units[span.range.upperBound]), "\(language) splits a character at \(span)") }
                    last = span.range.upperBound
                }
                if cut == sample.unicodeScalars.endIndex { break }
                cut = sample.unicodeScalars.index(after: cut)
            }
        }
    }

    @Test("non-ASCII text maps spans to UTF-16 offsets")
    func utf16() {
        #expect(tokens("let é = \"😀\"; // x", "rust") == [Token("let", .keyword), Token("\"😀\"", .string), Token("// x", .comment)])
    }
}
