import DiffModel

/// Picked lines quoted into an agent's input, in the desktop's format (`add_to_chat.rs`).
public struct CodeQuote: Hashable, Sendable {
    public enum Body: Hashable, Sendable {
        case code(language: String)
        /// Numbered on the new side, unless the pick only removes lines (`removed`).
        case diff(removed: Bool)
    }

    public let lines: ClosedRange<Int>
    public let text: String
    public let body: Body

    /// Source lines `pick` of `lines`, numbered from 1; nil when they're all blank.
    public static func code(_ lines: [String], pick: ClosedRange<Int>, language: String) -> CodeQuote? {
        guard lines.indices.contains(pick.lowerBound), lines.indices.contains(pick.upperBound) else { return nil }
        let text = lines[pick].joined(separator: "\n")
        guard text.contains(where: { !$0.isWhitespace }) else { return nil }
        return CodeQuote(lines: pick.lowerBound + 1...pick.upperBound + 1, text: text, body: .code(language: language == "text" ? "" : language))
    }

    /// Diff rows `pick`, each with its sign, hunk headers left out.
    public static func diff(_ lines: [UnifiedLine], pick: ClosedRange<Int>) -> CodeQuote? {
        guard lines.indices.contains(pick.lowerBound), lines.indices.contains(pick.upperBound) else { return nil }
        let picked = lines[pick].filter { $0.kind != .hunk }
        let new = picked.compactMap(\.new)
        let removed = new.isEmpty
        let numbers = removed ? picked.compactMap(\.old) : new
        guard let lo = numbers.min(), let hi = numbers.max() else { return nil }
        let text = picked.map { sign($0.kind) + $0.text }.joined(separator: "\n")
        return CodeQuote(lines: lo...hi, text: text, body: .diff(removed: removed))
    }

    private static func sign(_ kind: UnifiedLine.Kind) -> String {
        switch kind {
        case .added: "+"
        case .deleted: "-"
        default: " "
        }
    }

    /// "Line 40" or "Lines 40–58".
    public var title: String {
        lines.count == 1 ? "Line \(lines.lowerBound)" : "Lines \(lines.lowerBound)–\(lines.upperBound)"
    }

    /// The text to put in the agent's input, naming `path`.
    public func payload(path: String) -> String {
        let at = lines.count == 1 ? "\(lines.lowerBound)" : "\(lines.lowerBound)-\(lines.upperBound)"
        switch body {
        case .code(let language): return "\(path):\(at)\n```\(language)\n\(text)\n```"
        case .diff(let removed): return "\(path):\(at)\(removed ? " (before)" : ""), uncommitted change\n```diff\n\(text)\n```"
        }
    }
}
