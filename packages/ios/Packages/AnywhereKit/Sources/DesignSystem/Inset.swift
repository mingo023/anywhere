import SwiftUI

/// The inset-grouped list's measures: groups 16 pt in from the screen's edges, 14 pt corners, and rows whose text column
/// starts 52 pt in, after a 20 pt icon column.
public enum Inset {
    public static let margin: CGFloat = 16
    public static let radius: CGFloat = 14
    /// Where a row's text starts, and so where the hairline between rows starts.
    public static let textColumn: CGFloat = 52
}

extension View {
    /// An inset group: rows on a surface, 16 pt in from the screen's edges, with a hairline edge on the dark canvas.
    public func insetGroup() -> some View {
        frame(maxWidth: .infinity, alignment: .leading)
            .background(Palette.surface.color)
            .clipShape(RoundedRectangle(cornerRadius: Inset.radius, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: Inset.radius, style: .continuous).strokeBorder(Palette.groupBorder.color, lineWidth: 0.5))
            .padding(.horizontal, Inset.margin)
    }

    /// The hairline over a row that follows another in a group, inset to the text column.
    public func rowDivider(_ shown: Bool = true, leading: CGFloat = Inset.textColumn) -> some View {
        overlay(alignment: .top) {
            if shown { Palette.separator.color.frame(height: 0.5).padding(.leading, leading) }
        }
    }
}

/// A group's quiet header, inside the group's inset: "Needs you", "Settings".
@MainActor
public struct GroupHeader: View {
    let title: String

    public init(_ title: String) {
        self.title = title
    }

    public var body: some View {
        Text(title)
            .font(.system(size: 13, weight: .semibold))
            .foregroundStyle(Palette.text2.color)
            .padding(.horizontal, Inset.margin * 2)
            .padding(.top, Space.xl)
            .padding(.bottom, Space.s)
            .frame(maxWidth: .infinity, alignment: .leading)
            .accessibilityAddTraits(.isHeader)
    }
}

/// A section's header over a group in a list that counts: "Needs action 1", the count muted.
@MainActor
public struct CountedHeader: View {
    let title: String
    let count: Int?
    let first: Bool

    public init(_ title: String, count: Int? = nil, first: Bool = false) {
        self.title = title
        self.count = count
        self.first = first
    }

    public var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 6) {
            Text(title).fontWeight(.semibold).foregroundStyle(Palette.text.color)
            if let count { Text("\(count)").monospacedDigit().foregroundStyle(Palette.text3.color) }
        }
        .font(.system(size: 15))
        .padding(.horizontal, 20)
        .padding(.top, first ? 18 : 28)
        .padding(.bottom, Space.s)
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(.isHeader)
    }
}

/// A screen's large title with an optional line under it, as the lists lead: "Inbox", "2 need you · 1 failed".
@MainActor
public struct ScreenTitle: View {
    let title: String
    let detail: String?

    public init(_ title: String, detail: String? = nil) {
        self.title = title
        self.detail = detail
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text(title).font(.system(size: 34, weight: .bold)).foregroundStyle(Palette.text.color).lineLimit(1)
            if let detail {
                Text(detail).font(.system(size: 15)).foregroundStyle(Palette.text2.color).lineLimit(1)
            }
        }
        .padding(.horizontal, 20)
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(.isHeader)
    }
}

/// A round 44 pt glass button over a screen's top corner: New session's +, a list's ….
public struct RoundButtonStyle: ButtonStyle {
    let size: CGFloat

    public init(size: CGFloat = 44) {
        self.size = size
    }

    public func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .foregroundStyle(Palette.text.color)
            .frame(width: size, height: size)
            .background(Palette.glass.color, in: Circle())
            .overlay(Circle().strokeBorder(Palette.glassBorder.color, lineWidth: 0.5))
            .contentShape(Circle())
            .opacity(configuration.isPressed ? 0.7 : 1)
    }
}

/// The design's capsule buttons: ink to press, a quiet fill beside it, a red tint to refuse.
public struct PillButtonStyle: ButtonStyle {
    public enum Kind: Sendable {
        case primary, secondary, destructive
    }

    let kind: Kind
    let height: CGFloat
    @Environment(\.isEnabled) private var isEnabled

    public init(_ kind: Kind = .secondary, height: CGFloat = 44) {
        self.kind = kind
        self.height = height
    }

    public func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 15, weight: .semibold))
            .lineLimit(1)
            .foregroundStyle(foreground.color)
            .padding(.horizontal, height < 40 ? 16 : 20)
            .frame(height: height)
            .background(background.color, in: Capsule())
            .contentShape(Capsule())
            .opacity(configuration.isPressed ? 0.7 : 1)
    }

    private var foreground: Token {
        guard isEnabled else { return Palette.text4 }
        switch kind {
        case .primary: return Palette.onText
        case .secondary: return Palette.text
        case .destructive: return Palette.failedText
        }
    }

    private var background: Token {
        guard isEnabled else { return Palette.fill3 }
        switch kind {
        case .primary: return Palette.text
        case .secondary: return Palette.fill3
        case .destructive: return Palette.failedBackground
        }
    }
}

/// The small grey capsule a list row offers at most one of: Fix, Merge.
public struct RowActionStyle: ButtonStyle {
    public init() {}

    public func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 13, weight: .medium))
            .lineLimit(1)
            .foregroundStyle(Palette.text.color)
            .padding(.horizontal, 12)
            .frame(height: 28)
            .background(Palette.fill3.color, in: Capsule())
            .contentShape(Capsule())
            .opacity(configuration.isPressed ? 0.7 : 1)
    }
}
