import CoreGraphics
import FeatureTerminal
import Fixtures
import Foundation
import Testing
import Wire

private func text(_ b: [UInt8]) -> String { String(decoding: b, as: UTF8.self) }

@Suite("Keys")
struct KeyTests {
    @Test("characters are encoded by their text, Ctrl to its control byte and Alt behind an escape")
    func characters() {
        #expect(KeyEncoder.bytes(.text("c"), mods: .ctrl) == [3])
        #expect(KeyEncoder.bytes(.text("C"), mods: .ctrl) == [3])
        #expect(KeyEncoder.bytes(.text("b"), mods: .alt) == Array("\u{1b}b".utf8))
        #expect(KeyEncoder.bytes(.text("["), mods: .ctrl) == [0x1b])
        #expect(KeyEncoder.bytes(.text(" "), mods: .ctrl) == [0])
        #expect(KeyEncoder.bytes(.text("/"), mods: .ctrl) == [0x1f])
        #expect(KeyEncoder.bytes(.text("é"), mods: .ctrl) == Array("é".utf8))
        #expect(KeyEncoder.bytes(.text("~")) == Array("~".utf8))
    }

    @Test("arrows follow the program's cursor mode, and a modifier always takes the CSI form")
    func arrows() {
        #expect(text(KeyEncoder.bytes(.up)) == "\u{1b}[A")
        #expect(text(KeyEncoder.bytes(.up, applicationCursor: true)) == "\u{1b}OA")
        #expect(text(KeyEncoder.bytes(.left, mods: .ctrl, applicationCursor: true)) == "\u{1b}[1;5D")
        #expect(text(KeyEncoder.bytes(.right, mods: [.ctrl, .alt])) == "\u{1b}[1;7C")
        #expect(text(KeyEncoder.bytes(.pageUp)) == "\u{1b}[5~")
        #expect(text(KeyEncoder.bytes(.pageDown, mods: .alt)) == "\u{1b}[6;3~")
    }

    @Test("escape and tab are their bytes, behind an escape with Alt")
    func escapeAndTab() {
        #expect(KeyEncoder.bytes(.escape) == [0x1b])
        #expect(KeyEncoder.bytes(.tab) == [0x09])
        #expect(KeyEncoder.bytes(.tab, mods: .alt) == [0x1b, 0x09])
    }

    @Test("a sticky Ctrl applies to the next key, from the bar or the keyboard, then lets go")
    func sticky() {
        var mods = StickyMods()
        mods.toggle(.ctrl)
        #expect(mods.typed(Array("c".utf8)) == [3])
        #expect(mods.typed(Array("c".utf8)) == Array("c".utf8))
        mods.toggle(.alt)
        mods.toggle(.alt)
        #expect(mods.held.isEmpty)
        mods.toggle(.alt)
        #expect(text(mods.press(.left, applicationCursor: false)) == "\u{1b}[1;3D")
        #expect(mods.held.isEmpty)
    }
}

@Suite("Paste")
struct PasteTests {
    @Test("a bracketed paste is wrapped")
    func bracketed() {
        #expect(Paste.bytes("ls\n", bracketed: true) == Array("\u{1b}[200~ls\n\u{1b}[201~".utf8))
    }

    @Test("unbracketed newlines become returns")
    func returns() {
        #expect(Paste.bytes("a\nb", bracketed: false) == Array("a\rb".utf8))
    }

    @Test("control bytes are stripped from pastes")
    func stripped() {
        #expect(Paste.bytes("a\u{1b}[201~b", bracketed: true) == Array("\u{1b}[200~a [201~b\u{1b}[201~".utf8))
        #expect(Paste.bytes("x\u{03}y", bracketed: false) == Array("x y".utf8))
    }

    @Test("a multi-line paste is not safe")
    func safe() {
        #expect(Paste.isSafe("ls -la"))
        #expect(!Paste.isSafe("ls\nrm -rf ~"))
        #expect(!Paste.isSafe("a\u{1b}[201~b"))
    }

    @Test("a paste that could run commands asks unless turned off or bracketed")
    func asks() {
        let lines = "rm -rf build\nmake\n"
        #expect(Paste.asks(confirm: true, bracketed: false, lines))
        #expect(!Paste.asks(confirm: false, bracketed: false, lines))
        #expect(!Paste.asks(confirm: true, bracketed: true, lines))
        #expect(!Paste.asks(confirm: true, bracketed: false, "hello"))
    }
}

