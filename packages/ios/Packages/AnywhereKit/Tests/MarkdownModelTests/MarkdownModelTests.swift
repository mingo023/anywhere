import Testing
@testable import MarkdownModel

func blocks(_ markdown: String) -> [MDBlock] { Markdown.parse(markdown).blocks.map(\.block) }

func paragraph(_ markdown: String) -> [MDInline] {
    guard case .paragraph(let inlines)? = blocks(markdown).first else {
        Issue.record("expected a paragraph from \(markdown.debugDescription), got \(blocks(markdown))")
        return []
    }
    return inlines
}

func code(_ block: MDBlock?) -> MDCode? {
    if case .code(let code)? = block { return code }
    return nil
}

func list(_ block: MDBlock?) -> MDList? {
    if case .list(let list)? = block { return list }
    return nil
}

func table(_ block: MDBlock?) -> MDTable? {
    if case .table(let table)? = block { return table }
    return nil
}

func alert(_ block: MDBlock?) -> MDAlert? {
    if case .alert(let alert)? = block { return alert }
    return nil
}

/// Ported from the desktop's `vendor/gpui-base/src/text/format/markdown.rs` tests.
@Suite struct DesktopFormat {
    @Test("nested emphasis merges text marks")
    func nestedEmphasis() {
        #expect(paragraph("This has **_bold and italic_** text.") == [.text("This has "), .strong([.emphasis([.text("bold and italic")])]), .text(" text.")])
    }

