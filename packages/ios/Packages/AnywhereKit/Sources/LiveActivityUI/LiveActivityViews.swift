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

/// The status a state leads with, as a filled symbol tinted by urgency.
@MainActor
public struct ActivityGlyph: View {
    let glyph: ActivityDisplay.Glyph
    let size: CGFloat
    let dark: Bool

    public init(_ glyph: ActivityDisplay.Glyph, size: CGFloat = 22, dark: Bool = true) {
        self.glyph = glyph
        self.size = size
        self.dark = dark
    }

    public var body: some View {
        Image(systemName: Self.symbol(glyph))
            .font(.system(size: size * 0.86, weight: .semibold))
            .foregroundStyle(Color.of(Self.tint(glyph), dark: dark))
            .frame(width: size, height: size)
            .accessibilityLabel(Self.label(glyph))
    }

    public nonisolated static func symbol(_ g: ActivityDisplay.Glyph) -> String {
        switch g {
        case .needsYou: "hand.raised.circle.fill"
        case .failed: "xmark.circle.fill"
        case .working: "ellipsis.circle.fill"
        case .done: "checkmark.circle.fill"
        case .idle: "checkmark.circle"
        }
    }

    nonisolated static func tint(_ g: ActivityDisplay.Glyph) -> Token {
        switch g {
        case .needsYou: Palette.waitingDot
        case .failed: Palette.failed
        case .working: Palette.working
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

/// The compact trailing text: "!2", the longest turn's elapsed time, or the unseen count.
@MainActor
public struct ActivityTrailing: View {
    let trailing: ActivityDisplay.Trailing
    let dark: Bool

    public init(_ trailing: ActivityDisplay.Trailing, dark: Bool = true) {
        self.trailing = trailing
        self.dark = dark
    }

    public var body: some View {
        switch trailing {
        case .needs(let n):
            Text("!\(n)")
                .font(.system(.body, weight: .bold).monospacedDigit())
                .foregroundStyle(Color.of(Palette.waitingDot, dark: dark))
                .accessibilityLabel("\(n) \(n == 1 ? "needs" : "need") you")
        case .timer(let since):
            Text(timerInterval: since...Date.distantFuture, countsDown: false)
                .font(.system(.body, weight: .semibold).monospacedDigit())
                .foregroundStyle(Color.of(Palette.working, dark: dark))
                .multilineTextAlignment(.trailing)
                .frame(maxWidth: 56)
        case .unseen(let n):
            Label("\(n)", systemImage: "checkmark")
                .labelStyle(.titleAndIcon)
                .font(.system(.body, weight: .semibold).monospacedDigit())
                .foregroundStyle(Color.of(Palette.success, dark: dark))
                .accessibilityLabel("\(n) done")
        case .none:
            EmptyView()
        }
    }
}

/// Deny and Allow. On the phone they run `AnswerIntent` in the app without opening it; in the gallery they only show.
@MainActor
public struct AnswerButtons: View {
    let mac: String
    let request: String

    public init(mac: String, request: String) {
        self.mac = mac
        self.request = request
    }

    public var body: some View {
        HStack(spacing: Space.s) {
            button("Deny", decision: "deny", role: .destructive)
                .tint(Color.of(Palette.failed, dark: true))
            button("Allow", decision: "allow", role: nil)
                .tint(Color.of(Palette.success, dark: true))
        }
        .font(.system(.subheadline, weight: .semibold))
    }

    @ViewBuilder
    func button(_ title: String, decision: String, role: ButtonRole?) -> some View {
        #if os(iOS)
            Button(role: role, intent: AnswerIntent(mac: mac, request: request, decision: decision)) {
                Text(title).frame(maxWidth: .infinity)
            }
            .buttonStyle(.borderedProminent)
            .buttonBorderShape(.capsule)
        #else
            Button(role: role) {} label: { Text(title).frame(maxWidth: .infinity) }
                .buttonStyle(.borderedProminent)
                .controlSize(.large)
        #endif
    }
}

/// The Lock Screen banner: the top session, what it asks, and the Mac's counts.
@MainActor
public struct LockScreenActivity: View {
    let s: ActivitySnapshot
    @Environment(\.colorScheme) private var scheme

    public init(_ snapshot: ActivitySnapshot) {
        s = snapshot
    }

    public var body: some View {
        let dark = scheme == .dark
        VStack(alignment: .leading, spacing: Space.m) {
            HStack(alignment: .center, spacing: Space.m) {
                ActivityGlyph(s.isStale ? .idle : ActivityDisplay.glyph(s.state), size: 34, dark: dark)
                VStack(alignment: .leading, spacing: 2) {
                    Text(s.state.top == nil ? s.attributes.macName : s.entry.title)
                        .font(.system(.headline))
                        .lineLimit(1)
                    Text(subtitle)
                        .font(.system(.subheadline))
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
                Spacer(minLength: Space.s)
                if !s.isStale {
                    ActivityTrailing(ActivityDisplay.trailing(s.state), dark: dark)
                }
            }
            if let answer = s.answer {
                AnswerButtons(mac: answer.mac, request: answer.request)
            }
            HStack(spacing: Space.xs) {
                Image(systemName: "laptopcomputer")
                Text(footer).lineLimit(1)
            }
            .font(.system(.caption))
            .foregroundStyle(.secondary)
        }
        .padding(Space.l)
    }

    var subtitle: String {
        if s.isStale { return ActivityDisplay.offline(s.attributes.macName) }
        guard let top = s.state.top else { return ActivityDisplay.summary(s.state) }
        let ask = ActivityDisplay.ask(top)
        return s.entry.place.isEmpty ? ask : "\(ask) · \(s.entry.place)"
    }

    var footer: String {
        [s.attributes.macName, ActivityDisplay.summary(s.state), ActivityDisplay.more(s.state)].compactMap(\.self).joined(separator: " · ")
    }
}

/// The Dynamic Island's regions, each its own view so the widget places them and the gallery draws them.
@MainActor
public enum IslandRegion {
    public static func compactLeading(_ s: ActivitySnapshot) -> some View {
        ActivityGlyph(s.isStale ? .idle : ActivityDisplay.glyph(s.state), size: 22)
    }

    @ViewBuilder
    public static func compactTrailing(_ s: ActivitySnapshot) -> some View {
        if s.isStale {
            Image(systemName: "wifi.slash").foregroundStyle(.secondary)
        } else {
            ActivityTrailing(ActivityDisplay.trailing(s.state))
        }
    }

    public static func minimal(_ s: ActivitySnapshot) -> some View {
        ZStack(alignment: .topTrailing) {
            ActivityGlyph(s.isStale ? .idle : ActivityDisplay.glyph(s.state), size: 22)
            if !s.isStale, let n = ActivityDisplay.badge(s.state), n > 1 {
                Text("\(n)")
                    .font(.system(size: 10, weight: .bold).monospacedDigit())
                    .foregroundStyle(.black)
                    .padding(.horizontal, 3)
                    .background(Color.of(ActivityGlyph.tint(ActivityDisplay.glyph(s.state)), dark: true), in: Capsule())
                    .offset(x: 6, y: -4)
            }
        }
    }

    public static func expandedLeading(_ s: ActivitySnapshot) -> some View {
        Label(s.attributes.macName, systemImage: "laptopcomputer")
            .font(.system(.footnote, weight: .semibold))
            .foregroundStyle(.white.opacity(0.75))
            .lineLimit(1)
    }

    @ViewBuilder
    public static func expandedTrailing(_ s: ActivitySnapshot) -> some View {
        if s.isStale {
            Text("Offline").font(.system(.footnote, weight: .semibold)).foregroundStyle(.white.opacity(0.6))
        } else {
            ActivityTrailing(ActivityDisplay.trailing(s.state))
        }
    }

    public static func expandedCenter(_ s: ActivitySnapshot) -> some View {
        VStack(spacing: 1) {
            Text(s.state.top == nil ? ActivityDisplay.summary(s.state) : s.entry.title)
                .font(.system(.headline))
                .foregroundStyle(.white)
                .lineLimit(1)
            if s.state.top != nil, !s.entry.place.isEmpty {
                Text(s.entry.place).font(.system(.caption)).foregroundStyle(.white.opacity(0.6)).lineLimit(1)
            }
        }
    }

    @ViewBuilder
    public static func expandedBottom(_ s: ActivitySnapshot) -> some View {
        VStack(spacing: Space.s) {
            if s.isStale {
                Label(ActivityDisplay.offline(s.attributes.macName), systemImage: "wifi.slash")
                    .font(.system(.subheadline)).foregroundStyle(.white.opacity(0.7))
            } else if let top = s.state.top {
                HStack(spacing: Space.s) {
                    ActivityGlyph(ActivityDisplay.glyph(s.state), size: 16)
                    Text(ActivityDisplay.ask(top)).font(.system(.subheadline, weight: .medium)).foregroundStyle(.white).lineLimit(1)
                    Spacer(minLength: 0)
                    if let more = ActivityDisplay.more(s.state) {
                        Text(more).font(.system(.caption)).foregroundStyle(.white.opacity(0.6))
                    }
                }
                if let answer = s.answer {
                    AnswerButtons(mac: answer.mac, request: answer.request)
                } else {
                    Link(destination: s.link) {
                        Text("Open").font(.system(.subheadline, weight: .semibold)).frame(maxWidth: .infinity).padding(.vertical, 7)
                            .background(.white.opacity(0.16), in: Capsule())
                    }
                    .foregroundStyle(.white)
                }
            }
        }
    }
}
