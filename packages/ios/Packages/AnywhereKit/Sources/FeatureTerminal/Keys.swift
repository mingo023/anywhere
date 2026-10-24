import Foundation

/// A key the key bar sends, besides the keyboard's own.
public enum TerminalKey: Hashable, Sendable {
    case escape, tab, up, down, left, right, home, end, pageUp, pageDown
    case text(String)
}

public struct KeyMods: OptionSet, Hashable, Sendable {
    public let rawValue: Int
    public init(rawValue: Int) { self.rawValue = rawValue }
    public static let ctrl = KeyMods(rawValue: 1)
    public static let alt = KeyMods(rawValue: 2)
}

/// What a key sends to the program, as xterm encodes it.
public enum KeyEncoder {
    /// `applicationCursor` is DECCKM: arrows then send `ESC O` instead of `ESC [`.
    public static func bytes(_ key: TerminalKey, mods: KeyMods = [], applicationCursor: Bool = false) -> [UInt8] {
        let esc: UInt8 = 0x1b
        // xterm's modifier parameter: 1 + alt 2 + ctrl 4.
        let param = 1 + (mods.contains(.alt) ? 2 : 0) + (mods.contains(.ctrl) ? 4 : 0)
        func csi(_ final: Character) -> [UInt8] {
            if param > 1 { return Array("\u{1b}[1;\(param)\(final)".utf8) }
            return applicationCursor ? [esc, UInt8(ascii: "O"), final.asciiValue!] : [esc, UInt8(ascii: "["), final.asciiValue!]
        }
        func tilde(_ n: Int) -> [UInt8] { Array((param > 1 ? "\u{1b}[\(n);\(param)~" : "\u{1b}[\(n)~").utf8) }
        let alt: [UInt8] = mods.contains(.alt) ? [esc] : []
        switch key {
        case .escape: return alt + [esc]
        case .tab: return alt + [0x09]
        case .up: return csi("A")
        case .down: return csi("B")
        case .right: return csi("C")
        case .left: return csi("D")
        case .home: return csi("H")
        case .end: return csi("F")
        case .pageUp: return tilde(5)
        case .pageDown: return tilde(6)
        case .text(let s):
            guard mods.contains(.ctrl) else { return alt + Array(s.utf8) }
            return alt + s.flatMap { c in control(c).map { [$0] } ?? Array(String(c).utf8) }
        }
    }

    /// The control byte Ctrl and `c` make, if they make one.
    static func control(_ c: Character) -> UInt8? {
        guard let a = c.asciiValue else { return nil }
        switch a {
        case UInt8(ascii: "a")...UInt8(ascii: "z"): return a - 0x60
        case UInt8(ascii: "@")...UInt8(ascii: "_"): return a - 0x40
        case UInt8(ascii: " "), UInt8(ascii: "2"): return 0
        case UInt8(ascii: "3")...UInt8(ascii: "7"): return a - UInt8(ascii: "3") + 0x1b
        case UInt8(ascii: "8"), UInt8(ascii: "?"): return 0x7f
        case UInt8(ascii: "/"), UInt8(ascii: "-"): return 0x1f
        default: return nil
        }
    }
}

/// The key bar's sticky Ctrl and Alt: tapped, they apply to the next key, from the bar or the keyboard, then let go.
public struct StickyMods: Hashable, Sendable {
    public var held: KeyMods = []

    public init() {}

    public mutating func toggle(_ mod: KeyMods) {
        if held.contains(mod) { held.remove(mod) } else { held.insert(mod) }
    }

    public mutating func press(_ key: TerminalKey, applicationCursor: Bool) -> [UInt8] {
        defer { held = [] }
        return KeyEncoder.bytes(key, mods: held, applicationCursor: applicationCursor)
    }

    /// What the keyboard typed, with the held modifiers applied.
    public mutating func typed(_ bytes: [UInt8]) -> [UInt8] {
        guard !held.isEmpty, let text = String(bytes: bytes, encoding: .utf8) else { return bytes }
        return press(.text(text), applicationCursor: false)
    }
}