    @Test("an inline html image stays in the markdown paragraph")
    func inlineImage() {
        let inlines = paragraph(#"Before <img src="https://example.com/avatar.png" alt="Avatar" width="32" height="32" /> after."#)
        #expect(inlines == [.text("Before "), .image(MDImage(source: "https://example.com/avatar.png", alt: "Avatar", width: 32, height: 32)), .text(" after.")])
    }

    @Test("an inline html image without a size stays in the markdown paragraph")
    func inlineImageWithoutSize() {
        let inlines = paragraph(#"Before <img src="https://avatars.githubusercontent.com/u/5518"> after."#)
        #expect(inlines == [.text("Before "), .image(MDImage(source: "https://avatars.githubusercontent.com/u/5518", alt: "")), .text(" after.")])
    }

    @Test("a soft break reflows to a space whatever the line ending", arguments: [
        "this sentence\ncontinues as a soft wrap", "this sentence\r\ncontinues as a soft wrap", "this sentence\rcontinues as a soft wrap",
    ])
    func softBreak(source: String) {
        #expect(paragraph(source) == [.text("this sentence"), .softBreak, .text("continues as a soft wrap")])
        #expect(paragraph(source).text == "this sentence continues as a soft wrap")
    }

    @Test("a soft break reflows while a hard break survives")
    func softAndHard() {
        #expect(paragraph("a\nb  \nc") == [.text("a"), .softBreak, .text("b"), .hardBreak, .text("c")])
        #expect(paragraph("a\nb  \nc").text == "a b\nc")
    }

    @Test("a hard break renders a newline", arguments: ["Owner: Jane  \nPersona: assistant", "Owner: Jane\\\nPersona: assistant"])
    func hardBreak(source: String) {
        #expect(paragraph(source) == [.text("Owner: Jane"), .hardBreak, .text("Persona: assistant")])
    }

    @Test("yaml frontmatter is off: a rule, then a setext heading")
    func noFrontmatter() {
        #expect(blocks("---\nSome text\n---") == [.rule, .heading(level: 2, [.text("Some text")])])
    }

    @Test("fenced yaml remains a code block")
    func fencedYaml() {
        #expect(code(blocks("```yaml\nname: example\n```").first) == MDCode(language: "yaml", code: "name: example", fenced: true))
    }

    @Test("inline math stays literal, and code and escapes keep their dollars")
    func inlineMath() {
        let inlines = paragraph(#"$x$ `$code$` \$escaped\$"#)
        #expect(inlines.text == "$x$ $code$ $escaped$")
        #expect(inlines.contains(.code("$code$")))
    }

    @Test("block math parses as a block and falls back to a code block")
    func blockMath() {
        let all = blocks("$$\n\\sum_{i=1}^{n} i\n$$")
        #expect(all.count == 1)
        #expect(code(all.first) == MDCode(language: nil, code: "\\sum_{i=1}^{n} i", fenced: false))
        #expect(code(all.first)?.label == "text")
    }

    @Test("unclaimed inline math keeps its literal source")
    func dollars() {
        #expect(paragraph("spent $5 and $10 today") == [.text("spent $5 and $10 today")])
    }

    @Test("an alert without a title is titled by its kind")
    func alertKindTitle() {
        let a = alert(blocks("> [!warning]\n> Mind the gap.").first)
        #expect(a == MDAlert(kind: .warning, title: [.text("Warning")], body: [.paragraph([.text("Mind the gap.")])]))
    }

    @Test("an alert takes its title from the rest of the tag line")
    func alertTitle() {
        let a = alert(blocks("> [!TIP] Shortcut\n> Press space.").first)
        #expect(a == MDAlert(kind: .tip, title: [.text("Shortcut")], body: [.paragraph([.text("Press space.")])]))
    }

    @Test("a blockquote with an unknown tag stays a quote")
    func unknownAlert() {
        #expect(blocks("> [!DANGER]\n> Careful.") == [.quote([.paragraph([.text("[!DANGER]"), .softBreak, .text("Careful.")])])])
    }

    @Test("footnotes are numbered in reference order and unreferenced ones dropped")
    func footnotes() {
        let all = blocks("One[^b] two[^a] again[^b].\n\n[^a]: Alpha.\n[^b]: Beta.\n[^c]: Unused.")
        #expect(all.first?.text == "One¹ two² again¹.")
        guard case .footnotes(let notes)? = all.last else {
            Issue.record("expected footnotes last, got \(all)")
            return
        }
        #expect(notes == [
            MDFootnote(number: 1, blocks: [.paragraph([.text("Beta.")])], references: 2),
            MDFootnote(number: 2, blocks: [.paragraph([.text("Alpha.")])], references: 1),
        ])
        #expect(all.last?.text == "Beta. ↩ ↩\nAlpha. ↩")
    }

    @Test("a code block without a language is labelled by how it was written")
    func codeLabels() {
        #expect(blocks("```\nfenced\n```\n\n    indented").map { code($0)?.label } == ["code", "text"])
    }

    @Test("a code block is labelled by its language in lowercase")
    func codeLanguage() {
        #expect(code(blocks("```Swift\nlet a = 1\n```").first)?.label == "swift")
    }

    @Test("an ordered list counts from its first number")
    func orderedStart() {
        let l = list(blocks("3. three\n4. four").first)
        #expect(l?.ordered == true)
        #expect(l?.start == 3)
        #expect(l?.items.map(\.blocks.text) == ["three", "four"])
    }
}

/// Ported from the desktop's `pocket/chat/markdown.rs` and `chat/src/rows.rs` tests.
@Suite struct DesktopChat {
    @Test("a fenced block reads its language and code")
    func fence() {
        #expect(MarkdownSplit.fence("```rust\nfn main() {}\n```\n")! == ("rust", "fn main() {}"))
        #expect(MarkdownSplit.fence("~~~\na\nb")! == ("", "a\nb"))
        #expect(MarkdownSplit.fence("Some *text*") == nil)
    }

    @Test("a fence under a list item loses its indent from each line")
    func indentedFence() {
        #expect(MarkdownSplit.fence("   ```sh\n   brew install\n     --cask x\n  y\n   ```")! == ("sh", "brew install\n  --cask x\ny"))
    }

    @Test("inline code naming a file or folder is a path")
    func paths() {
        #expect(pathFolder(in: "crates/pocket/src/desktop.rs") == false)
        #expect(pathFolder(in: "CHANGELOG.md") == false)
        #expect(pathFolder(in: ".anywhere/") == true)
        for code in ["make app", "v*", "type:", "Desktop", "self.items", "0.1/5", "https://x.dev/a"] {
            #expect(pathFolder(in: code) == nil, "\(code)")
        }
    }

