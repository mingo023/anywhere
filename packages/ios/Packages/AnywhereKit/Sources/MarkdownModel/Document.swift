import Foundation

/// One block of a document, keyed for lists: `id` is its place, and two blocks are equal when they hold the same
/// content in the same place, so comparing them costs nothing however big they are.
public struct MarkdownBlock: Identifiable, Sendable, Equatable {
    public let id: Int
    public let block: MDBlock
    let digest: Int

    init(id: Int, block: MDBlock) {
        self.id = id
        self.block = block
        digest = block.hashValue
    }

    public static func == (a: MarkdownBlock, b: MarkdownBlock) -> Bool { a.id == b.id && a.digest == b.digest }
}

/// A parsed markdown string.
public struct MarkdownDocument: Sendable, Equatable {
    public let source: String
    public let blocks: [MarkdownBlock]
    /// The first block of the last top-level node, and the UTF-8 offset of the line it starts on: what a longer
    /// version of the source re-parses from.
    let tailBlock: Int
    let tailOffset: Int
    /// Holds link reference definitions or footnotes, which reach across blocks, so growth re-parses it whole.
    let global: Bool

    public static func == (a: MarkdownDocument, b: MarkdownDocument) -> Bool { a.blocks == b.blocks }
}

public enum Markdown {
    public static func parse(_ source: String) -> MarkdownDocument {
        assemble(source, kept: [], from: 0, parsed: Parser.parse(Substring(source)), global: isGlobal(source.utf8[...]))
    }

    /// Parses `source`, a longer version of `previous.source`, re-parsing only from `previous`'s last top-level block
    /// so the blocks before it keep their identity. CommonMark never changes a block once another follows it, except
    /// through reference definitions and footnotes, which take a whole parse.
    public static func parse(_ source: String, growing previous: MarkdownDocument) -> MarkdownDocument {
        guard !previous.global, hasPrefix(source, previous.source) else { return parse(source) }
        let start = source.utf8.index(source.utf8.startIndex, offsetBy: previous.tailOffset)
        let tail = source.utf8[start...]
        guard !isGlobal(tail) else { return parse(source) }
        return assemble(source, kept: Array(previous.blocks.prefix(previous.tailBlock)), from: previous.tailOffset, parsed: Parser.parse(Substring(tail)), global: false)
    }

    static func assemble(_ source: String, kept: [MarkdownBlock], from offset: Int, parsed: (top: [(blocks: [MDBlock], line: Int)], footnotes: MDBlock?), global: Bool) -> MarkdownDocument {
        let utf8 = source.utf8
        let chunk = utf8[utf8.index(utf8.startIndex, offsetBy: offset)...]
        // A node that starts on the unfinished last line may yet join the one before it (a table's next row is a
        // paragraph until its line holds a cell), so a longer text re-parses from the node before.
        let lastLine = lineOffsets(chunk).count - 1
        let unsettled = parsed.top.count > 1 && parsed.top.last!.line >= lastLine
        let tailNode = max(0, parsed.top.count - (unsettled ? 2 : 1))
        var blocks = kept
        var tailBlock = kept.count
        var tailLine = 0
        for (i, node) in parsed.top.enumerated() {
            if i == tailNode {
                tailBlock = blocks.count
                tailLine = node.line
            }
            for block in node.blocks { blocks.append(MarkdownBlock(id: blocks.count, block: block)) }
        }
        if let footnotes = parsed.footnotes { blocks.append(MarkdownBlock(id: blocks.count, block: footnotes)) }
        let tailOffset = offset + (tailLine == 0 ? 0 : lineOffsets(chunk)[tailLine])
        return MarkdownDocument(source: source, blocks: blocks, tailBlock: tailBlock, tailOffset: tailOffset, global: global)
    }

    /// The UTF-8 offset each line starts at, counting "\n", "\r\n" and a lone "\r" as line endings as cmark does.
    static func lineOffsets(_ text: Substring.UTF8View) -> [Int] {
        var starts = [0]
        var offset = 0
        var previousCR = false
        for byte in text {
            offset += 1
            if byte == 0x0A {
                if previousCR { starts[starts.count - 1] = offset } else { starts.append(offset) }
            } else if byte == 0x0D {
                starts.append(offset)
            }
            previousCR = byte == 0x0D
        }
        return starts
    }

