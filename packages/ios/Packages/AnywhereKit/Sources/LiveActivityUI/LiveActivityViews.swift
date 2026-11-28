import ActivityModel
import DesignSystem
import SwiftUI

/// Everything a Live Activity region needs: the widget fills it from its ActivityKit context, the gallery from fixtures.
public struct ActivitySnapshot: Hashable, Sendable {
    public var attributes: MacActivity
    public var state: MacActivity.ContentState
    /// The Mac stopped pushing past the stale date: asleep or offline.
    public var isStale: Bool
    /// The top session's title and place, from the App Group cache.
    public var entry: ActivityTitles.Entry

    public init(attributes: MacActivity, state: MacActivity.ContentState, isStale: Bool, entry: ActivityTitles.Entry) {
        self.attributes = attributes
        self.state = state
        self.isStale = isStale
        self.entry = entry
    }

    public var link: URL { ActivityDisplay.link(mac: attributes.mac, top: state.top) }

    /// Allow and Deny show for a plain ask while the Mac is reachable.
    public var answer: (mac: String, request: String)? {
        guard !isStale, let top = state.top, top.answerable, let request = top.request else { return nil }
        return (attributes.mac, request)
    }
}

extension Color {
    /// One appearance of a token: the Dynamic Island is always dark.
    static func of(_ token: Token, dark: Bool) -> Color {
        let (r, g, b, a) = Token.components(token.rgba(dark: dark))
        return Color(.sRGB, red: r, green: g, blue: b, opacity: a)
    }
}

/// What the activity's words say, as plain values the views and their tests share.
enum ActivityText {
    /// The ask as the Lock Screen quotes it: a command alone, any other ask led by its verb.
    static func command(_ top: MacActivity.Top) -> String? {
        guard let subject = top.subject, !subject.isEmpty else { return nil }
        guard let verb = top.verb, !verb.isEmpty, verb.lowercased() != "run" else { return subject }
        return "\(verb.lowercased()) \(subject)"
    }

    /// A place "project · worktree" split so the worktree can be set in mono.
    static func place(_ place: String) -> (project: String, worktree: String?) {
        let parts = place.components(separatedBy: " · ")
        return parts.count == 2 ? (parts[0], parts[1]) : (place, nil)
    }

    /// The Mac's name, with how many more sessions want a look.
    static func header(_ s: ActivitySnapshot) -> String {
        [s.attributes.macName, s.isStale ? nil : ActivityDisplay.more(s.state)].compactMap(\.self).joined(separator: " · ")
    }
}

/// A place in the caption's weight, its worktree in mono.
private struct PlaceLine: View {
    let place: String
    let color: Color

    var body: some View {
        let parts = ActivityText.place(place)
        HStack(spacing: 0) {
            Text(parts.worktree == nil ? parts.project : "\(parts.project) · ")
            if let worktree = parts.worktree { Text(worktree).font(.system(size: 12.5, design: .monospaced)) }
        }
        .font(.system(size: 13))
        .foregroundStyle(color)
        .lineLimit(1)
    }
}

/// The dot a state leads with, haloed: amber when it needs you, orange working, red failed, green done.
@MainActor
public struct ActivityDot: View {
    let glyph: ActivityDisplay.Glyph
    let size: CGFloat
    let dark: Bool

    public init(_ glyph: ActivityDisplay.Glyph, size: CGFloat = 7, dark: Bool = true) {
        self.glyph = glyph
        self.size = size
        self.dark = dark
    }

    public var body: some View {
        let tint = Color.of(Self.tint(glyph), dark: dark)
        Circle()
            .fill(tint)
            .frame(width: size, height: size)
            .padding(size * 0.45)
            .background(tint.opacity(0.18), in: Circle())
            .accessibilityLabel(Self.label(glyph))
    }

    nonisolated static func tint(_ g: ActivityDisplay.Glyph) -> Token {
        switch g {
        case .needsYou: Palette.waitingDot
        case .failed: Palette.failed
        case .working: Palette.workingDot
        case .done: Palette.success
        case .idle: Palette.text4
        }
    }

    nonisolated static func label(_ g: ActivityDisplay.Glyph) -> String {
        switch g {
        case .needsYou: "Needs you"
        case .failed: "Failed"
        case .working: "Working"
        case .done: "Done"
        case .idle: "Nothing running"
        }
    }
}

