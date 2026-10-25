import Foundation

/// A token's class, mapped to the desktop's Xcode palette.
public enum TokenKind: UInt8, Sendable, CaseIterable {
    case plain, keyword, string, comment, number, type, title, function, property, preproc, attribute, url
}

/// A coloured run; `range` is in UTF-16 offsets of the highlighted text.
public struct HighlightSpan: Sendable, Equatable {
    public var range: Range<Int>
    public var kind: TokenKind
    public init(range: Range<Int>, kind: TokenKind) {
        self.range = range
        self.kind = kind
    }
}

public enum Highlighter {
    /// Spans of `code` in `language` (a protocol.md language id or a fence label), sorted, non-overlapping.
    /// Pure and thread-safe; call it off the main actor.
    public static func spans(_ code: String, language: String) -> [HighlightSpan] {
        let lang = Language(id: Self.language(for: language))
        guard lang != .text, !code.isEmpty else { return [] }
        var code = code
        return code.withUTF8 { bytes in
            var lexer = Lexer(bytes)
            lexer.lex(lang)
            return utf16Spans(lexer.out, bytes)
        }
    }

    /// The language id for a file path or fence label ("sh" → "bash"), "text" when unknown.
    public static func language(for pathOrLabel: String) -> String {
        let name = (pathOrLabel.split(separator: "/").last.map(String.init) ?? pathOrLabel).lowercased()
        if let lang = names[name] ?? labels[name] { return lang }
        if name.hasPrefix("dockerfile") || name.hasSuffix(".dockerfile") || name.hasPrefix("containerfile") { return "dockerfile" }
        if let dot = name.lastIndex(of: "."), let lang = labels[String(name[name.index(after: dot)...])] { return lang }
        return "text"
    }

    /// Whole file names whose language their extension doesn't tell.
    static let names: [String: String] = [
        "makefile": "makefile", "gnumakefile": "makefile", "justfile": "makefile",
        ".bashrc": "bash", ".bash_profile": "bash", ".zshrc": "bash", ".zprofile": "bash", ".profile": "bash", ".envrc": "bash",
        ".env": "bash", "pkgbuild": "bash", "cargo.lock": "toml", "pipfile": "toml", ".babelrc": "json", ".eslintrc": "json",
        ".prettierrc": "json", "package.json": "json", "tsconfig.json": "json", "podfile": "text", "gemfile": "text",
    ]

    /// Fence labels and file extensions.
    static let labels: [String: String] = [
        "bash": "bash", "sh": "bash", "shell": "bash", "zsh": "bash", "console": "bash", "shellsession": "bash", "sh-session": "bash",
        "terminal": "bash", "ksh": "bash", "command": "bash", "fish": "bash",
        "css": "css", "scss": "css", "less": "css", "pcss": "css", "postcss": "css",
        "go": "go", "golang": "go",
        "html": "html", "htm": "html", "xhtml": "html", "xml": "html", "svg": "html", "plist": "html", "vue": "html", "svelte": "html",
        "java": "java",
        "javascript": "javascript", "js": "javascript", "mjs": "javascript", "cjs": "javascript", "jsx": "javascript", "node": "javascript",
        "kotlin": "kotlin", "kt": "kotlin", "kts": "kotlin",
        "markdown": "markdown", "md": "markdown", "mdx": "markdown", "mkd": "markdown",
        "python": "python", "py": "python", "py3": "python", "python3": "python", "pyi": "python", "pyw": "python", "gyp": "python",
        "rust": "rust", "rs": "rust",
        "swift": "swift",
        "toml": "toml",
        "tsx": "tsx",
        "typescript": "typescript", "ts": "typescript", "mts": "typescript", "cts": "typescript",
        "yaml": "yaml", "yml": "yaml",
        "json": "json", "jsonc": "json", "json5": "json", "geojson": "json", "jsonl": "json", "webmanifest": "json",
        "dockerfile": "dockerfile", "docker": "dockerfile", "containerfile": "dockerfile",
        "makefile": "makefile", "make": "makefile", "mk": "makefile", "mak": "makefile",
    ]
}

/// The languages with a lexer.
enum Language: Equatable {
    case bash, dockerfile, makefile, css, go, html, java, javascript, kotlin, markdown, python, rust, swift, toml, tsx, typescript, yaml, json, text

    init(id: String) {
        switch id {
        case "bash": self = .bash
        case "dockerfile": self = .dockerfile
        case "makefile": self = .makefile
        case "css": self = .css
        case "go": self = .go
        case "html": self = .html
        case "java": self = .java
        case "javascript": self = .javascript
        case "kotlin": self = .kotlin
        case "markdown": self = .markdown
        case "python": self = .python
        case "rust": self = .rust
        case "swift": self = .swift
        case "toml": self = .toml
        case "tsx": self = .tsx
        case "typescript": self = .typescript
        case "yaml": self = .yaml
        case "json": self = .json
        default: self = .text
        }
    }
}

extension Lexer {
    mutating func lex(_ language: Language) {
        switch language {
        case .bash: shell(.bash)
        case .dockerfile: shell(.dockerfile)
        case .makefile: shell(.makefile)
        case .css: css()
        case .html: html()
        case .markdown: markdown()
        case .toml: toml()
        case .yaml: yaml()
        case .json: json()
        case .go: clike(Specs.go)
        case .java: clike(Specs.java)
        case .javascript: clike(Specs.javascript)
        case .kotlin: clike(Specs.kotlin)
        case .python: clike(Specs.python)
        case .rust: clike(Specs.rust)
        case .swift: clike(Specs.swift)
        case .tsx: clike(Specs.tsx)
        case .typescript: clike(Specs.typescript)
        case .text: break
        }
    }
}

