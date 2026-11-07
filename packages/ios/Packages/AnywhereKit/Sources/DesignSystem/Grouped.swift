import SwiftUI

/// The app's screen background and grouped rows, in light and dark.
extension View {
    public func screenBackground() -> some View {
        background(Palette.background.color.ignoresSafeArea())
    }

    /// A grouped card: rows inset from the screen's edges on a raised surface.
    public func groupedCard() -> some View {
        padding(.vertical, Space.xs)
            .background(Palette.surface.color, in: RoundedRectangle(cornerRadius: Radius.card, style: .continuous))
            .padding(.horizontal, Space.l)
    }
}

/// What a screen shows before its feature is built, or while it has nothing yet; with an action, such as Try Again
/// on a screen whose first load failed.
@MainActor
public struct Placeholder: View {
    let title: String
    let systemImage: String
    let detail: String?
    let actionTitle: String
    let action: (() -> Void)?

    public init(_ title: String, systemImage: String, detail: String? = nil, actionTitle: String = "Try Again", action: (() -> Void)? = nil) {
        self.title = title
        self.systemImage = systemImage
        self.detail = detail
        self.actionTitle = actionTitle
        self.action = action
    }

    public var body: some View {
        VStack(spacing: Space.m) {
            Image(systemName: systemImage).font(.system(size: 34, weight: .light)).foregroundStyle(Palette.text4.color)
            Text(title).font(Typography.headline).foregroundStyle(Palette.text.color)
            if let detail {
                Text(detail).font(Typography.subheadline).foregroundStyle(Palette.text3.color).multilineTextAlignment(.center)
            }
            if let action {
                Button(actionTitle, action: action).buttonStyle(.bordered).controlSize(.large).padding(.top, Space.xs)
            }
        }
        .padding(Space.xxl)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .screenBackground()
    }
}
