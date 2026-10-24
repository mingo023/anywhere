import Highlight
import SwiftUI
import Testing
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
        let text = CodeStyle.attributed("é = 2", spans: [], words: [4..<5], word: .green, dark: false)
        let tinted = text.runs.filter { $0.backgroundColor != nil }.map { String(text[$0.range].characters) }
        #expect(tinted == ["2"])
    }

    @Test("a range past the line's end is ignored")
    func outside() {
        let text = CodeStyle.attributed("ab", spans: [HighlightSpan(range: 1..<9, kind: .keyword)], words: [5..<6], word: .red, dark: true)
        #expect(String(text.characters) == "ab")
        #expect(text.runs.allSatisfy { $0.backgroundColor == nil })
    }
}
