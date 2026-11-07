import Highlight
import SwiftUI
import Testing
import DiffModel
import Fixtures
@testable import CodeView

@Suite("Code lines")
struct CodeLinesTests {
    @Test("text splits on every newline style, without an empty line after the last")
    func split() {
        #expect(CodeLines("a\nb\r\nc\rd\n").lines == ["a", "b", "c", "d"])
        #expect(CodeLines("a\n\nb").lines == ["a", "", "b"])
        #expect(CodeLines("").lines == [])
        #expect(CodeLines("é😀\nü").lines == ["é😀", "ü"])
    }

    @Test("the gutter is as wide as the last line's number")
    func digits() {
        #expect(CodeLines("a").digits == 2)
        #expect(CodeLines(String(repeating: "x\n", count: 1200)).digits == 4)
    }

    @Test("lines are coloured a window of two hundred at a time")
    func windows() {
        #expect(HighlightWindow.window(of: 199) == 0 && HighlightWindow.window(of: 200) == 1)
        #expect(HighlightWindow.range(2, count: 450) == 400..<450)
    }

    @Test("spans over joined lines come back to each line, clipped at its ends")
    func perLine() {
        let lines = ["let a", "/* x", "y */", "é b"][...]
        let spans = [
            HighlightSpan(range: 0..<3, kind: .keyword), HighlightSpan(range: 6..<15, kind: .comment), HighlightSpan(range: 18..<19, kind: .plain),
        ]
        let got = HighlightWindow.perLine(spans, lines: lines)
        #expect(got[0] == [HighlightSpan(range: 0..<3, kind: .keyword)])
        #expect(got[1] == [HighlightSpan(range: 0..<4, kind: .comment)])
        #expect(got[2] == [HighlightSpan(range: 0..<4, kind: .comment)])
        #expect(got[3] == [HighlightSpan(range: 2..<3, kind: .plain)])
    }

    @Test("plain text skips the highlighter")
    func plain() {
        #expect(HighlightWindow.spans(["a", "b"], language: "text") == [[], []])
    }
}

@Suite("Code style")
struct CodeStyleTests {
    @Test("changed words tint their UTF-16 range and nothing else")
    func words() {
        var runs = LineRuns()
        runs.code("é = 2", spans: [], words: [4..<5], word: 0x00ff_00ff, dark: false)
        let utf16 = Array(runs.string.utf16)
        #expect(runs.tints.map { String(decoding: utf16[$0.range], as: UTF16.self) } == ["2"])
    }

    @Test("a range past the line's end is ignored")
    func outside() {
        var runs = LineRuns()
        runs.code("ab", spans: [HighlightSpan(range: 1..<9, kind: .keyword)], words: [5..<6], word: 0xff00_00ff, dark: true)
        #expect(runs.string == "ab")
        #expect(runs.tints.isEmpty && runs.colors.isEmpty)
    }

    @Test("a diff row is its numbers right-aligned, its sign, then its code, with word tints moved past the gutter")
    func diffRow() {
        let line = UnifiedLine(id: 3, kind: .added, new: 12, text: "let a = 2", words: [8..<9], cut: true)
        let runs = CodeStyle.diffRow(line, gutter: .both, digits: 3, spans: [HighlightSpan(range: 0..<3, kind: .keyword)], dark: false)
        #expect(runs.string == "     12 + let a = 2 …")
        let utf16 = Array(runs.string.utf16)
        #expect(runs.tints.map { String(decoding: utf16[$0.range], as: UTF16.self) } == ["2"])
        #expect(runs.colors.contains { String(decoding: utf16[$0.range], as: UTF16.self) == "let" })
        #expect(CodeStyle.diffRow(line, gutter: .old, digits: 2, spans: [], dark: false).string.hasPrefix("   + "))
    }

    @Test("a drawn line is as wide as its text and puts a word's tint over that word")
    func drawn() {
        var runs = LineRuns()
        runs.code("abcd", spans: [], words: [2..<4], word: 0x00ff_00ff, dark: false)
        let text = DrawnText(runs, size: 13, color: 0x0000_00ff)
        #expect(text.width > 4 * 6 && text.width < 4 * 10)
    }
}

@Suite("Code rows")
struct CodeRowsTests {
    private func pieces(_ text: String, _ rows: CodeRows) -> [String] {
        let u = Array(text.utf16)
        return (0..<rows.count).map { i in rows[i].more ? "…" : String(decoding: u[rows[i].range ?? 0..<u.count], as: UTF16.self) }
    }

    @Test("unwrapped, every line is one whole row that carries its number")
    func whole() {
        let rows = CodeRows(lineCount: 3)
        #expect(rows.count == 3)
        #expect(rows[1] == CodeRows.Row(line: 1, range: nil))
        #expect(rows[1].first)
        #expect(rows.first(of: 2) == 2 && rows.rows(of: 2) == 2..<3)
    }

    @Test("a wrapped line breaks after the last space that fits")
    func words() {
        let text = "let total = price * count"
        #expect(pieces(text, CodeRows([text], columns: 12)) == ["let total = ", "price * ", "count"])
    }

