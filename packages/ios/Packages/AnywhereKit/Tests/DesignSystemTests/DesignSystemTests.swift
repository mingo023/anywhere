import Domain
import Foundation
import Testing
@testable import DesignSystem

@Suite("Design system")
struct DesignSystemTests {
    @Test("a brand mark's path fills its 24-point box")
    func marks() {
        for mark in [Mark.claude, Mark.openai] {
            let box = mark.boundingRect
            #expect(box.minX > -0.5 && box.minY > -0.5 && box.maxX < 24.5 && box.maxY < 24.5)
            #expect(box.width > 20 && box.height > 20)
        }
    }

    @Test("a file badge names its language", arguments: [
        ("src/chat.rs", "RS", 0xC2410C), ("App.TSX", "TS", 0x3178C6), ("Cargo.toml", "TM", 0x9C4221), ("Makefile", "", 0x8B8B94),
        ("Sources/Wire/Caps.swift", "SW", 0xF05138), ("README.md", "MD", 0x3E8EF7), ("package.json", "JS", 0x8B8B94),
    ] as [(String, String, UInt32)])
    func badges(_ path: String, _ letters: String, _ rgb: UInt32) {
        let badge = LanguageBadge(path: path)
        #expect(badge.letters == letters)
        #expect(badge.rgb == rgb)
    }

    @Test("a file chip shows the base name")
    func chipName() {
        #expect(FileChip.name("packages/ios/App/AnywhereApp.swift") == "AnywhereApp.swift")
        #expect(FileChip.name("Makefile") == "Makefile")
        #expect(FileChip.name("dir/") == "dir")
    }

    @Test("a token's channels read from 0xRRGGBBAA, per appearance")
    func tokens() {
        let (r, g, b, a) = Token.components(0xff800040)
        #expect(r == 1 && g == 128.0 / 255 && b == 0 && a == 64.0 / 255)
        #expect(Palette.text.rgba(dark: false) == 0x111113ff)
        #expect(Palette.text.rgba(dark: true) == 0xebebebff)
        #expect(Palette.waitingDot.rgba(dark: false) == Palette.waitingDot.rgba(dark: true))
    }

    @Test("the spinner's dots pulse as the desktop's do")
    func spinner() {
        #expect(GradientSpinner.opacity(0) == 1)
        #expect(GradientSpinner.opacity(0.45) == GradientSpinner.dim)
        #expect(GradientSpinner.opacity(0.7) == GradientSpinner.dim)
        #expect(GradientSpinner.opacity(1.25) == GradientSpinner.opacity(0.25))
        #expect(GradientSpinner.lead(row: 2, col: 1) == 0)
        #expect(GradientSpinner.lead(row: 0, col: 0) == 0.75)
    }

    @Test("a cell clear of the content shows a dot, and under the lens a glyph of the map")
    func fieldMarks() {
        #expect(Field.mark(level: 1, glow: 1, mask: 0) == .hidden)
        #expect(Field.mark(level: 0.5, glow: 0, mask: 1) == .dot(alpha: 0.35 + 0.5 * 0.65))
        guard case .glyph(let index, _, let alpha) = Field.mark(level: 1, glow: 1, mask: 1) else {
            Issue.record("no glyph")
            return
        }
        #expect(index == Field.ramp.count - 1)
        #expect(alpha == 1)
    }

    @MainActor @Test("the map fades around the content and follows it, keeping its contour")
    func fieldMasks() {
        let map = FieldMap()
        let top = map.sized(20, 10, around: [CGRect(x: 0, y: 0, width: 200, height: 20)])
        let bottom = map.sized(20, 10, around: [CGRect(x: 0, y: 160, width: 200, height: 20)])
        #expect(top.levels == bottom.levels)
        #expect(top.masks[0] == 0)
        #expect(bottom.masks[0] == 1)
    }

