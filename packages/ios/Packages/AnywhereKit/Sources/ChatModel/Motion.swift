/// Times here are seconds on one clock, as `Date.timeIntervalSinceReferenceDate`.
public typealias Seconds = Double

/// 1 - (1 - t)⁵.
public func easeOutQuint(_ t: Double) -> Double { 1 - pow5(1 - t) }
/// 1 - (1 - t)³.
public func easeOutCubic(_ t: Double) -> Double { 1 - (1 - t) * (1 - t) * (1 - t) }
private func pow5(_ x: Double) -> Double { x * x * x * x * x }

/// How much of a streaming message shows. What arrives is let out at an even pace,
/// so a whole paragraph landing at once still reads as being written.
public struct Reveal: Hashable, Sendable {
    /// Bytes a reply lets out each second, until a big chunk needs it faster.
    static let pace = 320
    /// However much arrives at once, it shows within this…
    static let quickest: Seconds = 0.3
    /// …and this.
    static let slowest: Seconds = 1.4
    /// How long a word takes to fade in once it shows.
    public static let fade: Seconds = 0.24

    var from: Int
    var to: Int
    var start: Seconds
    var took: Seconds

    /// A message `len` UTF-8 bytes long, all of it showing.
    public static func shown(_ len: Int, _ now: Seconds) -> Reveal {
        Reveal(from: len, to: len, start: now, took: 0)
    }

    /// The message is now `len` bytes: let out what's new from wherever the reveal is.
    public mutating func arrive(_ len: Int, _ now: Seconds) {
        if len <= to {
            if len < to { self = .shown(len, now) }
            return
        }
        let from = at(now)
        let took = min(max(Double((len - from) * 1000 / Self.pace) / 1000, Self.quickest), Self.slowest)
        self = Reveal(from: from, to: len, start: now, took: took)
    }

    func at(_ now: Seconds) -> Int {
        if took == 0 { return to }
        let t = min(max(now - start, 0) / took, 1)
        return from + Int(Double(to - from) * t)
    }

    /// How many bytes of `text` show at `now`, in whole words: a word shows once the reveal reaches it.
    /// `text` may be a block of the message starting `from` bytes in; the cut is then into the block.
    public func cut(_ text: String, from offset: Int = 0, _ now: Seconds) -> Int {
        let utf8 = Array(text.utf8)
        var at = min(max(self.at(now) - offset, 0), utf8.count)
        if at == 0 || at == utf8.count { return at }
        while at > 0 && utf8[at] & 0xC0 == 0x80 { at -= 1 }
        var offset = at
        for u in String(decoding: utf8[at...], as: UTF8.self).unicodeScalars {
            if u.properties.isWhitespace { return offset }
            offset += UTF8.width(u)
        }
        return utf8.count
    }

    /// Whether it's still letting text out, or fading in the last of it.
    public func moving(_ now: Seconds) -> Bool {
        now < start + took + Self.fade
    }
}

/// A scroll to the end, eased over 300 ms; past 2.5 viewports there's no glide and the list jumps.
public struct Glide: Hashable, Sendable {
    public static let duration: Seconds = 0.3
    var distance: Double
    var moved = 0.0

    public init?(distance: Double, viewport: Double) {
        guard distance <= viewport * 2.5 else { return nil }
        self.distance = distance
    }

    /// The points to scroll `elapsed` into the glide, and whether it has arrived.
    public mutating func step(_ elapsed: Seconds) -> (by: Double, done: Bool) {
        let t = min(elapsed / Self.duration, 1)
        let to = distance * easeOutQuint(t)
        let by = to - moved
        moved = to
        return (by, t >= 1)
    }
}

/// Once offered, the jump pill stays until the end is within 2 pt; one 320 pt threshold dropped it halfway through a scroll down.
public func jumpVisibility(wasShown: Bool, distance: Double) -> Bool {
    distance > (wasShown ? 2 : 320)
}