    @Test("a word longer than a row breaks inside it")
    func long() {
        #expect(pieces("abcdefghij", CodeRows(["abcdefghij"], columns: 4)) == ["abcd", "efgh", "ij"])
    }

    @Test("an empty line still takes a row, and only a line's first row carries its number")
    func empty() {
        let rows = CodeRows(["", "abcdef"], columns: 3)
        #expect(rows.count == 3)
        #expect(rows[0].range == 0..<0)
        #expect(rows[1].first && !rows[2].first)
    }

    @Test("a line past the limit stops there with a row for the rest, until it's expanded")
    func limit() {
        let text = String(repeating: "x", count: 30)
        let cut = CodeRows([text], columns: 10, limit: 25)
        #expect(pieces(text, cut).map(\.count) == [10, 10, 5, 1])
        #expect(cut[3].more && !cut[3].first)
        #expect(pieces(text, CodeRows([text], columns: 10, limit: 25, expanded: [0])).map(\.count) == [10, 10, 10])
    }

    @Test("a minified file of one 512 KB line wraps only its first thousand characters")
    func minified() {
        let rows = CodeRows([String(repeating: "a", count: 512 * 1024)], columns: 40)
        #expect(rows.count == 27)
        #expect(rows[25].range == 1000..<1024)
        #expect(rows[26].more)
    }

    @Test("wide characters take two columns")
    func wide() {
        let text = "日本語です"
        #expect(pieces(text, CodeRows([text], columns: 4)) == ["日本", "語で", "す"])
    }

    @Test("a line's rows are found from its index")
    func lookup() {
        let rows = CodeRows(["abcdefgh", "ab", "abcdef"], columns: 3)
        #expect(rows.first(of: 1) == 3)
        #expect(rows.rows(of: 2) == 4..<6)
        #expect(rows.first(of: 3) == rows.count)
    }
}

@Suite("Find in code")
struct CodeFindTests {
    @Test("matches ignore case and point at UTF-16 ranges of their line")
    func matches() {
        let find = CodeFind("ab", in: ["xAb ab", "é ab", "none"])
        #expect(find.matches.map(\.line) == [0, 0, 1])
        #expect(find.matches.map(\.range) == [1..<3, 4..<6, 2..<4])
        #expect(find.ranges(in: 0) == [1..<3, 4..<6] && find.ranges(in: 2).isEmpty)
        #expect(find.position == "1 of 3")
    }

    @Test("stepping wraps around at both ends")
    func step() {
        var find = CodeFind("a", in: ["a", "a", "a"])
        find.step(by: -1)
        #expect(find.position == "3 of 3" && find.match?.line == 2)
        find.step()
        #expect(find.position == "1 of 3")
    }

    @Test("an empty query says nothing; a query found nowhere says so")
    func nothing() {
        #expect(CodeFind("", in: ["a"]).position == "")
        let none = CodeFind("zz", in: ["a"])
        #expect(none.position == "No matches" && none.match == nil)
    }

    @Test("a search run again over reloaded lines resumes at the first match from the old current one")
    func resume() throws {
        var old = CodeFind("a", in: ["a", "b a", "a"])
        old.step(by: 2)
        var find = CodeFind("a", in: ["a", "x", "b a a", "a"])
        find.resume(at: try #require(old.match))
        #expect(find.match == CodeFind.Match(line: 2, range: 2..<3))
        find.resume(at: CodeFind.Match(line: 9, range: 0..<1))
        #expect(find.position == "4 of 4")
    }
}

@Suite("Picking lines")
struct LinePickTests {
    private let diff = [
        UnifiedLine(id: 0, kind: .hunk, text: "@@ -1,2 +1,3 @@"), UnifiedLine(id: 1, kind: .context, old: 1, new: 1, text: "a"),
        UnifiedLine(id: 2, kind: .deleted, old: 2, text: "b"), UnifiedLine(id: 3, kind: .added, new: 2, text: "c"),
        UnifiedLine(id: 4, kind: .added, new: 3, text: "d"), UnifiedLine(id: 5, kind: .hunk, text: "@@ -9 +10 @@"),
        UnifiedLine(id: 6, kind: .added, new: 10, text: "y"),
    ]

    @Test("dragging stretches the pick from where the press started")
    func drag() {
        var pick = LinePick(5)
        let moved = [pick.drag(to: 2), pick.drag(to: 2)]
        #expect(moved == [true, false])
        #expect(pick.lines == 2...5 && pick.anchor == 5)
    }

    @Test("a handle moves its own edge and keeps the other")
    func handles() {
        var pick = LinePick(3)
        _ = pick.drag(to: 6)
        pick.grab(.top)
        let top = pick.drag(to: 1)
        #expect(top && pick.lines == 1...6)
        pick.grab(.bottom)
        let bottom = [pick.drag(to: 4), pick.drag(to: 4)]
        #expect(bottom == [true, false] && pick.lines == 1...4)
    }

