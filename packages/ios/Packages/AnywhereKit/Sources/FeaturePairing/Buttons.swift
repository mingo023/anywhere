import DesignSystem
import SwiftUI

/// The pairing screens' big capsule, 52 pt of 17 pt text: ink to press, a quiet fill beside it, and the quiet fill when
/// it can't be pressed. `PillButtonStyle` stops at 15 pt text.
struct LargeButtonStyle: ButtonStyle {
    let primary: Bool
    @Environment(\.isEnabled) private var isEnabled

    init(primary: Bool = true) {
        self.primary = primary
    }

    func makeBody(configuration: Configuration) -> some View {
        let ink = primary && isEnabled
        configuration.label
            .font(.system(size: 17, weight: .semibold))
            .foregroundStyle((ink ? Palette.onText : isEnabled ? Palette.text : Palette.text4).color)
            .frame(maxWidth: .infinity)
            .frame(height: 52)
            .background((ink ? Palette.text : Palette.fill3).color, in: Capsule())
            .contentShape(Capsule())
            .opacity(configuration.isPressed ? 0.7 : 1)
    }
}

/// A text button under the big one: Cancel, Enter code instead.
struct QuietButtonStyle: ButtonStyle {
    var destructive = false

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 17, weight: .semibold))
            .foregroundStyle((destructive ? Palette.failedText : Palette.text).color)
            .frame(maxWidth: .infinity)
            .frame(height: HitTarget.minimum)
            .contentShape(Rectangle())
            .opacity(configuration.isPressed ? 0.7 : 1)
    }
}

/// Leaves the pairing flow: closes its sheet, or goes back to the welcome screen when it is the app's first. Equal while
/// its flow and way out are, so a redraw that makes a new closure doesn't invalidate every screen reading it.
struct ClosePairing: Equatable {
    let owner: ObjectIdentifier?
    let presented: Bool
    let run: @MainActor () -> Void

    @MainActor func callAsFunction() { run() }

    static func == (a: Self, b: Self) -> Bool { a.owner == b.owner && a.presented == b.presented }
}

extension EnvironmentValues {
    @Entry var closePairing = ClosePairing(owner: nil, presented: false, run: {})
}

/// The close button in a pairing screen's bar.
@MainActor
struct CloseButton: View {
    var tint: Token = Palette.text
    @Environment(\.closePairing) private var close

    var body: some View {
        Button { close() } label: { LucideIcon(.close, size: 20).foregroundStyle(tint.color) }
            .accessibilityLabel("Close")
    }
}

/// The small print under a field.
@MainActor
struct Footnote: View {
    let text: String
    var failed = false

    init(_ text: String, failed: Bool = false) {
        self.text = text
        self.failed = failed
    }

    var body: some View {
        Text(text)
            .font(.system(size: 13))
            .foregroundStyle((failed ? Palette.failedText : Palette.text3).color)
            .padding(.horizontal, Inset.margin * 2)
            .padding(.top, Space.s)
            .frame(maxWidth: .infinity, alignment: .leading)
            .fixedSize(horizontal: false, vertical: true)
    }
}

extension View {
    /// The system's inline bar over a pairing screen, with `title` in its middle or nothing there.
    @ViewBuilder
    func pairingBar(_ title: String = "") -> some View {
        #if os(iOS)
            navigationTitle(title).navigationBarTitleDisplayMode(.inline)
        #else
            navigationTitle(title)
        #endif
    }
}
