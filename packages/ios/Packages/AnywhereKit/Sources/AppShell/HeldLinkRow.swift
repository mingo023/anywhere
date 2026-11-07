import DesignSystem
import Domain
import Stores
import SwiftUI

/// "Opening <session> when <Mac> connects…" while a tapped link waits for its Mac, with a way to give up on it.
@MainActor
struct HeldLinkRow: View {
    let text: String
    let dismiss: () -> Void

    var body: some View {
        HStack(spacing: Space.s) {
            ProgressView().controlSize(.small)
            Text(text).font(Typography.subheadline).foregroundStyle(Palette.text.color).lineLimit(2)
            Spacer(minLength: Space.s)
            Button(action: dismiss) { Image(systemName: "xmark").imageScale(.small).hitTarget() }
                .buttonStyle(.borderless)
                .foregroundStyle(Palette.text3.color)
                .accessibilityLabel("Don't open")
        }
        .padding(.leading, Space.l)
        .padding(.trailing, Space.xs)
        .glass(in: Capsule())
        .shadow(color: .black.opacity(0.08), radius: 12, y: 4)
        .padding(.horizontal, Space.l)
        .padding(.top, Space.xs)
        .transition(.move(edge: .top).combined(with: .opacity))
    }

    /// The row for a held chat link on a paired Mac; nil for anything else.
    static func text(_ link: DeepLink?, app: AppModel) -> String? {
        guard case .agent(let mac, let agentId)? = link, let host = app.host(mac: mac) else { return nil }
        let title = host.sessions.session(agentId)?.summary.title
        return text(session: title?.isEmpty == false ? title : nil, mac: host.name)
    }

    nonisolated static func text(session: String?, mac: String) -> String {
        "Opening \(session ?? "the session") when \(mac) connects…"
    }
}