/// The compact trailing region: the state's dot and how long it has waited, or how many are done.
@MainActor
public struct ActivityTrailing: View {
    /// What follows the dot.
    enum Reading: Hashable {
        case elapsed(since: Date)
        case count(Int)
        case none
    }

    let s: ActivitySnapshot
    let dark: Bool

    public init(_ snapshot: ActivitySnapshot, dark: Bool = true) {
        s = snapshot
        self.dark = dark
    }

    nonisolated static func reading(_ state: MacActivity.ContentState) -> Reading {
        switch ActivityDisplay.trailing(state) {
        case .needs(let n): state.top.map { .elapsed(since: $0.since) } ?? .count(n)
        case .timer(let since): .elapsed(since: since)
        case .unseen(let n): .count(n)
        case .none: .none
        }
    }

    public var body: some View {
        HStack(spacing: 6) {
            ActivityDot(ActivityDisplay.glyph(s.state), dark: dark)
            switch Self.reading(s.state) {
            case .elapsed(let since):
                Text(timerInterval: since...Date.distantFuture, countsDown: false)
                    .multilineTextAlignment(.trailing)
                    .frame(maxWidth: 56, alignment: .trailing)
            case .count(let n):
                Text("\(n)").accessibilityLabel("\(n) done")
            case .none:
                EmptyView()
            }
        }
        .font(.system(size: 13, weight: .semibold).monospacedDigit())
        .foregroundStyle(Color.of(Palette.text, dark: dark))
    }
}

/// Deny and Allow. On the phone they run `AnswerIntent` in the app without opening it; in the gallery they only show.
@MainActor
public struct AnswerButtons: View {
    let mac: String
    let request: String
    let height: CGFloat
    let dark: Bool

    public init(mac: String, request: String, height: CGFloat = 42, dark: Bool = true) {
        self.mac = mac
        self.request = request
        self.height = height
        self.dark = dark
    }

    private static let denyBackground = Token.fixed(0xe5484d2e)

    public var body: some View {
        HStack(spacing: Space.s) {
            button("Deny", decision: "deny", role: .destructive, foreground: Palette.failedText, background: Self.denyBackground)
            button("Allow", decision: "allow", role: nil, foreground: Palette.onText, background: Palette.text)
        }
    }

    @ViewBuilder
    func button(_ title: String, decision: String, role: ButtonRole?, foreground: Token, background: Token) -> some View {
        let label = Text(title)
            .font(.system(size: 15, weight: .semibold))
            .foregroundStyle(Color.of(foreground, dark: dark))
            .frame(maxWidth: .infinity)
            .frame(height: height)
            .background(Color.of(background, dark: dark), in: Capsule())
        #if os(iOS)
            Button(role: role, intent: AnswerIntent(mac: mac, request: request, decision: decision)) { label }
                .buttonStyle(.plain)
        #else
            Button(role: role) {} label: { label }.buttonStyle(.plain)
        #endif
    }
}

/// The Lock Screen banner: the Mac, the top session, what it asks.
@MainActor
public struct LockScreenActivity: View {
    let s: ActivitySnapshot
    @Environment(\.colorScheme) private var scheme

    public init(_ snapshot: ActivitySnapshot) {
        s = snapshot
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: Space.s) {
                LucideIcon(.laptop, size: 16)
                Text(ActivityText.header(s)).lineLimit(1)
                Spacer(minLength: Space.s)
                if !s.isStale, case .elapsed(let since) = ActivityTrailing.reading(s.state) {
                    Text(timerInterval: since...Date.distantFuture, countsDown: false)
                        .monospacedDigit().multilineTextAlignment(.trailing).frame(maxWidth: 56, alignment: .trailing)
                }
            }
            .font(.system(size: 13))
            .foregroundStyle(.secondary)
            Text(s.state.top == nil ? ActivityDisplay.summary(s.state) : s.entry.title)
                .font(.system(size: 17, weight: .semibold))
                .lineLimit(1)
                .padding(.top, 10)
            detail.padding(.top, 4)
            if let answer = s.answer {
                AnswerButtons(mac: answer.mac, request: answer.request, height: 40, dark: scheme == .dark).padding(.top, Space.m)
            }
        }
        .padding(Space.l)
    }

    @ViewBuilder
    private var detail: some View {
        if s.isStale {
            Text(ActivityDisplay.offline(s.attributes.macName)).font(.system(size: 15)).foregroundStyle(.secondary).lineLimit(1)
        } else if let top = s.state.top {
            if let command = ActivityText.command(top) {
                Text(command).font(.system(size: 14, design: .monospaced)).lineLimit(1)
            } else {
                let ask = ActivityDisplay.ask(top)
                Text(s.entry.place.isEmpty ? ask : "\(ask) · \(s.entry.place)").font(.system(size: 15)).foregroundStyle(.secondary).lineLimit(1)
            }
        }
    }
}

