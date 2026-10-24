import Highlight

/// Source text cut into lines once, so a row costs only its own line.
public struct CodeLines: Sendable, Equatable {
    public let lines: [String]

    /// Splits on `\n`, `\r\n` and `\r`; a final newline doesn't make an empty last line.
    public init(_ text: String) {
        var out: [String] = []
        // Bytes, not Characters: grapheme breaking would cost most of the split.
        let bytes = text.utf8
        var start = bytes.startIndex
        var i = start
        while i != bytes.endIndex {
            let b = bytes[i]
            guard b == 10 || b == 13 else {
                i = bytes.index(after: i)
                continue
            }
            out.append(String(text[start..<i]))
            var next = bytes.index(after: i)
            if b == 13, next != bytes.endIndex, bytes[next] == 10 { next = bytes.index(after: next) }
            start = next
            i = next
        }
        if start != bytes.endIndex { out.append(String(text[start...])) }
        lines = out
    }

    public var count: Int { lines.count }

    /// Digits of the last line's number, so the gutter keeps one width.
    public var digits: Int { max(String(lines.count).count, 2) }
}

/// The highlighter runs over a window of lines at a time, off the main actor, so a long file colours only what is seen.
public enum HighlightWindow {
    public static let size = 200

    public static func window(of line: Int) -> Int { line / size }

    public static func range(_ window: Int, count: Int) -> Range<Int> {
        (window * size)..<min((window + 1) * size, count)
    }

    /// Highlights `lines` as one text and hands each line back its own spans, in UTF-16 offsets of that line.
    public static func spans(_ lines: ArraySlice<String>, language: String) -> [[HighlightSpan]] {
        guard language != "text", !lines.isEmpty else { return Array(repeating: [], count: lines.count) }
        return perLine(Highlighter.spans(lines.joined(separator: "\n"), language: language), lines: lines)
    }

    /// Cuts spans over lines joined by `\n` into each line's own, clipped at the line's ends.
    public static func perLine(_ spans: [HighlightSpan], lines: ArraySlice<String>) -> [[HighlightSpan]] {
        var out: [[HighlightSpan]] = Array(repeating: [], count: lines.count)
        var starts: [Int] = []
        starts.reserveCapacity(lines.count)
        var at = 0
        for l in lines {
            starts.append(at)
            at += l.utf16.count + 1
        }
        var i = 0
        for s in spans {
            while i + 1 < starts.count, starts[i + 1] <= s.range.lowerBound { i += 1 }
            var k = i
            while k < starts.count, starts[k] < s.range.upperBound {
                let end = starts[k] + lines[lines.startIndex + k].utf16.count
                let lo = max(s.range.lowerBound, starts[k])
                let hi = min(s.range.upperBound, end)
                if lo < hi { out[k].append(HighlightSpan(range: (lo - starts[k])..<(hi - starts[k]), kind: s.kind)) }
                k += 1
            }
        }
        return out
    }
}