    @Test("inline code naming a path parses as a path chip, and other code stays code")
    func pathChips() {
        #expect(paragraph("See `src/a.rs`, `Package.swift`, `docs/` and `make app`.") == [
            .text("See "), .path("src/a.rs", folder: false), .text(", "), .path("Package.swift", folder: false), .text(", "),
            .path("docs/", folder: true), .text(" and "), .code("make app"), .text("."),
        ])
    }

    @Test("a long message becomes one row per markdown block")
    func split() {
        let md = "## Step 1\nIntro line\nmore intro\n\n- one\n- two\n\n- three\n  continued\n\nLast."
        #expect(MarkdownSplit.blocks(md).map { String(md[$0]) } == ["## Step 1", "Intro line\nmore intro", "- one\n- two\n\n- three\n  continued", "Last."])
    }

    @Test("a fenced code block is never split")
    func fenceNeverSplit() {
        let md = "Run:\n```sh\nmake\n\n## not a heading\n```\nAfter\n\n~~~~\n```\n\n~~~~\n\n```unclosed\n\nrest"
        #expect(MarkdownSplit.blocks(md).map { String(md[$0]) } == ["Run:", "```sh\nmake\n\n## not a heading\n```", "After", "~~~~\n```\n\n~~~~", "```unclosed\n\nrest"])
    }

    @Test("inline backticks open no fence")
    func inlineBackticks() {
        #expect(MarkdownSplit.blocks("```inline``` code\n\nnext").count == 2)
    }

    @Test("a message with CRLF line endings splits like one with LF")
    func crlfSplit() {
        let md = "a\r\nb\r\n\r\n- c"
        #expect(MarkdownSplit.blocks(md).map { String(md[$0]) } == ["a\r\nb", "- c"])
    }
}

/// The fixture list of `research-chat.md` §3.7.
@Suite struct EdgeCases {
    @Test("an unclosed fence while streaming is a code block running to the end")
    func unclosedFence() {
        #expect(blocks("Here:\n\n```swift\nlet a = 1\n\nlet b") == [.paragraph([.text("Here:")]), .code(MDCode(language: "swift", code: "let a = 1\n\nlet b", fenced: true))])
        #expect(blocks("```") == [.code(MDCode(language: nil, code: "", fenced: true))])
    }

    @Test("a heading mark inside a fence is code, not a heading")
    func hashesInFence() {
        #expect(blocks("```md\n## not a heading\n```") == [.code(MDCode(language: "md", code: "## not a heading", fenced: true))])
    }

    @Test("a tilde fence holds backtick fences")
    func tildeHoldsBackticks() {
        #expect(blocks("~~~~\n```\ninner\n```\n~~~~") == [.code(MDCode(language: nil, code: "```\ninner\n```", fenced: true))])
    }

    @Test("an inline triple backtick is code, not a fence")
    func inlineTriple() {
        #expect(blocks("```inline``` code") == [.paragraph([.code("inline"), .text(" code")])])
    }

    @Test("a fence indented under a list item is that item's code block")
    func fenceInList() {
        let l = list(blocks("1. Install:\n   ```sh\n   brew install x\n   ```\n2. Run").first)
        #expect(l?.items.count == 2)
        #expect(l?.items.first?.blocks == [.paragraph([.text("Install:")]), .code(MDCode(language: "sh", code: "brew install x", fenced: true))])
    }

    @Test("blank lines between list items keep one loose list")
    func looseList() {
        let all = blocks("- one\n\n- two\n\n- three")
        #expect(all.count == 1)
        #expect(list(all.first)?.tight == false)
        #expect(list(all.first)?.items.count == 3)
        #expect(list(blocks("- a\n- b").first)?.tight == true)
    }

    @Test("a list item carries on through an indented line")
    func continuedItem() {
        #expect(list(blocks("- three\n  continued").first)?.items.first?.blocks.text == "three continued")
    }

    @Test("text with CJK, emoji and right-to-left scripts keeps its characters and marks")
    func unicode() {
        #expect(paragraph("中文**强调**文字") == [.text("中文"), .strong([.text("强调")]), .text("文字")])
        #expect(paragraph("👍🏽 **ok** 🎉 👨‍👩‍👧") == [.text("👍🏽 "), .strong([.text("ok")]), .text(" 🎉 👨‍👩‍👧")])
        #expect(paragraph("مرحبا **بالعالم** שלום") == [.text("مرحبا "), .strong([.text("بالعالم")]), .text(" שלום")])
        #expect(paragraph("日本語の`パス/ファイル.md`") == [.text("日本語の"), .path("パス/ファイル.md", folder: false)])
    }
}