enum Specs {
    static let rust = CSpec(
        keywords: Words("as async await break const continue crate dyn else enum extern false fn for if impl in let loop match mod move mut pub ref return self Self static struct super trait true type unsafe use where while union macro_rules yield"),
        types: Words("i8 i16 i32 i64 i128 isize u8 u16 u32 u64 u128 usize f32 f64 bool char str"),
        functionDecl: Words("fn"), typeDecl: Words("struct enum trait type union"),
        strings: .rust, nestedComments: true, hashAttributes: true, macros: true)

    static let swift = CSpec(
        keywords: Words("associatedtype class deinit enum extension fileprivate func import init inout internal let open operator private precedencegroup protocol public rethrows static struct subscript typealias var break case catch continue default defer do else fallthrough for guard if in repeat return throw throws switch where while as false is nil self Self super true try async await actor some any nonisolated isolated consuming borrowing sending mutating nonmutating override required convenience final lazy weak unowned indirect willSet didSet"),
        types: Words(""),
        functionDecl: Words("func"), typeDecl: Words("class struct enum protocol extension actor typealias associatedtype"),
        strings: .swift, nestedComments: true, annotations: true, hashDirectives: true, screamingConstants: false)

    static let go = CSpec(
        keywords: Words("break case chan const continue default defer else fallthrough for func go goto if import interface map package range return select struct switch type var true false nil iota"),
        types: Words("bool byte complex64 complex128 error float32 float64 int int8 int16 int32 int64 rune string uint uint8 uint16 uint32 uint64 uintptr any comparable"),
        functionDecl: Words("func"), typeDecl: Words("type"),
        strings: .go, screamingConstants: false)

    static let java = CSpec(
        keywords: Words("abstract assert break case catch class const continue default do else enum extends final finally for goto if implements import instanceof interface native new package private protected public return static strictfp super switch synchronized this throw throws transient try volatile while var record sealed permits yield true false null"),
        types: Words("boolean byte char double float int long short void"),
        functionDecl: Words(""), typeDecl: Words("class interface enum record"),
        strings: .java, annotations: true)

    static let kotlin = CSpec(
        keywords: Words("as break class continue do else false for fun if in interface is null object package return super this throw true try typealias typeof val var when while by catch constructor finally import init where actual abstract annotation companion const crossinline data enum expect external final infix inline inner internal lateinit noinline open operator out override private protected public reified sealed suspend tailrec vararg"),
        types: Words(""),
        functionDecl: Words("fun"), typeDecl: Words("class interface object typealias"),
        strings: .kotlin, nestedComments: true, annotations: true)

    static let python = CSpec(
        keywords: Words("False None True and as assert async await break class continue def del elif else except finally for from global if import in is lambda nonlocal not or pass raise return try while with yield match case self cls"),
        types: Words("int float str bool bytes list dict set tuple object type complex frozenset bytearray"),
        functionDecl: Words("def"), typeDecl: Words("class"),
        strings: .python, slashComments: false, hashComments: true, annotations: true)

    static let jsKeywords = "async await break case catch class const continue debugger default delete do else export extends finally for from function get if import in instanceof let new of return set static super switch this throw try typeof var void while with yield true false null undefined as"
    static let tsKeywords = jsKeywords + " abstract declare enum implements interface keyof namespace private protected public readonly type infer is satisfies override asserts module unique accessor"

    static let javascript = CSpec(
        keywords: Words(jsKeywords),
        functionDecl: Words("function"), typeDecl: Words("class"),
        strings: .js, regex: true, jsx: true, objectKeys: true)

    static let typescript = CSpec(
        keywords: Words(tsKeywords),
        types: Words("string number boolean any unknown never void object bigint symbol"),
        functionDecl: Words("function"), typeDecl: Words("class interface type enum namespace"),
        strings: .js, annotations: true, regex: true, objectKeys: true)

    static let tsx: CSpec = {
        var spec = typescript
        spec.jsx = true
        return spec
    }()
}

/// sRGB colour as 0xRRGGBB: Xcode's default syntax colours, as the desktop's code blocks use whatever the chosen theme
/// (`packages/desktop/crates/theme/src/theme.rs markdown_highlight_theme`). nil means the text's own colour.
public enum SyntaxPalette {
    public static func rgb(_ kind: TokenKind, dark: Bool) -> UInt32? {
        let pair: (UInt32, UInt32)
        switch kind {
        case .plain: return nil
        case .keyword: pair = (0x9b2393, 0xfc5fa3)
        case .string: pair = (0xc41a16, 0xfc6a5d)
        case .comment: pair = (0x5d6c79, 0x6c7986)
        case .number: pair = (0x1c00cf, 0xd0bf69)
        case .type: pair = (0x3900a0, 0xd0a8ff)
        case .title: pair = (0x0b4f79, 0x5dd8ff)
        case .function: pair = (0x0f68a0, 0x41a1c0)
        case .property: pair = (0x326d74, 0x67b7a4)
        case .preproc: pair = (0x643820, 0xfd8f3f)
        case .attribute: pair = (0x815f03, 0xbf8555)
        case .url: pair = (0x0e0eff, 0x5482ff)
        }
        return dark ? pair.1 : pair.0
    }
}