@Suite("Terminal list")
struct TerminalListTests {
    @Test("terminals group by the place each opened in, a Local apart from its project's folder")
    func groups() {
        let groups = TerminalGroup.of(Sample.terminals, locals: Sample.locals)
        #expect(groups.map(\.title) == ["calm-otter", "anywhere", "Review", "website"])
        #expect(groups.map(\.path) == ["~/.worktrees/anywhere/calm-otter", "~/dev/anywhere", "~/dev/anywhere", "~/dev/website"])
        #expect(groups[1].terminals.map(\.id) == ["t-shell"])
        #expect(groups[2].terminals.map(\.id) == ["t-review"])
    }

    @Test("a Local pocketd lists but the phone doesn't know yet is named by its folder")
    func unknownLocal() {
        let groups = TerminalGroup.of([Terminal(id: "t1", cwd: "/srv/app", cols: 80, rows: 24, localId: "new")], locals: [])
        #expect(groups.map(\.title) == ["app"])
        #expect(groups.map(\.path) == ["/srv/app"])
    }
}

@Suite("Fit")
struct FitTests {
    @Test("a phone fits the cols and rows its cells fill, within what a program can use")
    func grid() {
        let cell = CGSize(width: 7, height: 14)
        #expect(Fit.grid(CGSize(width: 393, height: 600), cell: cell) == (56, 42))
        #expect(Fit.grid(CGSize(width: 50, height: 20), cell: cell) == (20, 5))
        #expect(Fit.grid(CGSize(width: 393, height: 600), cell: .zero) == (80, 24))
    }

    @Test("a monospaced cell is as wide at every letter and grows with the font")
    func cell() {
        let small = Fit.cell(fontSize: 11)
        let large = Fit.cell(fontSize: 22)
        #expect(small.width > 5 && small.width < 9)
        #expect(abs(large.width - small.width * 2) < 0.5)
        #expect(large.height > small.height)
    }

    @Test("pinching zooms the font in half points, within its range")
    func zoom() {
        #expect(Fit.zoom(11, by: 1.5) == 16.5)
        #expect(Fit.zoom(11, by: 0.1) == Fit.fontSizes.lowerBound)
        #expect(Fit.zoom(20, by: 3) == Fit.fontSizes.upperBound)
    }
}

@Suite("Terminal session")
@MainActor
struct TerminalSessionTests {
    @Test("a snapshot replaces the screen at the terminal's size, and output draws on it")
    func snapshot() {
        let s = TerminalSession(id: "t1")
        s.feed(Data("old screen".utf8))
        s.replace(cols: 60, rows: 10, with: Data("$ ".utf8))
        #expect(s.cols == 60 && s.terminal.cols == 60 && s.terminal.rows == 10)
        s.feed(Data("echo hi\r\nhi\r\n$ ".utf8))
        #expect(Array(s.lines().prefix(3)) == ["$ echo hi", "hi", "$ "])
        #expect(!s.lines().joined().contains("old"))
    }

    @Test("the session reports the program's cursor and paste modes, for the keys it sends")
    func modes() {
        let s = TerminalSession(id: "t1")
        #expect(!s.applicationCursor && !s.bracketedPaste)
        s.feed(Data("\u{1b}[?1h\u{1b}[?2004h".utf8))
        #expect(s.applicationCursor && s.bracketedPaste)
    }

    @Test("a sample shell screen draws its prompt and output")
    func sample() {
        let s = TerminalSession(id: "t1", cols: 100, rows: 12)
        s.replace(cols: 100, rows: 12, with: Sample.shellScreen)
        #expect(s.lines()[0].hasPrefix("mingo@mbp ~/dev/anywhere (main) $ git status --short"))
    }

    /// Run in release (`swift test -c release --filter FeatureTerminalTests`) for a number worth reporting.
    @Test("10 MB of output goes through frame parse and the emulator")
    func throughput() throws {
        let log = Sample.terminalLog(bytes: 10 << 20)
        let frames = stride(from: 0, to: log.count, by: 32 << 10).map {
            TerminalFrame(kind: .output, terminalId: "t-bench", payload: log[$0..<min($0 + (32 << 10), log.count)]).encoded
        }
        let s = TerminalSession(id: "t-bench", cols: 120, rows: 40)
        let clock = ContinuousClock()
        var parsing = Duration.zero
        let start = clock.now
        for raw in frames {
            let parsed = clock.now
            let frame = try TerminalFrame(parsing: raw)
            parsing += clock.now - parsed
            s.feed(frame.payload)
        }
        let seconds = (clock.now - start) / .seconds(1)
        let parse = parsing / .seconds(1)
        print("terminal bench: 10 MiB in \(Int(seconds * 1000)) ms, \(Int(10 / seconds)) MiB/s; parse alone \(Int(parse * 1000)) ms, \(Int(10 / parse)) MiB/s")
        #expect(s.lines().contains { $0.contains("PASS") || $0.contains("FAIL") })
    }
}
