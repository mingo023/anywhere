/// A token's bytes, before they become UTF-16 offsets.
struct RawSpan {
    var start: Int
    var end: Int
    var kind: TokenKind
}

/// Up to 16 ASCII bytes of a word, packed, for keyword lookups without making a `String`.
struct Word: Hashable {
    let a: UInt64
    let b: UInt64

    init?(_ bytes: UnsafeBufferPointer<UInt8>, _ start: Int, _ end: Int) {
        guard end - start <= 16 else { return nil }
        var a: UInt64 = 0
        var b: UInt64 = 0
        for i in start..<end {
            let k = i - start
            if k < 8 { a |= UInt64(bytes[i]) << (8 * UInt64(k)) } else { b |= UInt64(bytes[i]) << (8 * UInt64(k - 8)) }
        }
        self.a = a
        self.b = b
    }

    init(_ s: String) {
        var bytes = Array(s.utf8)
        self = bytes.withUnsafeMutableBufferPointer { Word(UnsafeBufferPointer($0), 0, $0.count)! }
    }
}

/// A set of words, from a space-separated list.
struct Words {
    let set: Set<Word>

    init(_ list: String) { set = Set(list.split(separator: " ").map { Word(String($0)) }) }

    func contains(_ bytes: UnsafeBufferPointer<UInt8>, _ start: Int, _ end: Int) -> Bool {
        guard !set.isEmpty, let word = Word(bytes, start, end) else { return false }
        return set.contains(word)
    }
}

/// The bytes being lexed and the spans found so far. Every lexer reads `b` between `i` and `end` and calls `emit`.
struct Lexer {
    let b: UnsafeBufferPointer<UInt8>
    var i: Int
    var end: Int
    var out: [RawSpan] = []

    init(_ bytes: UnsafeBufferPointer<UInt8>, start: Int = 0, end: Int? = nil) {
        b = bytes
        i = start
        self.end = end ?? bytes.count
    }

    /// The byte at `k`, or 0 past the end.
    @inline(__always) func at(_ k: Int) -> UInt8 { k < end && k >= 0 ? b[k] : 0 }

    @inline(__always) func peek(_ offset: Int = 0) -> UInt8 { at(i + offset) }

    @inline(__always) mutating func emit(_ kind: TokenKind, _ start: Int, _ stop: Int) {
        // A token never reaches back over one already found.
        let start = max(start, out.last?.end ?? 0)
        guard kind != .plain, start < stop else { return }
        if let last = out.last, last.kind == kind, last.end == start {
            out[out.count - 1].end = stop
        } else {
            out.append(RawSpan(start: start, end: stop, kind: kind))
        }
    }

    /// Whether the bytes at `k` spell `s`.
    @inline(__always) func matches(_ s: StaticString, at k: Int) -> Bool {
        let n = s.utf8CodeUnitCount
        guard k + n <= end else { return false }
        let p = s.utf8Start
        for j in 0..<n where b[k + j] != p[j] { return false }
        return true
    }

    mutating func skipLine() {
        while i < end, b[i] != 0x0A { i += 1 }
    }

    /// A comment from `start` to `stop`, with any web address in it marked as a URL.
    mutating func comment(_ start: Int, _ stop: Int) {
        var k = start
        var from = start
        while k < stop {
            if b[k] == UInt8(ascii: "h"), matchesURL(at: k) {
                var e = k
                while e < stop, !isURLEnd(b[e]) { e += 1 }
                while e > k, ".,;:!?".utf8.contains(b[e - 1]) { e -= 1 }
                emit(.comment, from, k)
                emit(.url, k, e)
                k = e
                from = e
                continue
            }
            k += 1
        }
        emit(.comment, from, stop)
    }

    func matchesURL(at k: Int) -> Bool {
        matches("https://", at: k) || matches("http://", at: k)
    }

    @inline(__always) func isURLEnd(_ c: UInt8) -> Bool {
        c <= 0x20 || c == UInt8(ascii: ")") || c == UInt8(ascii: ">") || c == UInt8(ascii: "\"") || c == UInt8(ascii: "'") || c == UInt8(ascii: "`")
    }

    /// A number at `i`: decimal, hex, octal or binary, with underscores, a fraction, an exponent and a suffix.
    mutating func number() {
        let start = i
        if b[i] == UInt8(ascii: "0"), "xXoObB".utf8.contains(at(i + 1)) {
            i += 2
            while i < end, isIdent(b[i]) { i += 1 }
        } else {
            while i < end, isDigit(b[i]) || b[i] == UInt8(ascii: "_") { i += 1 }
            if at(i) == UInt8(ascii: "."), isDigit(at(i + 1)) {
                i += 1
                while i < end, isDigit(b[i]) || b[i] == UInt8(ascii: "_") { i += 1 }
            }
            if at(i) | 0x20 == UInt8(ascii: "e"), isDigit(at(i + 1)) || ((at(i + 1) == UInt8(ascii: "+") || at(i + 1) == UInt8(ascii: "-")) && isDigit(at(i + 2))) {
                i += 2
                while i < end, isDigit(b[i]) { i += 1 }
            }
            // Suffixes: 10u32, 1.5f, 10L, 10n.
            while i < end, isIdent(b[i]) { i += 1 }
        }
        emit(.number, start, i)
    }

    /// Steps over one UTF-8 scalar.
    @inline(__always) mutating func skipScalar() {
        i += 1
        while i < end, b[i] & 0xC0 == 0x80 { i += 1 }
    }
}

@inline(__always) func isDigit(_ c: UInt8) -> Bool { c &- 0x30 < 10 }
@inline(__always) func isLetter(_ c: UInt8) -> Bool { (c | 0x20) &- 0x61 < 26 }
@inline(__always) func isUpper(_ c: UInt8) -> Bool { c &- 0x41 < 26 }
@inline(__always) func isLower(_ c: UInt8) -> Bool { c &- 0x61 < 26 }
/// Letters, digits, `_`, `$` and any non-ASCII byte, so a word never ends inside a character.
@inline(__always) func isIdent(_ c: UInt8) -> Bool { isLetter(c) || isDigit(c) || c == 0x5F || c == 0x24 || c >= 0x80 }
@inline(__always) func isIdentStart(_ c: UInt8) -> Bool { isLetter(c) || c == 0x5F || c >= 0x80 }
@inline(__always) func isSpace(_ c: UInt8) -> Bool { c == 0x20 || c == 0x09 || c == 0x0A || c == 0x0D || c == 0x0C }

/// Byte spans as UTF-16 spans of the same text, in one pass.
func utf16Spans(_ raw: [RawSpan], _ bytes: UnsafeBufferPointer<UInt8>) -> [HighlightSpan] {
    var ascii = true
    for c in bytes where c >= 0x80 {
        ascii = false
        break
    }
    if ascii { return raw.map { HighlightSpan(range: $0.start..<$0.end, kind: $0.kind) } }
    var byte = 0
    var unit = 0
    func advance(to target: Int) -> Int {
        while byte < target {
            let c = bytes[byte]
            if c < 0x80 || (c >= 0xC0 && c < 0xF0) { unit += 1 } else if c >= 0xF0 { unit += 2 }
            byte += 1
        }
        return unit
    }
    var spans: [HighlightSpan] = []
    spans.reserveCapacity(raw.count)
    for span in raw {
        let start = advance(to: span.start)
        spans.append(HighlightSpan(range: start..<advance(to: span.end), kind: span.kind))
    }
    return spans
}
