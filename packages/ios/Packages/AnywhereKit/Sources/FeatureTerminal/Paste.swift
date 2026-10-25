import Foundation

/// Pastes as the desktop sends them (`crates/term/src/paste.rs`, libghostty's `paste.zig`).
public enum Paste {
    /// Bytes xterm turns into spaces in any paste, so one can't inject a command.
    static let stripped: Set<UInt8> = [0x00, 0x08, 0x05, 0x04, 0x1b, 0x7f, 0x03, 0x1c, 0x15, 0x1a, 0x11, 0x13, 0x17, 0x16, 0x12, 0x0f]
    static let start = Array("\u{1b}[200~".utf8)
    static let end = Array("\u{1b}[201~".utf8)

    /// Control bytes blanked, then wrapped in bracketed-paste markers or with newlines as returns.
    public static func bytes(_ text: String, bracketed: Bool) -> [UInt8] {
        let body = text.utf8.map { stripped.contains($0) ? UInt8(ascii: " ") : $0 }
        if bracketed { return start + body + end }
        return body.map { $0 == 0x0a ? 0x0d : $0 }
    }

    /// Whether `text` can't run anything by itself: no newline, and no marker that ends a bracketed paste.
    public static func isSafe(_ text: String) -> Bool {
        let b = Array(text.utf8)
        return !b.contains(0x0a) && !contains(b, end)
    }

    /// Text that could run commands asks first, unless the program brackets pastes or the user turned the question off.
    public static func asks(confirm: Bool, bracketed: Bool, _ text: String) -> Bool {
        confirm && !bracketed && !isSafe(text)
    }

    private static func contains(_ b: [UInt8], _ needle: [UInt8]) -> Bool {
        guard b.count >= needle.count else { return false }
        return (0...(b.count - needle.count)).contains { b[$0..<($0 + needle.count)].elementsEqual(needle) }
    }
}