    /// Whether `text` might hold a reference definition (`[x]: …`) or a footnote (`[^x]`).
    static func isGlobal(_ text: Substring.UTF8View) -> Bool {
        var previous: UInt8 = 0
        for byte in text {
            if previous == UInt8(ascii: "]"), byte == UInt8(ascii: ":") { return true }
            if previous == UInt8(ascii: "["), byte == UInt8(ascii: "^") { return true }
            previous = byte
        }
        return false
    }

    /// Whether `text`'s bytes start with `prefix`'s, compared a word at a time.
    static func hasPrefix(_ text: String, _ prefix: String) -> Bool {
        guard text.utf8.count >= prefix.utf8.count else { return false }
        let fast = text.utf8.withContiguousStorageIfAvailable { a in
            prefix.utf8.withContiguousStorageIfAvailable { b in
                b.isEmpty || memcmp(a.baseAddress!, b.baseAddress!, b.count) == 0
            }
        }
        return (fast ?? nil) ?? text.utf8.starts(with: prefix.utf8)
    }

    /// A 64-bit FNV-1a digest of the bytes, stable across runs, unlike `hashValue`.
    public static func digest(_ text: String) -> UInt64 {
        var hash: UInt64 = 0xcbf2_9ce4_8422_2325
        for byte in text.utf8 {
            hash ^= UInt64(byte)
            hash = hash &* 0x100_0000_01b3
        }
        return hash
    }
}

/// Parsed documents by content, shared by every view: a string seen before costs a lookup, and one that grew from a
/// string seen lately re-parses only its last block.
public final class MarkdownCache: @unchecked Sendable {
    public static let shared = MarkdownCache()

    private let lock = NSLock()
    private var documents: [UInt64: MarkdownDocument] = [:]
    private var order: [UInt64] = []
    private var recent: [MarkdownDocument] = []
    private let limit: Int

    public init(limit: Int = 512) { self.limit = limit }

    /// The document for `source` if it was parsed before.
    public func cached(_ source: String, digest: UInt64? = nil) -> MarkdownDocument? {
        let key = digest ?? Markdown.digest(source)
        return lock.withLock { documents[key].flatMap { $0.source == source ? $0 : nil } }
    }

    /// The document for `source`, parsing it if needed. Pure and thread-safe; call it off the main actor for long text.
    public func document(_ source: String) -> MarkdownDocument {
        let key = Markdown.digest(source)
        if let hit = cached(source, digest: key) { return hit }
        return parse(source, key, base: base(for: source))
    }

    /// The document for `source` if it's cached or cheap to parse now: at most `budget` bytes, or that much grown
    /// from a string seen lately. nil means parse it off the main actor with `document(_:)`.
    public func documentIfCheap(_ source: String, budget: Int = 16 * 1024) -> MarkdownDocument? {
        let key = Markdown.digest(source)
        if let hit = cached(source, digest: key) { return hit }
        let base = base(for: source)
        guard source.utf8.count <= budget || base.map({ source.utf8.count - $0.tailOffset <= budget }) == true else { return nil }
        return parse(source, key, base: base)
    }

    private func base(for source: String) -> MarkdownDocument? {
        lock.withLock { recent.last { $0.source.utf8.count < source.utf8.count && Markdown.hasPrefix(source, $0.source) } }
    }

    private func parse(_ source: String, _ key: UInt64, base: MarkdownDocument?) -> MarkdownDocument {
        let document = base.map { Markdown.parse(source, growing: $0) } ?? Markdown.parse(source)
        lock.withLock {
            if documents.updateValue(document, forKey: key) == nil { order.append(key) }
            if order.count > limit { documents[order.removeFirst()] = nil }
            recent.removeAll { Markdown.hasPrefix(source, $0.source) }
            recent.append(document)
            if recent.count > 16 { recent.removeFirst() }
        }
        return document
    }
}