/// The Dynamic Island's regions, each its own view so the widget places them and the gallery draws them.
@MainActor
public enum IslandRegion {
    private static let mute = Color.of(Palette.text2, dark: true)

    public static func compactLeading(_ s: ActivitySnapshot) -> some View {
        LucideIcon(.laptop, size: 15).foregroundStyle(Color.of(Palette.text, dark: true))
    }

    @ViewBuilder
    public static func compactTrailing(_ s: ActivitySnapshot) -> some View {
        if s.isStale {
            LucideIcon(.cloudOff, size: 15).foregroundStyle(mute)
        } else {
            ActivityTrailing(s)
        }
    }

    public static func minimal(_ s: ActivitySnapshot) -> some View {
        ActivityDot(s.isStale ? .idle : ActivityDisplay.glyph(s.state), size: 8)
    }

    public static func expandedLeading(_ s: ActivitySnapshot) -> some View {
        HStack(spacing: Space.s) {
            LucideIcon(.laptop, size: 16)
            Text(s.attributes.macName).lineLimit(1)
        }
        .font(.system(size: 13))
        .foregroundStyle(mute)
    }

    @ViewBuilder
    public static func expandedTrailing(_ s: ActivitySnapshot) -> some View {
        if s.isStale {
            Text("Offline").font(.system(size: 13)).foregroundStyle(mute)
        } else {
            ActivityTrailing(s)
        }
    }

    @ViewBuilder
    public static func expandedBottom(_ s: ActivitySnapshot) -> some View {
        let white = Color.of(Palette.text, dark: true)
        VStack(alignment: .leading, spacing: 0) {
            Text(s.state.top == nil ? ActivityDisplay.summary(s.state) : s.entry.title)
                .font(.system(size: 17, weight: .semibold)).foregroundStyle(white).lineLimit(1)
            if s.state.top != nil, !s.entry.place.isEmpty {
                HStack {
                    PlaceLine(place: s.entry.place, color: mute)
                    Spacer(minLength: Space.s)
                    if !s.isStale, let more = ActivityDisplay.more(s.state) {
                        Text(more).font(.system(size: 13)).foregroundStyle(mute)
                    }
                }
                .padding(.top, 2)
            }
            if s.isStale {
                HStack(spacing: Space.s) {
                    LucideIcon(.cloudOff, size: 14)
                    Text(ActivityDisplay.offline(s.attributes.macName)).lineLimit(1)
                }
                .font(.system(size: 15)).foregroundStyle(mute).padding(.top, Space.m)
            } else if let top = s.state.top {
                if let command = ActivityText.command(top) {
                    Text(command)
                        .font(.system(size: 14, design: .monospaced)).foregroundStyle(white).lineLimit(1)
                        .padding(.horizontal, Space.m).padding(.vertical, 9)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .background(Color.of(Palette.surface, dark: true), in: RoundedRectangle(cornerRadius: Radius.small, style: .continuous))
                        .padding(.top, Space.m)
                } else if s.answer == nil {
                    Text(ActivityDisplay.ask(top)).font(.system(size: 15)).foregroundStyle(mute).padding(.top, Space.m)
                }
                if let answer = s.answer {
                    AnswerButtons(mac: answer.mac, request: answer.request).padding(.top, Space.m)
                } else {
                    Link(destination: s.link) {
                        Text("Open").font(.system(size: 15, weight: .semibold)).foregroundStyle(white)
                            .frame(maxWidth: .infinity).frame(height: 42)
                            .background(Color.of(Palette.fill3, dark: true), in: Capsule())
                    }
                    .padding(.top, Space.m)
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}