/// GitHub Flavored Markdown as cmark-gfm reads it, through to our blocks.
@Suite struct GFM {
    @Test("a table keeps its alignments, header and cells")
    func tableAlignment() {
        let t = table(blocks("| a | b | c | d |\n|:--|:-:|--:|---|\n| 1 | 2 | 3 | 4 |").first)
        #expect(t == MDTable(alignments: [.left, .center, .right, .none], header: [[.text("a")], [.text("b")], [.text("c")], [.text("d")]], rows: [[[.text("1")], [.text("2")], [.text("3")], [.text("4")]]]))
    }

    @Test("a table cell takes escaped pipes and inline code holding them")
    func tablePipes() {
        let t = table(blocks("| op | meaning |\n|---|---|\n| `a \\| b` | either \\| or |").first)
        #expect(t?.rows.first == [[.code("a | b")], [.text("either | or")]])
    }

    @Test("a row short of cells is padded and one with too many is cut")
    func ragged() {
        let t = table(blocks("| a | b |\n|---|---|\n| 1 |\n| 1 | 2 | 3 |").first)
        #expect(t?.rows == [[[.text("1")], []], [[.text("1")], [.text("2")]]])
    }

    @Test("a table cell holds inline marks, links and paths")
    func richCells() {
        let t = table(blocks("| file | note |\n|---|---|\n| `src/main.rs` | **bold** [link](https://x.dev) |").first)
        #expect(t?.rows.first == [[.path("src/main.rs", folder: false)], [.strong([.text("bold")]), .text(" "), .link(destination: "https://x.dev", title: nil, [.text("link")])]])
    }

    @Test("a header with no body rows is a table of one row")
    func headerOnly() {
        #expect(table(blocks("| a |\n|---|").first)?.rows == [])
    }

    @Test("nested lists mix ordered, bullet and task items")
    func nestedLists() {
        let md = "1. first\n   - bullet\n     - [x] done\n     - [ ] todo\n2. second\n\n- [ ] top task"
        let all = blocks(md)
        #expect(all.count == 2)
        let outer = list(all.first)
        #expect(outer?.ordered == true)
        let inner = list(outer?.items.first?.blocks.last)
        #expect(inner?.ordered == false)
        let tasks = list(inner?.items.first?.blocks.last)
        #expect(tasks?.items.map(\.checked) == [true, false])
        #expect(tasks?.items.map(\.blocks.text) == ["done", "todo"])
        #expect(list(all.last)?.items.first?.checked == false)
        #expect(outer?.items.last?.checked == nil)
    }

    @Test("an ordered list may use parentheses and start at zero")
    func parenList() {
        let l = list(blocks("0) zero\n1) one").first)
        #expect(l?.ordered == true)
        #expect(l?.start == 0)
    }

    @Test("an alert holds nested lists and code", arguments: ["NOTE", "tip", "Important", "WARNING", "caution"])
    func alertBody(tag: String) {
        let a = alert(blocks("> [!\(tag)]\n> Steps:\n> - one\n>   1. inner\n> ```sh\n> make\n> ```").first)
        #expect(a?.kind == MDAlertKind(rawValue: tag.lowercased()))
        #expect(a?.title == [.text(MDAlertKind(rawValue: tag.lowercased())!.title)])
        #expect(a?.body.first == .paragraph([.text("Steps:")]))
        #expect(list(a?.body.dropFirst().first)?.items.first?.blocks.last.flatMap(list)?.ordered == true)
        #expect(code(a?.body.last) == MDCode(language: "sh", code: "make", fenced: true))
    }

    @Test("an alert's title keeps its marks and its body may be empty")
    func alertMarks() {
        #expect(alert(blocks("> [!NOTE] Read **this**").first) == MDAlert(kind: .note, title: [.text("Read "), .strong([.text("this")])], body: []))
    }

    @Test("a tag that isn't the quote's opening leaves it a quote")
    func lateTag() {
        #expect(alert(blocks("> Hello [!NOTE]").first) == nil)
    }

