import Domain
import Foundation
import Testing
@testable import DesignSystem

@Suite("Design system")
struct DesignSystemTests {
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

    @Test("each status has its own colour")
    func statusColours() {
        let colours = Status.allCases.map { StatusGlyph.color($0) }
        #expect(Set(colours).count == Status.allCases.count)
    }

    @Test("the shimmer band sweeps from before the leading edge to past the trailing one")
    func shimmer() {
        let phases = stride(from: 0.0, to: 1.6, by: 0.05).map { Shimmer.phase(at: Date(timeIntervalSinceReferenceDate: $0)) }
        #expect(phases.min()! >= -0.2)
        #expect(phases.max()! <= 1.2)
        #expect(phases == phases.sorted())
    }
}