    @Test("a handle dragged past the other edge keeps that edge, there and back")
    func crossing() {
        var pick = LinePick(3)
        _ = pick.drag(to: 6)
        pick.grab(.top)
        var seen: [ClosedRange<Int>] = []
        for line in [8, 9, 5] {
            _ = pick.drag(to: line)
            seen.append(pick.lines)
        }
        #expect(seen == [6...8, 6...9, 5...6])
    }

    @Test("in a diff a pick stops at the edge of its hunk")
    func hunk() {
        var pick = LinePick(1)
        let sameHunk = { LinePick.sameHunk(diff, $0, $1) }
        let moved = [pick.drag(to: 4, allowed: sameHunk), pick.drag(to: 6, allowed: sameHunk)]
        pick.grab(.top)
        let top = pick.drag(to: 0, allowed: sameHunk)
        #expect(moved == [true, false] && !top)
        #expect(pick.lines == 1...4)
    }

    @Test("picked code quotes as its path, lines and a block fenced in the file's language")
    func code() throws {
        let quote = try #require(CodeQuote.code(["a", "b", "c", "d"], pick: 1...2, language: "rust"))
        #expect(quote.payload(path: "src/a.rs") == "src/a.rs:2-3\n```rust\nb\nc\n```")
        #expect(quote.title == "Lines 2–3")
        let one = try #require(CodeQuote.code(["hi"], pick: 0...0, language: "text"))
        #expect(one.payload(path: "notes.txt") == "notes.txt:1\n```\nhi\n```")
        #expect(one.title == "Line 1")
        #expect(CodeQuote.code(["", "  "], pick: 0...1, language: "rust") == nil)
    }

    @Test("picked diff lines quote with their signs, numbered on the new side unless they only remove")
    func diffQuote() throws {
        let quote = try #require(CodeQuote.diff(diff, pick: 1...4))
        #expect(quote.payload(path: "src/a.rs") == "src/a.rs:1-3, uncommitted change\n```diff\n a\n-b\n+c\n+d\n```")
        let removed = try #require(CodeQuote.diff(diff, pick: 2...2))
        #expect(removed.payload(path: "src/a.rs") == "src/a.rs:2 (before), uncommitted change\n```diff\n-b\n```")
        #expect(CodeQuote.diff(diff, pick: 0...0) == nil)
    }
}

@Suite("Wrapped rows")
struct WrappedRowTests {
    @Test("on a narrow screen a diff row shows one number: the new line's, or the old one's for a deletion")
    func oneNumber() {
        let added = UnifiedLine(id: 0, kind: .added, new: 12, text: "x")
        let deleted = UnifiedLine(id: 1, kind: .deleted, old: 7, text: "y")
        #expect(CodeStyle.diffRow(added, gutter: .one, digits: 3, spans: [], dark: false).string == " 12 + x")
        #expect(CodeStyle.diffRow(deleted, gutter: .one, digits: 3, spans: [], dark: false).string == "  7 − y")
    }

    @Test("a wrapped diff line's later rows leave the gutter blank, and only the last shows the Mac's cut")
    func diffPieces() {
        let line = UnifiedLine(id: 0, kind: .added, new: 12, text: "abcdef", cut: true)
        #expect(CodeStyle.diffRow(line, gutter: .one, digits: 2, spans: [], dark: false, part: 0..<3).string == "12 + abc")
        #expect(CodeStyle.diffRow(line, gutter: .one, digits: 2, spans: [], dark: false, part: 3..<6).string == "     def …")
    }

    @Test("a wrapped source line keeps the gutter's width on its later rows but not its number")
    func codePieces() {
        #expect(CodeStyle.codeRow("abcdef", number: 1, digits: 2, spans: [], dark: false, part: 0..<3).string == " 1  abc")
        #expect(CodeStyle.codeRow("abcdef", number: nil, digits: 2, spans: [], dark: false, part: 3..<6).string == "    def")
        #expect(CodeStyle.codeRow("abc", number: nil, digits: nil, spans: [], dark: false).string == "abc")
    }

    @Test("colours and word tints crossing a wrap are clipped to each piece")
    func clipped() {
        var runs = LineRuns()
        runs.code("let abc", spans: [HighlightSpan(range: 0..<3, kind: .keyword)], words: [4..<7], word: 0x00ff_00ff, dark: false, part: 2..<5)
        let utf16 = Array(runs.string.utf16)
        #expect(runs.string == "t a")
        #expect(runs.colors.map { String(decoding: utf16[$0.range], as: UTF16.self) } == ["t"])
        #expect(runs.tints.map { String(decoding: utf16[$0.range], as: UTF16.self) } == ["a"])
    }
}

@Suite("Diff scroll target")
struct DiffTargetTests {
    @Test("a target that comes with a new document waits for that document's rows, then shows once")
    func waitsForItsDocument() {
        let old = DiffDocument(Sample.smallDiff)
        var grown = old
        grown.lines += old.lines
        let target = old.lines.count - 1
        #expect(DiffView.target(target, shown: old, document: grown, done: nil) == nil)
        #expect(DiffView.target(target, shown: grown, document: grown, done: nil) == target)
        #expect(DiffView.target(target, shown: grown, document: grown, done: target) == nil)
    }
}
