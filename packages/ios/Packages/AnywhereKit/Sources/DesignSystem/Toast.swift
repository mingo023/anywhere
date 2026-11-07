import Domain
import SwiftUI

/// A short note at the bottom of a screen that something happened, with at most one action: "Copied path",
/// "Mentioned in Fix the build · Go to chat". Show one with `.toast($toast)`.
public struct Toast: Identifiable {
    /// How long a toast stays, as the desktop's.
    public static let duration: Duration = .milliseconds(2400)

    public let id = UUID()
    public let text: String
    /// How long it stays; nil keeps it until it is replaced or cleared, for work in flight ("Deleting calm-otter…").
    public let lasts: Duration?
    public let actionTitle: String?
    let action: (@MainActor () -> Void)?

    public init(_ text: String, lasts: Duration? = Toast.duration, actionTitle: String? = nil, action: (@MainActor () -> Void)? = nil) {
        self.text = text
        self.lasts = lasts
        self.actionTitle = actionTitle
        self.action = action
    }

    /// `text` about something now in a chat, with Go to chat when the screen can `open` it.
    @MainActor
    public static func sent(_ text: String, chat: Route, open: ((Route) -> Void)?) -> Toast {
        guard let open else { return Toast(text) }
        return Toast(text, actionTitle: "Go to chat") { open(chat) }
    }
}

extension View {
    /// Shows `toast` over the bottom of this view and clears it once it has lasted, or once its action is taken.
    public func toast(_ toast: Binding<Toast?>) -> some View {
        modifier(ToastHost(toast: toast))
    }
}

@MainActor
private struct ToastHost: ViewModifier {
    @Binding var toast: Toast?

    func body(content: Content) -> some View {
        content
            .overlay(alignment: .bottom) {
                if let toast {
                    ToastBar(toast.text, actionTitle: toast.actionTitle, act: toast.action.map { action in
                        {
                            self.toast = nil
                            action()
                        }
                    })
                }
            }
            .animation(.snappy, value: toast?.id)
            .task(id: toast?.id) {
                guard let shown = toast else { return }
                AccessibilityNotification.Announcement(shown.text).post()
                guard let lasts = shown.lasts else { return }
                try? await Task.sleep(for: lasts)
                if !Task.isCancelled, toast?.id == shown.id { toast = nil }
            }
    }
}

/// A toast as it's drawn, for a screen whose notes come and go with its store rather than `.toast`.
@MainActor
public struct ToastBar: View {
    let text: String
    let actionTitle: String?
    let act: (() -> Void)?

    public init(_ text: String, actionTitle: String? = nil, act: (() -> Void)? = nil) {
        self.text = text
        self.actionTitle = actionTitle
        self.act = act
    }

    public var body: some View {
        HStack(spacing: Space.m) {
            Text(text).font(Typography.subheadline).foregroundStyle(Palette.text.color).lineLimit(2)
            Spacer(minLength: 0)
            if let actionTitle, let act {
                Button(action: act) { Text(actionTitle).hitTarget() }
                    .font(Typography.subheadline.weight(.semibold))
                    .buttonStyle(.borderless)
            }
        }
        .padding(.horizontal, Space.l)
        .padding(.vertical, act == nil ? Space.m : 0)
        .frame(minHeight: HitTarget.minimum)
        .background(Palette.popover.color, in: RoundedRectangle(cornerRadius: Radius.card, style: .continuous))
        .shadow(color: .black.opacity(0.12), radius: 12, y: 4)
        .padding(.horizontal, Space.l)
        .padding(.bottom, Space.s)
        .transition(.move(edge: .bottom).combined(with: .opacity))
    }
}