    @Test("a footnote can hold several blocks and be referenced from a list")
    func richFootnote() {
        let all = blocks("- item[^n]\n\n[^n]: First.\n\n    Second paragraph.")
        guard case .footnotes(let notes)? = all.last else {
            Issue.record("no footnotes in \(all)")
            return
        }
        #expect(notes.first?.blocks == [.paragraph([.text("First.")]), .paragraph([.text("Second paragraph.")])])
        #expect(list(all.first)?.items.first?.blocks == [.paragraph([.text("item"), .footnote(1)])])
    }

    @Test("a reference to a footnote never defined stays text")
    func missingFootnote() {
        #expect(paragraph("See[^nope].").text == "See[^nope].")
    }

    @Test("bare web addresses and emails become links")
    func autolinks() {
        #expect(paragraph("Go to www.example.com now") == [.text("Go to "), .link(destination: "http://www.example.com", title: nil, [.text("www.example.com")]), .text(" now")])
        #expect(paragraph("See https://x.dev/a?b=1.") == [.text("See "), .link(destination: "https://x.dev/a?b=1", title: nil, [.text("https://x.dev/a?b=1")]), .text(".")])
        #expect(paragraph("Mail foo@bar.com") == [.text("Mail "), .link(destination: "mailto:foo@bar.com", title: nil, [.text("foo@bar.com")])])
        #expect(paragraph("<https://a.b/c>") == [.link(destination: "https://a.b/c", title: nil, [.text("https://a.b/c")])])
    }

    @Test("a reference link resolves to its definition")
    func referenceLink() {
        let all = blocks("Read [the docs][d] and [d].\n\n[d]: https://docs.dev \"Docs\"")
        #expect(all == [.paragraph([
            .text("Read "), .link(destination: "https://docs.dev", title: "Docs", [.text("the docs")]), .text(" and "),
            .link(destination: "https://docs.dev", title: "Docs", [.text("d")]), .text("."),
        ])])
    }

    @Test("emphasis follows CommonMark's flanking rules", arguments: [
        ("snake_case_name", [MDInline.text("snake_case_name")]),
        ("*a **b** c*", [.emphasis([.text("a "), .strong([.text("b")]), .text(" c")])]),
        ("**a*b*c**", [.strong([.text("a"), .emphasis([.text("b")]), .text("c")])]),
        ("foo*bar*baz", [.text("foo"), .emphasis([.text("bar")]), .text("baz")]),
        ("~~gone~~ and ~one~", [.strikethrough([.text("gone")]), .text(" and "), .strikethrough([.text("one")])]),
        ("a * not emphasis *", [.text("a * not emphasis *")]),
        ("2 * 3 * 4", [.text("2 * 3 * 4")]),
        ("***both***", [.emphasis([.strong([.text("both")])])]),
        ("\\*literal\\*", [.text("*literal*")]),
    ])
    func emphasis(source: String, expected: [MDInline]) {
        #expect(paragraph(source) == expected)
    }

    @Test("smart punctuation is off")
    func noSmartQuotes() {
        #expect(paragraph("\"quoted\" -- it's...") == [.text("\"quoted\" -- it's...")])
    }

    @Test("headings take levels one to six, ATX or setext")
    func headings() {
        #expect(blocks("# a\n## b\n### c\n#### d\n##### e\n###### f\n####### g") == [
            .heading(level: 1, [.text("a")]), .heading(level: 2, [.text("b")]), .heading(level: 3, [.text("c")]),
            .heading(level: 4, [.text("d")]), .heading(level: 5, [.text("e")]), .heading(level: 6, [.text("f")]),
            .paragraph([.text("####### g")]),
        ])
        #expect(blocks("Title\n=====") == [.heading(level: 1, [.text("Title")])])
    }

    @Test("an image keeps its alt text and title")
    func image() {
        #expect(paragraph("![alt *b*](https://x.dev/i.png \"T\")") == [.image(MDImage(source: "https://x.dev/i.png", alt: "alt b", title: "T"))])
    }

    @Test("a link keeps its marks and title")
    func link() {
        #expect(paragraph("[**a** b](src/a.rs#L3 \"t\")") == [.link(destination: "src/a.rs#L3", title: "t", [.strong([.text("a")]), .text(" b")])])
    }

    @Test("thematic breaks in each style are rules")
    func rules() {
        #expect(blocks("***\n\n- - -\n\n___") == [.rule, .rule, .rule])
    }

    @Test("a quote holds blocks and nests")
    func quotes() {
        #expect(blocks("> a\n>\n> > b") == [.quote([.paragraph([.text("a")]), .quote([.paragraph([.text("b")])])])])
    }

    @Test("indented code keeps its lines and is labelled text")
    func indentedCode() {
        #expect(code(blocks("    a\n      b").first) == MDCode(language: nil, code: "a\n  b", fenced: false))
    }

    @Test("a fence's language is the first word of its info string")
    func infoString() {
        #expect(code(blocks("```TypeScript title=\"a.ts\"\nx\n```").first)?.language == "typescript")
    }
}

