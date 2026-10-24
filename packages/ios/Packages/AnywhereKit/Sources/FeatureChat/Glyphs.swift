import ChatModel
import DesignSystem
import SwiftUI

extension Glyph {
    /// The SF Symbol standing in for the desktop's icon.
    var symbol: String {
        switch self {
        case .terminal: "terminal"
        case .file: "doc"
        case .pencil: "pencil"
        case .filePlus: "doc.badge.plus"
        case .search: "magnifyingglass"
        case .globe: "globe"
        case .bot: "cpu"
        case .blocks: "square.grid.2x2"
        case .bulb: "lightbulb"
        case .tool: "wrench.and.screwdriver"
        }
    }
}

/// The agent's brand mark: a tinted sparkle for Claude, a hexagon for Codex.
@MainActor
struct ProviderIcon: View {
    let provider: String
    var size: CGFloat = 13

    var body: some View {
        Image(systemName: provider == "codex" ? "hexagon" : "sparkle")
            .font(.system(size: size, weight: .semibold))
            .foregroundStyle((provider == "codex" ? Palette.agentCodex : provider == "claude" ? Palette.agentClaude : Palette.text3).color)
    }
}

/// The chat's own sizes, from the desktop's points.
enum ChatFont {
    static let body = Font.system(size: 14)
    static let bubble = Font.system(size: Bubble.textSize)
    static let fold = Font.system(size: 14)
    static let small = Font.system(size: 13)
    static let mono = Font.system(size: 12, design: .monospaced)
    static let monoSmall = Font.system(size: 11.5, design: .monospaced)
    static let caption = Font.system(size: 12)
}

extension View {
    /// Calls `action` with the view's height whenever it lays out anew.
    func onHeight(_ action: @escaping (CGFloat) -> Void) -> some View {
        background(GeometryReader { geo in Color.clear.onChange(of: geo.size.height, initial: true) { _, h in action(h) } })
    }

    /// The FILL_4 flash a row gives when shown from a tray or a notice, fading over 900 ms.
    func flash(_ on: Bool) -> some View {
        modifier(Flash(on: on))
    }
}

private struct Flash: ViewModifier {
    let on: Bool
    @State private var opacity = 0.0

    func body(content: Content) -> some View {
        content
            .background(Palette.fill4.color.opacity(opacity), in: RoundedRectangle(cornerRadius: Radius.small))
            .onChange(of: on, initial: true) { _, on in
                guard on else { return }
                opacity = 1
                withAnimation(.timingCurve(0.22, 1, 0.36, 1, duration: 0.9)) { opacity = 0 }
            }
    }
}

/// A copy button whose icon turns to a check for 1.2 s.
@MainActor
struct CopyButton: View {
    let text: String
    var label: String?
    @State private var copied = false

    var body: some View {
        Button {
            Clipboard.copy(text)
            Haptics.tap()
            copied = true
            Task {
                try? await Task.sleep(for: .seconds(1.2))
                copied = false
            }
        } label: {
            HStack(spacing: Space.xs) {
                Image(systemName: copied ? "checkmark" : "doc.on.doc")
                if let label { Text(label) }
            }
            .font(ChatFont.caption)
            .foregroundStyle(Palette.text3.color)
            .frame(minWidth: 28, minHeight: 28)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(label ?? "Copy")
    }
}

enum Clipboard {
    @MainActor static func copy(_ text: String) {
        #if os(iOS)
            UIPasteboard.general.string = text
        #else
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(text, forType: .string)
        #endif
    }
}
