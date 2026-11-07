import DesignSystem
import SwiftUI

/// The keys a phone keyboard lacks, above it. Ctrl and Alt stay down for the next key.
@MainActor
struct KeyBar: View {
    let held: KeyMods
    let enabled: Bool
    let press: (TerminalKey) -> Void
    let toggle: (KeyMods) -> Void
    let paste: () -> Void

    /// Its keys and the padding above and below them.
    nonisolated static let height = HitTarget.minimum + 2 * inset
    private nonisolated static let inset = Space.xs + 2

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: Space.s) {
                key("esc") { press(.escape) }
                key("tab") { press(.tab) }
                modifier("ctrl", .ctrl)
                modifier("alt", .alt)
                key(systemImage: "arrow.left", label: "Left") { press(.left) }
                key(systemImage: "arrow.up", label: "Up") { press(.up) }
                key(systemImage: "arrow.down", label: "Down") { press(.down) }
                key(systemImage: "arrow.right", label: "Right") { press(.right) }
                ForEach(["~", "|", "/", "-"], id: \.self) { c in key(c) { press(.text(c)) } }
                key(systemImage: "doc.on.clipboard", label: "Paste", paste)
            }
            .padding(.horizontal, Space.s)
            .padding(.vertical, Self.inset)
        }
        .disabled(!enabled)
        .opacity(enabled ? 1 : 0.4)
        .background(.bar)
    }

    private func key(_ text: String, _ action: @escaping () -> Void) -> some View {
        Button {
            Haptics.tap()
            action()
        } label: {
            Text(text).font(.system(.callout, design: .monospaced).weight(.medium)).padding(.horizontal, Space.xs).hitTarget()
        }
        .buttonStyle(KeyStyle(on: false))
    }

    private func key(systemImage: String, label: String, _ action: @escaping () -> Void) -> some View {
        Button {
            Haptics.tap()
            action()
        } label: {
            Image(systemName: systemImage).font(.callout.weight(.medium)).hitTarget()
        }
        .buttonStyle(KeyStyle(on: false))
        .accessibilityLabel(label)
    }

    private func modifier(_ text: String, _ mod: KeyMods) -> some View {
        let on = held.contains(mod)
        return Button {
            Haptics.tap()
            toggle(mod)
        } label: {
            Text(text).font(.system(.callout, design: .monospaced).weight(.medium)).padding(.horizontal, Space.xs).hitTarget()
        }
        .buttonStyle(KeyStyle(on: on))
        .accessibilityAddTraits(on ? .isSelected : [])
    }
}

private struct KeyStyle: ButtonStyle {
    let on: Bool

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .foregroundStyle(on ? Palette.onText.color : Palette.text.color)
            .background(
                on ? Palette.accent.color : (configuration.isPressed ? Palette.fill4.color : Palette.surfaceRaised.color),
                in: RoundedRectangle(cornerRadius: Radius.small, style: .continuous)
            )
            .shadow(color: .black.opacity(0.12), radius: 0, y: 1)
    }
}
