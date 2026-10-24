import Foundation
import Highlight
import MarkdownModel
import Testing
@testable import MarkdownView

@Suite struct Links {
    @Test("a link opens a web page or a file under the agent's folder")
    func target() {
        #expect(MarkdownLink.target("https://x.dev/a#b") == .web(URL(string: "https://x.dev/a#b")!))
        #expect(MarkdownLink.target("src/a.rs") == .file(path: "src/a.rs", line: nil))
        #expect(MarkdownLink.target("src/a.rs:42:7") == .file(path: "src/a.rs", line: 42))
        #expect(MarkdownLink.target("src/a.rs:42") == .file(path: "src/a.rs", line: 42))
        #expect(MarkdownLink.target("a.rs#L42") == .file(path: "a.rs", line: 42))
        #expect(MarkdownLink.target("a.rs#L42-L50") == .file(path: "a.rs", line: 42))
        #expect(MarkdownLink.target("file:///etc/hosts") == .file(path: "/etc/hosts", line: nil))
        #expect(MarkdownLink.target("docs/my%20notes.md") == .file(path: "docs/my notes.md", line: nil))
    }

    @Test("mail and phone links open outside, and a bare anchor goes nowhere")
    func otherSchemes() {
        #expect(MarkdownLink.target("mailto:a@b.dev") == .web(URL(string: "mailto:a@b.dev")!))
        #expect(MarkdownLink.target("#usage") == nil)
        #expect(MarkdownLink.target("") == nil)
    }

    @Test("a link's destination survives the trip through its tap URL")
    func roundTrip() {
        for destination in ["src/a b.rs:3", "https://x.dev/?q=1&r=2#f", "中文/文件.md", "a+b%20c"] {
            #expect(LinkURL.url(for: destination).flatMap(LinkURL.destination) == destination)
        }
    }
}

/// Ported from the desktop's `pocket/chat/markdown.rs` highlight cache tests.
@Suite struct Colours {
    let span = HighlightSpan(range: 0..<3, kind: .keyword)

    @Test("a code block is highlighted again when the appearance flips")
    func flips() {
        var h = CodeColours()
        #expect(h.get("r", code: "a();", dark: false).needsRun)
        h.set("r", length: 4, dark: false, spans: [])
        #expect(h.get("r", code: "a();", dark: false) == ([], false))
        #expect(h.get("r", code: "a();", dark: true) == (nil, true))
        h.set("r", length: 4, dark: false, spans: [])
        #expect(h.get("r", code: "a();", dark: true) == (nil, false), "a light run landing late doesn't fill the dark slot")
    }

    @Test("a growing code block keeps its old colours until the new run lands")
    func grows() {
        var h = CodeColours()
        _ = h.get("r", code: "let a", dark: false)
        h.set("r", length: 5, dark: false, spans: [span])
        #expect(h.get("r", code: "let a = 1;", dark: false) == ([span], true))
        #expect(h.get("r", code: "let a = 1;", dark: false) == ([span], false), "asked once per length")
        h.set("r", length: 5, dark: false, spans: [])
        #expect(h.get("r", code: "let a = 1;", dark: false).spans == [span], "a stale run doesn't replace them")
        let more = [span, HighlightSpan(range: 8..<9, kind: .number)]
        h.set("r", length: 10, dark: false, spans: more)
        #expect(h.get("r", code: "let a = 1;", dark: false) == (more, false))
    }

    @Test("kept colours stop at the code and never split a character")
    func kept() {
        var h = CodeColours()
        _ = h.get("r", code: "a😀 b ``", dark: false)
        // "a😀 b": a=0, 😀=1..<3, space=3, b=4.
        h.set("r", length: 8, dark: false, spans: [
            HighlightSpan(range: 0..<2, kind: .string), HighlightSpan(range: 4..<6, kind: .string), HighlightSpan(range: 6..<8, kind: .string),
        ])
        #expect(h.get("r", code: "a😀 b", dark: false).spans == [HighlightSpan(range: 4..<5, kind: .string)])
    }

