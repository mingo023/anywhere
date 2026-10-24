import Foundation

/// The prompt floated over the top of the chat once its turn scrolls past it, as a pill.
public enum Sticky {
    /// The pill's height while folded to one line.
    public static let height: Double = 34
    public static let line: Double = 20
    /// The arrow and the chevron, inset from the pill's ends as far as from its top.
    public static let knob: Double = 22
    /// An opened pill's padding over and under its text.
    public static let openTop: Double = 6
    public static let openBottom: Double = 8
    /// Past this an opened prompt scrolls.
    public static let openMaxHeight: Double = 160
    /// How far a card is pushed before its shadow is gone.
    static let settle: Double = 12
    public static let marginTop: Double = 10
    /// A prompt's first images, before its text; the rest are counted as +N.
    public static let thumbs = 2

    /// The prompt row whose turn the top of the list is in, once that prompt has scrolled up past the top: none while it
    /// shows from its top, above the first prompt, or under a send not yet echoed, which ends the turn before it.
    /// `top` is the row at the list's top and `offset` how far into it.
    public static func stuck(_ rows: [Row], top: Int, offset: Double) -> Int? {
        guard !rows.isEmpty else { return nil }
        let last = min(top, rows.count - 1)
        guard let ix = (0...last).reversed().first(where: { isPrompt(rows[$0]) }), case .prompt = rows[ix] else { return nil }
        return ix < top || offset > 0 ? ix : nil
    }

    /// The row at the top of a list scrolled to its end, and how far into it, from each row's height to the end and the
    /// height its rows show in. Rows too few to fill it show from the first.
    public static func topAtEnd(_ count: Int, shown: Double, from: (Int) -> Double) -> (Int, Double) {
        for ix in (0..<count).reversed() {
            let height = from(ix)
            if height >= shown { return (ix, height - shown) }
        }
        return (0, 0)
    }

    /// The next prompt under the list's top, which pushes the card up as it arrives.
    public static func nextPrompt(_ rows: [Row], top: Int) -> Int? {
        rows.indices.first { $0 > top && isPrompt(rows[$0]) }
    }

    private static func isPrompt(_ r: Row) -> Bool {
        switch r {
        case .prompt, .pending: true
        default: false
        }
    }

    /// The card's height for a prompt that wraps to `lines` lines when opened.
    public static func height(lines: Int, open: Bool) -> Double {
        guard open else { return height }
        // The text sits 1 pt down, so its first line is level with the arrow.
        return openTop + max(1 + min(Double(lines) * line, openMaxHeight), knob) + openBottom
    }

    /// A prompt folded onto one line, its line breaks read as spaces.
    public static func oneLine(_ text: String) -> String {
        lines(text).map { $0.trimmingWhitespace() }.filter { !$0.isEmpty }.joined(separator: " ")
    }

    /// What a prompt with no text shows.
    public static func imageCount(_ n: Int) -> String {
        "\(n) image\(n == 1 ? "" : "s")"
    }

    /// How far the card moves up so the next prompt, `next` pt under the list's top, pushes it out rather than slides
    /// under it; `bottom` is where the card ends.
    public static func pushed(bottom: Double, next: Double?) -> Double {
        next.map { min($0 - bottom, 0) } ?? 0
    }

    /// How much of its shadow a card keeps when pushed `shift` pt up: it settles onto the next bubble over the first
    /// 12 pt, so the shadow doesn't darken that bubble.
    public static func lift(_ shift: Double) -> Double {
        min(max(1 + shift / settle, 0), 1)
    }
}

/// `text`'s lines as Rust's `str::lines` reads them: split at `\n`, a trailing `\r` dropped, no empty last line.
func lines(_ text: String) -> [Substring] {
    var out = text.split(separator: "\n", omittingEmptySubsequences: false)
    if out.last == "" { out.removeLast() }
    return out.map { $0.hasSuffix("\r") ? $0.dropLast() : $0 }
}

private let imagesHeading = "# Attached images\n\n"

/// The prompt goes to the agent as text, so images go in as paths it can read.
public func prompt(_ typed: String, images: [String]) -> String {
    let typed = typed.trimmingWhitespace()
    if images.isEmpty { return typed }
    let block = imagesHeading + images.map { "- \($0)" }.joined(separator: "\n")
    return typed.isEmpty ? block : "\(typed)\n\n\(block)"
}

/// Undoes `prompt(_:images:)`: the typed text and the attached image paths.
public func splitImages(_ prompt: String) -> (text: String, images: [String]) {
    let end: String.Index
    let block: String.Index
    if let r = prompt.range(of: "\n\n" + imagesHeading, options: .backwards) {
        end = r.lowerBound
        block = prompt.index(r.lowerBound, offsetBy: 2)
    } else if prompt.hasPrefix(imagesHeading) {
        (end, block) = (prompt.startIndex, prompt.startIndex)
    } else {
        return (prompt, [])
    }
    let rest = String(prompt[block...].dropFirst(imagesHeading.count))
    var paths: [String] = []
    for line in lines(rest) {
        guard line.hasPrefix("- ") else { return (prompt, []) }
        paths.append(String(line.dropFirst(2)))
    }
    return paths.isEmpty ? (prompt, []) : (String(prompt[..<end]), paths)
}