/// A sent prompt held a quarter down the chat while its reply grows under it, by room padded under the list's end.
/// The chat glides it there from the list's end, keeping the end of what came before in sight. Once the reply fills
/// what's under it the chat follows its tail again.
public struct Pin: Hashable, Sendable {
    /// The list's padding over and under its rows.
    public static let top: Double = 8
    public static let bottom: Double = 24
    /// How far down the viewport a sent prompt is held, under the end of what came before it.
    static let hold = 0.25
    /// How long the chat takes to glide a sent prompt to where it's held.
    public static let glide: Seconds = 0.52

    /// Until the prompt's row is measured.
    var sent = true
    /// Whether the list sits on the prompt rather than where the user scrolled.
    public var anchored = false
    /// What pads the list's end now.
    public var spacer = 0.0
    /// What holds the prompt where it's held, as last measured.
    var room = 0.0
    /// How far under the viewport's top the prompt is held.
    var held = Pin.top
    /// Where the prompt glides from, under the viewport's top, and since when.
    var gliding: (from: Double, start: Seconds)?
    /// Steers sent into the prompt's turn since, whose rows land under it.
    var steers = 0

    public init() {}

    public static func == (a: Pin, b: Pin) -> Bool {
        a.sent == b.sent && a.anchored == b.anchored && a.spacer == b.spacer && a.room == b.room && a.held == b.held
            && a.gliding?.from == b.gliding?.from && a.gliding?.start == b.gliding?.start && a.steers == b.steers
    }

    public func hash(into h: inout Hasher) {
        h.combine(spacer)
        h.combine(steers)
    }

    /// A steer went into the held prompt's turn: the prompt stays held and the steer shows under its reply.
    public mutating func steered() { steers += 1 }

    /// The row of the held prompt: the last sent but for the steers under it.
    public func row(_ rows: [Row]) -> Int? {
        let prompts = rows.indices.reversed().filter {
            switch rows[$0] {
            case .prompt, .pending: true
            default: false
            }
        }
        return steers < prompts.count ? prompts[steers] : nil
    }

    /// The user moved the list: it stays on the prompt only if they left it at the end, where the prompt sits.
    public mutating func moved(atEnd: Bool) {
        sent = false
        anchored = atEnd
        spacer = room
        gliding = nil
    }

    /// Whether the prompt's row is still to be measured: until then it shows at the list's end, not yet held.
    public var unmeasured: Bool { sent }

    /// The height of the prompt and what follows it, for `step`: as measured now, else, until first held, as last laid
    /// out once the prompt's own row was. A reply streaming in under it is remeasured on every update, so waiting for all
    /// of it to measure at once would hold the prompt only once the reply stops.
    public func below(measured: Double?, laidOut: Double?) -> Double? {
        measured ?? (sent ? laidOut : nil)
    }

    /// Sizes the spacer from the height of the rows `above` the prompt and of the prompt and what follows it, nil while
    /// any of it is unmeasured. Until it's measured the spacer fills what's under the prompt but for a point.
    /// Once first measured, the prompt glides from the list's end unless `glide` is nil.
    /// Returns false once the reply fills what's under the prompt and the pin lets go.
    public mutating func step(viewport: Double, above: Double, below: Double?, glide: Seconds?) -> Bool {
        held = min(viewport * Self.hold, Self.top + above)
        if let below {
            room = viewport - held - Self.bottom - below
            if room <= 0 { return false }
            if sent {
                sent = false
                anchored = true
                gliding = glide.map { (viewport - Self.bottom - below, $0) }
            }
        }
        if !sent { spacer = below != nil ? room : viewport - held - Self.bottom - 1 }
        return true
    }

    /// How far under the viewport's top the prompt shows at `now`, and whether it's still gliding there.
    public mutating func top(_ now: Seconds) -> (Double, Bool) {
        guard let (from, start) = gliding else { return (held, false) }
        let t = min(max(now - start, 0) / Self.glide, 1)
        if t >= 1 { gliding = nil }
        return (from + (held - from) * easeOutCubic(t), t < 1)
    }
}
