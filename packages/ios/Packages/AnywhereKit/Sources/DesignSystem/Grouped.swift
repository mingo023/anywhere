import Domain
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

/// The screen title's way to another Mac: a list that holds the menu's content, supplied by the shell.
public struct MacSwitch {
    public let othersWaiting: Bool
    public let menu: @MainActor () -> AnyView

    public init(othersWaiting: Bool, menu: @escaping @MainActor () -> AnyView) {
        self.othersWaiting = othersWaiting
        self.menu = menu
    }
}

extension EnvironmentValues {
    @Entry public var macSwitch: MacSwitch? = nil
}

/// The screen's two capsule buttons: ink for the one to press, a quiet surface beside it.
public struct CapsuleButtonStyle: ButtonStyle {
    let primary: Bool
    let compact: Bool
    @Environment(\.isEnabled) private var isEnabled

    public init(primary: Bool = true, compact: Bool = false) {
        self.primary = primary
        self.compact = compact
    }

    public func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font((compact ? Typography.footnote : Typography.body).weight(.semibold))
            .foregroundStyle((primary ? Palette.onText : Palette.text).color)
            .padding(.horizontal, compact ? Space.m : Space.l)
            .frame(minHeight: compact ? 30 : HitTarget.minimum)
            .background((primary ? Palette.text : Palette.fill3).color, in: Capsule())
            .frame(minHeight: HitTarget.minimum)
            .contentShape(Rectangle())
            .opacity(configuration.isPressed ? 0.7 : isEnabled ? 1 : 0.45)
    }
}

/// A search or filter box under a screen's title.
@MainActor
public struct SearchField: View {
    let prompt: String
    @Binding var text: String

    public init(_ prompt: String, text: Binding<String>) {
        self.prompt = prompt
        _text = text
    }

    public var body: some View {
        SearchBox {
            TextField(prompt, text: $text)
                .textFieldStyle(.plain)
                .font(Typography.body)
                .autocorrectionDisabled()
            if !text.isEmpty {
                Button { text = "" } label: { Image(systemName: "xmark.circle.fill").foregroundStyle(Palette.text4.color) }
                    .buttonStyle(.plain)
                    .accessibilityLabel("Clear")
            }
        }
    }
}

/// The box a search field sits in, for one that opens elsewhere too.
@MainActor
public struct SearchBox<Content: View>: View {
    @ViewBuilder let content: Content

    public init(@ViewBuilder content: () -> Content) {
        self.content = content()
    }

    public var body: some View {
        HStack(spacing: Space.s) {
            Image(systemName: "magnifyingglass").foregroundStyle(Palette.text3.color)
            content
        }
        .padding(.horizontal, Space.m)
        .frame(minHeight: 38)
        .background(Palette.fill2.color, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
    }
}

/// A row that pushes `value`, without the disclosure chevron a `NavigationLink` row draws: the list's rows are
/// plainly places to go, and the chevron costs every title its width.
@MainActor
public struct PushRow<Content: View>: View {
    let value: Route
    let content: Content
    @Environment(\.showInDetail) private var showInDetail

    public init(value: Route, @ViewBuilder content: () -> Content) {
        self.value = value
        self.content = content()
    }

    public var body: some View {
        if let showInDetail {
            Button { showInDetail(value) } label: {
                content.frame(maxWidth: .infinity, alignment: .leading).contentShape(Rectangle())
            }
            .buttonStyle(.plain)
        } else {
            link
        }
    }

    @ViewBuilder
    private var link: some View {
        #if os(iOS)
            if #available(iOS 26, *) {
                NavigationLink(value: value) { content }.navigationLinkIndicatorVisibility(.hidden)
            } else {
                // Before iOS 26 only a link hidden behind the row drops the chevron; VoiceOver reads it as a button.
                ZStack(alignment: .leading) {
                    NavigationLink(value: value) { EmptyView() }.opacity(0)
                    content
                }
                .accessibilityElement(children: .combine)
                .accessibilityAddTraits(.isButton)
            }
        #else
            NavigationLink(value: value) { content }
        #endif
    }
}

extension EnvironmentValues {
    /// Set on a split view column whose rows open in another column: a `NavigationLink` only finds destinations in its own.
    @Entry public var showInDetail: (@MainActor (Route) -> Void)?
}

extension View {
    /// A section's title as the lists set them: sentence case, quieter than the rows.
    public func sectionTitle() -> some View {
        font(Typography.subheadline.weight(.semibold)).foregroundStyle(Palette.text2.color).textCase(nil)
    }
}

/// Leaves the bar's middle empty over a screen that leads with its own large title; `navigationTitle` still names it for
/// the back button.
@MainActor
public struct NoBarTitle: ToolbarContent {
    public init() {}

    public var body: some ToolbarContent {
        ToolbarItem(placement: .principal) { Color.clear.frame(width: 1, height: 1) }
    }
}

/// A navigation bar's title with a quieter line under it.
@MainActor
public struct BarTitle: View {
    let title: String
    let detail: String?
    let mono: Bool
    let truncation: Text.TruncationMode

    public init(_ title: String, detail: String?, mono: Bool = true, truncation: Text.TruncationMode = .middle) {
        self.title = title
        self.detail = detail
        self.mono = mono
        self.truncation = truncation
    }

    public var body: some View {
        VStack(spacing: 0) {
            Text(title).font(Typography.subheadline.weight(.semibold)).foregroundStyle(Palette.text.color).lineLimit(1).truncationMode(.middle)
            if let detail {
                Text(detail).font(Typography.caption).fontDesign(mono ? .monospaced : nil).foregroundStyle(Palette.text3.color).lineLimit(1).truncationMode(truncation)
            }
        }
    }
}
