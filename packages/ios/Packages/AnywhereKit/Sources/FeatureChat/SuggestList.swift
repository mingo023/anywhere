import ChatModel
import DesignSystem
import SwiftUI
import Wire

/// The `/` command menu, the `@` file picker or the `@mcp/` server picker, opening over the composer: eight rows
/// showing, the rest a scroll away.
@MainActor
struct SuggestList: View {
    let rows: SuggestRows
    let selected: Int
    let provider: String
    let signsIn: (String) -> Bool
    let acting: (String) -> ClientMessage.AgentMcp.Action?
    let pick: (Int) -> Void

    static let rowHeight: CGFloat = 40
    static let maxHeight = rowHeight * CGFloat(Suggest.shown) + 8

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                switch rows {
                case .commands(_, _, let list):
                    ForEach(Array(list.enumerated()), id: \.offset) { i, c in
                        row(i) {
                            Text("/\(c.name)").font(ChatFont.mono.weight(.medium)).foregroundStyle(Palette.text.color)
                            if !c.hint.isEmpty { Text(c.hint).font(ChatFont.mono).foregroundStyle(Palette.text3.color) }
                            Text(c.description).font(ChatFont.caption).foregroundStyle(Palette.text3.color).lineLimit(1)
                        }
                    }
                case .files(_, _, let paths):
                    ForEach(Array(paths.enumerated()), id: \.offset) { i, path in
                        row(i) {
                            FileChip(path)
                            Text(path).font(ChatFont.caption).foregroundStyle(Palette.text3.color).lineLimit(1).truncationMode(.head)
                        }
                    }
                case .servers(_, _, nil, _):
                    note("Checking MCP servers…", Palette.text3)
                case .servers(_, _, .failure(let e)?, _):
                    note(e.message, Palette.failedText)
                case .servers(_, let query, .success(let all)?, _) where all.isEmpty:
                    note(query.isEmpty ? "No MCP servers found" : "No matching MCP servers", Palette.text3)
                case .servers(_, _, .success(let all)?, let pickable):
                    ForEach(Array(all.enumerated()), id: \.offset) { i, s in server(i, s, pickable: i < pickable) }
                }
            }
            .padding(4)
        }
        .frame(maxHeight: Self.maxHeight)
        .fixedSize(horizontal: false, vertical: true)
        .background(Palette.popover.color, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous).strokeBorder(Palette.hairline.color))
        .shadow(color: .black.opacity(0.12), radius: 12, y: 4)
    }

    private func row<Content: View>(_ i: Int, enabled: Bool = true, @ViewBuilder _ content: () -> Content) -> some View {
        Button {
            Haptics.tap()
            pick(i)
        } label: {
            HStack(spacing: 8) { content() }
                .padding(.horizontal, 10)
                .frame(maxWidth: .infinity, minHeight: Self.rowHeight, alignment: .leading)
                .background(i == selected && enabled ? Palette.fill3.color : .clear, in: RoundedRectangle(cornerRadius: 8))
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(!enabled)
    }

    private func server(_ i: Int, _ s: McpServer, pickable: Bool) -> some View {
        let (label, tone) = Mcp.label(provider, s, signingIn: signsIn(s.name), acting: acting(s.name))
        let place = [provider == "codex" ? "Codex" : "Claude", s.scope ?? ""] + (Mcp.action(provider, s) != nil ? [Mcp.status(s, signingIn: signsIn(s.name))] : [])
        return row(i, enabled: pickable) {
            BrandMark(provider, size: 14)
            VStack(alignment: .leading, spacing: 1) {
                Text(s.name).font(ChatFont.small).lineLimit(1)
                Text(place.filter { !$0.isEmpty }.joined(separator: " · ")).font(.system(size: 11)).foregroundStyle(Palette.text3.color).lineLimit(1)
            }
            .foregroundStyle((pickable ? Palette.text : Palette.text4).color)
            Spacer(minLength: 4)
            Text(label).font(.system(size: 11)).foregroundStyle(color(tone).color)
        }
    }

    private func color(_ tone: Mcp.Tone) -> Token {
        switch tone {
        case .muted: Palette.text3
        case .dim: Palette.text4
        case .waiting: Palette.waitingText
        case .link: Palette.link
        }
    }

    private func note(_ text: String, _ color: Token) -> some View {
        Text(text).font(ChatFont.caption).foregroundStyle(color.color).padding(.horizontal, 10).frame(minHeight: Self.rowHeight)
    }
}