@Suite struct HTMLBlocks {
    @Test("a br inside an html block becomes a break")
    func br() {
        #expect(blocks("<p>a<br>b</p>") == [.paragraph([.text("a"), .hardBreak, .text("b")])])
    }

    @Test("details shows its summary strong and its markdown body")
    func details() {
        #expect(blocks("<details>\n<summary>More</summary>\n\nBody **here**\n\n</details>") == [.paragraph([.strong([.text("More")])]), .paragraph([.text("Body "), .strong([.text("here")])])])
    }

    @Test("unsupported tags are dropped and their text kept; scripts go whole")
    func unsupported() {
        #expect(blocks("<div><custom-tag>hi</custom-tag></div>") == [.paragraph([.text("hi")])])
        #expect(blocks("<script>alert(1)</script>") == [])
        #expect(blocks("<!-- a comment -->") == [])
    }

    @Test("html that doesn't tokenize shows as its source")
    func failed() {
        #expect(blocks("<div class=\"x") == [.paragraph([.text("<div class=\"x")])])
    }

    @Test("html headings, lists, quotes, code and rules map to their blocks")
    func structure() {
        let html = "<div>\n<h2>T</h2><ul><li>a</li><li><b>b</b></li></ul><blockquote>q</blockquote><pre><code class=\"language-go\">x := 1\n</code></pre><hr>\n</div>"
        #expect(blocks(html) == [
            .heading(level: 2, [.text("T")]),
            .list(MDList(ordered: false, items: [MDListItem(blocks: [.paragraph([.text("a")])]), MDListItem(blocks: [.paragraph([.strong([.text("b")])])])])),
            .quote([.paragraph([.text("q")])]), .code(MDCode(language: "go", code: "x := 1", fenced: true)), .rule,
        ])
    }

    @Test("an html table reads its header, cells and alignment")
    func htmlTable() {
        let t = table(blocks("<table><tr><th align=\"right\">n</th><th>name</th></tr><tr><td>1</td><td>a &amp; b</td></tr></table>").first)
        #expect(t == MDTable(alignments: [.right, .none], header: [[.text("n")], [.text("name")]], rows: [[[.text("1")], [.text("a & b")]]]))
    }

    @Test("an html image block keeps its size")
    func imageBlock() {
        #expect(blocks("<p align=\"center\"><img src=\"logo.png\" width=\"120\"></p>") == [.paragraph([.image(MDImage(source: "logo.png", alt: "", width: 120))])])
    }

    @Test("inline html marks wrap what they enclose; unknown and unclosed ones don't")
    func inlineMarks() {
        #expect(paragraph("a <b>bold</b> <i>it</i> <u>u</u> <s>s</s> <kbd>⌘K</kbd> <a href=\"https://x.dev\">l</a> <span>p</span> <b>open")
            == [
                .text("a "), .strong([.text("bold")]), .text(" "), .emphasis([.text("it")]), .text(" "), .underline([.text("u")]), .text(" "),
                .strikethrough([.text("s")]), .text(" "), .code("⌘K"), .text(" "), .link(destination: "https://x.dev", title: nil, [.text("l")]),
                .text(" p open"),
            ])
    }

    @Test("an inline br breaks the line")
    func inlineBr() {
        #expect(paragraph("a<br/>b") == [.text("a"), .hardBreak, .text("b")])
    }

    @Test("entities decode in html text")
    func entities() {
        #expect(HTML.decode("a &lt;b&gt; &#65;&#x42; &unknown; &") == "a <b> AB &unknown; &")
    }
}

