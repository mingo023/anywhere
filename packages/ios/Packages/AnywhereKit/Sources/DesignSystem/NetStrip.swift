import Domain
import SwiftUI

/// The one network strip a screen shows: reconnecting, back online, a reconnect that failed (with Retry), or a Mac that
/// is offline. Reconnecting and failed dim the screen's last-known data and turn off its answers and writes; the screen
/// does that, the strip only says so.
@MainActor
public struct NetStrip: View {
    public enum Kind: Hashable, Sendable {
        case reconnecting, back, failed, offline
    }

    let kind: Kind
    let text: String
    let action: (title: String, run: () -> Void)?
    let glass: Bool

    public init(_ kind: Kind, _ text: String, glass: Bool = false, action: (title: String, run: () -> Void)? = nil) {
        self.kind = kind
        self.text = text
        self.glass = glass
        self.action = action
    }

    public var body: some View {
        HStack(spacing: Space.s) {
            icon
            Text(text).frame(maxWidth: .infinity, alignment: .leading).fixedSize(horizontal: false, vertical: true)
            if let action {
                Button(action.title, action: action.run)
                    .buttonStyle(.plain)
                    .font(.system(size: 13, weight: .semibold))
                    .foregroundStyle(Palette.onText.color)
                    .padding(.horizontal, 14)
                    .frame(height: 28)
                    .background(Palette.text.color, in: Capsule())
            }
        }
        .font(.system(size: 13))
        .foregroundStyle((kind == .failed ? Palette.text : Palette.text2).color)
        .padding(.leading, Space.l)
        .padding(.trailing, action == nil ? Space.l : Space.xs)
        .padding(.vertical, Space.s)
        .frame(minHeight: 36)
        .background((glass ? Palette.glass : Palette.fill2).color, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 18, style: .continuous).strokeBorder((glass ? Palette.glassBorder : .fixed(0)).color, lineWidth: 0.5))
        .accessibilityElement(children: .combine)
    }

    @ViewBuilder private var icon: some View {
        switch kind {
        case .reconnecting: RingSpinner(size: 13)
        case .back: LucideIcon(.circleCheck, size: 13).foregroundStyle(Palette.success.color)
        case .failed: LucideIcon(.circleAlert, size: 13).foregroundStyle(Palette.failed.color)
        case .offline: LucideIcon(.cloudOff, size: 13).foregroundStyle(Palette.text2.color)
        }
    }

    /// "Studio Mac Mini is offline · showing data from 10:42", from the last data's time in ms.
    public nonisolated static func offline(mac: String, asOf: Int?, timeZone: TimeZone = .current) -> String {
        guard let asOf else { return "\(mac) is offline" }
        return "\(mac) is offline · showing data from \(TimeText.clock(ms: asOf, timeZone: timeZone))"
    }

    public nonisolated static func reconnecting(mac: String) -> String { "Reconnecting to \(mac)…" }

    public nonisolated static func failed(mac: String, asOf: Int?, timeZone: TimeZone = .current) -> String {
        guard let asOf else { return "Couldn't reconnect to \(mac)" }
        return "Couldn't reconnect to \(mac) · showing data from \(TimeText.clock(ms: asOf, timeZone: timeZone))"
    }
}

/// A ring with one lit quarter, turning: the strip's reconnecting mark.
@MainActor
public struct RingSpinner: View {
    let size: CGFloat
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    public init(size: CGFloat = 13) {
        self.size = size
    }

    public var body: some View {
        TimelineView(.animation(minimumInterval: 1.0 / 30, paused: reduceMotion)) { context in
            let turn = context.date.timeIntervalSinceReferenceDate.truncatingRemainder(dividingBy: 0.8) / 0.8
            ZStack {
                Circle().stroke(Palette.fill4.color, lineWidth: 2)
                Circle().trim(from: 0, to: 0.25).stroke(Palette.text.color, style: StrokeStyle(lineWidth: 2, lineCap: .butt))
                    .rotationEffect(.degrees(reduceMotion ? 0 : turn * 360 - 90))
            }
            .padding(1)
        }
        .frame(width: size, height: size)
        .accessibilityHidden(true)
    }
}
