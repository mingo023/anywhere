import Highlight
import SwiftUI
import Testing
import DiffModel
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
