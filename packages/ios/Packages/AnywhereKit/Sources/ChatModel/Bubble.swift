/// The user's prompt as its bubble shows it.
public enum Bubble {
    public static let collapsedLines = 5
    public static let lineHeight: Double = 22
    public static let textSize: Double = 14.5
    /// A bubble's widest, of the reading width.
    public static let width = 0.78

    /// A run of a prompt's text, and how it shows.
    public struct Segment: Hashable, Sendable {
        public enum Kind: Hashable, Sendable {
            case plain
            /// A `/command` or `/skill`, tinted.
            case command
            /// An `@path`, shown as its icon and name; `path` is what it names.
            case mention(path: String)
        }

        public var text: String
        public var kind: Kind

        public init(_ text: String, _ kind: Kind) {
            self.text = text
            self.kind = kind
        }
    }

    /// `text` with its commands tinted and each mention shown by its name.
    public static func segments(_ text: String) -> [Segment] {
        let m = marks(text)
        let bytes = Array(text.utf8)
        let marked = (m.commands.map { ($0, false) } + m.mentions.map { ($0, true) }).sorted { $0.0.lowerBound < $1.0.lowerBound }
        func slice(_ r: Range<Int>) -> String { String(decoding: bytes[r], as: UTF8.self) }
        var out: [Segment] = []
        var at = 0
        for (r, mention) in marked {
            if r.lowerBound > at { out.append(Segment(slice(at..<r.lowerBound), .plain)) }
            if mention {
                let path = slice(r.lowerBound + 1..<r.upperBound)
                out.append(Segment(mentionName(path), .mention(path: path)))
            } else {
                out.append(Segment(slice(r), .command))
            }
            at = r.upperBound
        }
        if at < bytes.count { out.append(Segment(slice(at..<bytes.count), .plain)) }
        return out
    }

    public static func needsCollapse(_ lines: Int) -> Bool { lines > collapsedLines }

    /// Growing or shrinking by `delta` pt takes longer the further it goes, up to 850 ms.
    public static func tween(_ delta: Double) -> Seconds {
        Double(Int(min(220 + delta * 0.32, 850))) / 1000
    }

    /// Under the last prompt the agent hasn't taken: when it will.
    public static func waiting(_ who: String, _ count: Int) -> String {
        count > 1 ? "\(who) reads these \(count) after its current step" : "\(who) reads this after its current step"
    }

    public static let notDelivered = "Not delivered — tap to retry"
}