@Suite struct Streaming {
    static let document = """
        # Plan

        Intro with **bold**, `src/a.rs` and a [link](https://x.dev).

        - one
        - two

          still two
        - [x] three

        > [!NOTE]
        > Remember this.

        | a | b |
        |:--|--:|
        | 1 | 2 |

        ```swift
        let a = 1

        print(a)
        ```

        <details>
        <summary>More</summary>

        Hidden.

        </details>

        $$
        x^2
        $$

        1. last
        2. list

        ---

        Done.
        """

    static let tricky = "Para line\n---\n\nText\n===\n\n- a\n- b\n-\n\nfoo\n    not code\n\n    code\n\nx | y\n--|--\n1 | 2\nafter\n\n> quote\nlazy\n\n<div>\n*x*\n</div>\n\n***\n1. a\n1) b\n\n~~~\n```\n~~~\n"
    static let unicode = "中文段落 **粗体**\n\n- 項目 👍🏽\n- [ ] タスク\n\n| 列 | 🎉 |\n|---|---|\n| ٱ | ש |\n\nمرحبا `src/ملف.rs`"

    @Test("a document streamed character by character always parses as it would whole", arguments: [
        document, tricky, unicode, document.replacingOccurrences(of: "\n", with: "\r\n"), "line\rnext\r\r- a\r- b",
    ])
    func growsLikeWhole(document: String) {
        var previous = Markdown.parse("")
        var cut = document.unicodeScalars.startIndex
        while cut < document.unicodeScalars.endIndex {
            cut = document.unicodeScalars.index(after: cut)
            let text = String(document.unicodeScalars[..<cut])
            let grown = Markdown.parse(text, growing: previous)
            let whole = Markdown.parse(text)
            if grown.blocks.map(\.block) != whole.blocks.map(\.block) {
                Issue.record("grown and whole differ at \(text.count): \(text.suffix(30).debugDescription)")
            }
            #expect(grown.blocks.map(\.id) == Array(grown.blocks.indices))
            previous = grown
        }
    }

    @Test("growing keeps every block before the last one as it was")
    func keepsEarlierBlocks() {
        let before = Markdown.parse("# A\n\nPara one.\n\nPara tw")
        let after = Markdown.parse("# A\n\nPara one.\n\nPara two, longer.", growing: before)
        #expect(after.blocks.prefix(2) == before.blocks.prefix(2))
        #expect(after.blocks[2] != before.blocks[2])
        #expect(after.blocks[2].block == .paragraph([.text("Para two, longer.")]))
    }

    @Test("growth after a reference definition or footnote re-parses the whole")
    func globalsReparse() {
        let before = Markdown.parse("See [x].\n\n")
        let after = Markdown.parse("See [x].\n\n[x]: https://x.dev", growing: before)
        #expect(after.blocks.map(\.block) == [.paragraph([.text("See "), .link(destination: "https://x.dev", title: nil, [.text("x")]), .text(".")])])
    }

    @Test("text that isn't a longer version re-parses whole")
    func rewritten() {
        let before = Markdown.parse("# A\n\nB")
        #expect(Markdown.parse("# Z\n\nB", growing: before).blocks.map(\.block) == [.heading(level: 1, [.text("Z")]), .paragraph([.text("B")])])
    }

    @Test("the cache hands back the same document for the same text and grows from a recent one")
    func cache() {
        let cache = MarkdownCache()
        let first = cache.document("# Title\n\nSome")
        #expect(cache.cached("# Title\n\nSome") == first)
        let grown = cache.document("# Title\n\nSome more")
        #expect(grown.blocks.first == first.blocks.first)
        #expect(grown.blocks.map(\.block) == Markdown.parse("# Title\n\nSome more").blocks.map(\.block))
        #expect(cache.cached("never seen") == nil)
    }

    @Test("digests are stable and tell texts apart")
    func digest() {
        #expect(Markdown.digest("a") == 0xaf63_dc4c_8601_ec8c)
        #expect(Markdown.digest("ab") != Markdown.digest("ba"))
    }

    @Test("line offsets count every kind of line ending")
    func lineOffsets() {
        let text = "a\nb\r\nc\rd"
        #expect(Markdown.lineOffsets(text.utf8[...]) == [0, 2, 5, 7])
    }
}