    @Test("coloured code paints each span in the palette's colour for the appearance")
    func coloured() {
        let code = "let 😀 = 1"
        let spans = Highlighter.spans(code, language: "swift")
        let light = colouredCode(code, spans, dark: false)
        #expect(String(light.characters) == code)
        #expect(light.runs.count > 1)
    }
}

@Suite struct Tables {
    @Test("columns that fit grow evenly to fill the width")
    func fill() {
        #expect(tableColumnWidths(natural: [100, 50], available: 250) == [150, 100])
    }

    @Test("columns too wide wrap toward a floor of about two lines")
    func shrink() {
        let widths = tableColumnWidths(natural: [600, 100], available: 500)
        #expect(widths[1] == 100, "a column narrower than its floor keeps its width")
        #expect(widths[0] == 400)
    }

    @Test("past every floor the table keeps its floors and scrolls")
    func scrolls() {
        let widths = tableColumnWidths(natural: [1200, 1000, 300], available: 390)
        #expect(widths == [480, 480, 160])
        #expect(widths.reduce(0, +) > 390)
    }
}

@Suite struct Blocks {
    @Test("images split a paragraph and a run of them shares a row")
    func pieces() {
        let a = MDImage(source: "https://img.shields.io/a.svg", alt: "a")
        let b = MDImage(source: "https://img.shields.io/b.svg", alt: "b")
        let pieces = ParagraphView.pieces([.text("Badges:"), .softBreak, .image(a), .text(" "), .link(destination: "https://x.dev", title: nil, [.image(b)]), .softBreak, .text("after")])
        guard pieces.count == 3, case .text(let first) = pieces[0], case .images(let images) = pieces[1], case .text(let last) = pieces[2] else {
            Issue.record("unexpected pieces \(pieces)")
            return
        }
        #expect(first == [.text("Badges:"), .softBreak])
        #expect(images == [LinkedImage(image: a, link: nil), LinkedImage(image: b, link: "https://x.dev")])
        #expect(last == [.text("after")])
    }

    @Test("a paragraph without images is one piece of text")
    func onePiece() {
        #expect(ParagraphView.pieces([.text("a")]).count == 1)
    }

    @Test("a footnote ends with a back arrow per reference")
    func backrefs() {
        let note = MDFootnote(number: 1, blocks: [.paragraph([.text("Beta.")])], references: 2)
        #expect(FootnotesView.withBackrefs(note) == [.paragraph([.text("Beta."), .text(" ↩"), .text(" ↩")])])
        let code = MDFootnote(number: 2, blocks: [.code(MDCode(language: nil, code: "x", fenced: true))], references: 1)
        #expect(FootnotesView.withBackrefs(code).last == .paragraph([.text(" ↩")]))
    }

    @Test("data URLs decode to their bytes")
    func dataURL() {
        #expect(dataURLBytes("data:image/png;base64,aGVsbG8=") == Data("hello".utf8))
        #expect(dataURLBytes("data:text/plain,a%20b") == Data("a b".utf8))
        #expect(dataURLBytes("https://x.dev/a.png") == nil)
    }

    @Test("the styles keep the desktop's sizes and grow with the reader's text size")
    func metrics() {
        let chat = Metrics.of(.chat)
        #expect((chat.body, chat.line) == (14, 24))
        #expect(abs(chat.indent - 26.6) < 0.001)
        #expect(chat.headings.map(\.size) == [19, 16, 15, 14])
        #expect(chat.heading(6).line == 22)
        #expect(Metrics.of(.chat, scale: Metrics.scale(.accessibility1)).body > 20)
    }
}

@Suite struct Cache {
    @Test("a short document is parsed on the spot; a long one waits unless it only grew a little")
    func cheap() {
        let cache = MarkdownCache()
        #expect(cache.documentIfCheap("# Hi") != nil)
        let long = String(repeating: "Paragraph text that goes on.\n\n", count: 2000)
        #expect(cache.documentIfCheap(long) == nil)
        _ = cache.document(long)
        #expect(cache.documentIfCheap(long + "More") != nil)
    }
}