    @Test("an idle lens starts at rest, crosses the field and stays in its band")
    func fieldWander() {
        let rest = CGPoint(x: 200, y: 120)
        #expect(Field.wander(0, around: rest) == rest)
        let path = stride(from: 0.0, to: 187, by: 0.5).map { Field.wander($0, around: rest) }
        #expect(path.allSatisfy { abs($0.x - 200) <= 160 && abs($0.y - 120) <= 36 })
        #expect(path.map(\.x).max()! > 340 && path.map(\.x).min()! < 60)
    }

    @Test("a drifting lens stays lit and keeps up with its path")
    func fieldLensDrifts() {
        let rest = CGPoint(x: 200, y: 120)
        var lens = FieldLens()
        lens.aim(rest)
        for _ in 0..<200 {
            lens.wander(0.05, around: rest)
            lens.step(0.05)
        }
        #expect(abs(lens.x - lens.aimX) < 5 && abs(lens.y - lens.aimY) < 5)
        #expect(lens.glow(CGPoint(x: lens.x, y: lens.y)) > 0.99)
    }

    @Test("the map's noise stays within 0 and 1, and a level never moves")
    func fieldNoise() {
        for i in 0..<2000 {
            let n = Field.noise(Double(i) * 0.37 - 300, Double(i) * 0.91 - 700)
            #expect(n >= 0 && n <= 1)
        }
        #expect(Field.level(12, 40) == Field.level(12, 40))
    }

    @Test("the field stays clear of content, then fades in")
    func fieldMask() {
        let rect = CGRect(x: 100, y: 100, width: 50, height: 20)
        #expect(Field.outside(rect, CGPoint(x: 120, y: 110)) == 0)
        #expect(Field.outside(rect, CGPoint(x: 160, y: 110)) == 10)
        #expect(Field.mask(0) == 0)
        #expect(Field.mask(Field.margin + Field.fade) == 1)
    }

    @Test("the lens eases to where it's aimed, and stops asking for frames there")
    func fieldLens() {
        var lens = FieldLens()
        lens.aim(CGPoint(x: 100, y: 50))
        #expect(lens.x == 100 && lens.moving)
        for _ in 0..<200 { lens.step(0.016) }
        #expect(!lens.moving)
        #expect(lens.glow(CGPoint(x: 100, y: 50)) > 0.99)
        #expect(lens.glow(CGPoint(x: 100, y: 50 + FieldLens.radius)) == 0)
    }

    @Test("each status has its own colour")
    func statusColours() {
        let colours = Status.allCases.map { StatusGlyph.color($0) }
        #expect(Set(colours).count == Status.allCases.count)
    }

    @Test("an offline Mac's strip says when its data is from, or that the link is reconnecting")
    func staleStrip() {
        let utc = TimeZone(identifier: "UTC")!
        #expect(StaleStrip.text(mac: "Studio", asOf: 1_762_512_120_000, timeZone: utc) == "Studio is offline · as of 10:42")
        #expect(StaleStrip.text(mac: "Studio", asOf: nil) == "Studio is offline · reconnecting…")
    }

    @Test("a toast stays 2.4 seconds unless it waits on work in flight")
    func toastLasts() {
        #expect(Toast("Copied path").lasts == .milliseconds(2400))
        #expect(Toast("Deleting calm-otter…", lasts: nil).lasts == nil)
        #expect(Toast("Copied").id != Toast("Copied").id)
    }

    @Test("the shimmer band sweeps from before the leading edge to past the trailing one")
    func shimmer() {
        let phases = stride(from: 0.0, to: 1.6, by: 0.05).map { Shimmer.phase(at: Date(timeIntervalSinceReferenceDate: $0)) }
        #expect(phases.min()! >= -0.2)
        #expect(phases.max()! <= 1.2)
        #expect(phases == phases.sorted())
    }

    @Test("a toast about something sent to a chat goes to it only where the screen can open it")
    @MainActor
    func sentToast() {
        let chat = Route.chat(mac: "m", agentId: "a1")
        #expect(Toast.sent("Added", chat: chat, open: nil).actionTitle == nil)
        var opened: Route?
        let toast = Toast.sent("Added", chat: chat) { opened = $0 }
        #expect(toast.text == "Added" && toast.actionTitle == "Go to chat")
        toast.action?()
        #expect(opened == chat)
    }
}
