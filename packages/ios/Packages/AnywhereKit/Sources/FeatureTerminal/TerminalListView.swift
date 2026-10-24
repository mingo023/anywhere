import DesignSystem
import Domain
import Stores
import SwiftUI
import Wire

/// The terminals of one place: a worktree's folder, or a Local.
public struct TerminalGroup: Identifiable, Hashable, Sendable {
    public let id: String
    public let title: String
    public let path: String
    public let terminals: [Terminal]

    /// Terminals in pocketd's order, grouped by the place each opened in.
    public nonisolated static func of(_ terminals: [Terminal], locals: [Local]) -> [TerminalGroup] {
        var order: [String] = []
        var byPlace: [String: [Terminal]] = [:]
        for t in terminals {
            let key = t.localId.map { "local:\($0)" } ?? t.cwd
            if byPlace[key] == nil { order.append(key) }
            byPlace[key, default: []].append(t)
        }
        return order.map { key in
            let first = byPlace[key]![0]
            let local = first.localId.flatMap { id in locals.first { $0.id == id } }
            let title = local?.name ?? (first.cwd as NSString).lastPathComponent
            return TerminalGroup(id: key, title: title.isEmpty ? first.cwd : title, path: displayPath(local?.project ?? first.cwd), terminals: byPlace[key]!)
        }
    }

    /// A path under a home folder with `~` for it.
    public nonisolated static func displayPath(_ path: String) -> String {
        let parts = path.split(separator: "/", omittingEmptySubsequences: false)
        if parts.count >= 3, parts[0].isEmpty, parts[1] == "Users" || parts[1] == "home" {
            return (["~"] + parts.dropFirst(3)).joined(separator: "/")
        }
        return path
    }
}

/// One terminal's row: what runs in it, where, and the agent if one does.
@MainActor
struct TerminalRow: View, Equatable {
    let terminal: Terminal
    let agent: String?

    var body: some View {
        HStack(spacing: Space.m) {
            Image(systemName: terminal.agentId == nil ? "terminal" : "sparkles")
                .font(.body.weight(.medium))
                .foregroundStyle(terminal.agentId == nil ? Palette.text2.color : Palette.agentClaude.color)
                .frame(width: 34, height: 34)
                .background(Palette.fill2.color, in: RoundedRectangle(cornerRadius: Radius.small, style: .continuous))
            VStack(alignment: .leading, spacing: Space.xxs) {
                Text(terminal.title?.isEmpty == false ? terminal.title! : "Shell")
                    .font(Typography.body.weight(.medium))
                    .foregroundStyle(Palette.text.color)
                    .lineLimit(1)
                if let agent {
                    Text(agent).font(Typography.subheadline).foregroundStyle(Palette.text2.color).lineLimit(1)
                }
            }
            Spacer(minLength: Space.s)
            Text("\(terminal.cols)×\(terminal.rows)")
                .font(Typography.digits)
                .foregroundStyle(Palette.text3.color)
        }
        .padding(.vertical, Space.xxs)
        .accessibilityElement(children: .combine)
    }
}

/// A Mac's terminals by place, with a new one in a worktree or Local.
@MainActor
struct TerminalListView: View {
    let host: HostModel
    @State private var closing: Terminal?
    @State private var creating = false
    @State private var opened: String?
    @State private var problem: String?

    private var store: TerminalStore { host.terminals }
    private var mac: String { host.hello?.serverId ?? host.id }

    var body: some View {
        content
            .navigationTitle("Terminals")
            .toolbar {
                ToolbarItem(placement: .primaryAction) {
                    Button("New Terminal", systemImage: "plus") { creating = true }
                        .disabled(!store.available || store.writesOff)
                }
            }
            .sheet(isPresented: $creating) {
                NewTerminalSheet(store: store) { id in opened = id }
            }
            .navigationDestination(item: $opened) { id in
                TerminalScreenView(store: store, terminalId: id, title: store.terminal(id)?.title ?? "Shell")
            }
            .confirmationDialog("Close this terminal?", isPresented: closingShown, titleVisibility: .visible, presenting: closing) { t in
                Button("Close Terminal", role: .destructive) { close(t) }
            } message: { t in
                Text("\(t.title ?? "The shell") and whatever runs in it end, on your Mac too.")
            }
    }

    @ViewBuilder
    private var content: some View {
        if !store.available {
            Placeholder("Update Pocket on your Mac", systemImage: "terminal", detail: "This Mac's Pocket can't share its terminals yet.")
        } else if !store.listed && store.terminals.isEmpty {
            ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity).screenBackground()
                .onAppear { store.refresh() }
        } else if store.terminals.isEmpty {
            Placeholder("No terminals", systemImage: "terminal", detail: "Open a shell in one of your projects with +.")
        } else {
            list
        }
    }

    private var list: some View {
        List {
            if let problem {
                Text(problem).font(Typography.footnote).foregroundStyle(Palette.failedText.color)
            }
            ForEach(TerminalGroup.of(store.terminals, locals: store.locals)) { group in
                Section {
                    ForEach(group.terminals) { t in
                        NavigationLink(value: Route.terminal(mac: mac, terminalId: t.id)) {
                            TerminalRow(terminal: t, agent: t.agentId.flatMap { host.sessions.session($0)?.summary.title })
                        }
                        .swipeActions(edge: .trailing) {
                            if !store.writesOff {
                                Button("Close", systemImage: "xmark", role: .destructive) { closing = t }
                            }
                        }
                        .contextMenu {
                            Button("Copy Path", systemImage: "doc.on.doc") { copy(t.cwd) }
                            if !store.writesOff {
                                Button("Close Terminal", systemImage: "xmark.circle", role: .destructive) { closing = t }
                            }
                        }
                    }
                } header: {
                    VStack(alignment: .leading, spacing: 0) {
                        Text(group.title)
                        Text(group.path).font(Typography.caption).foregroundStyle(Palette.text3.color).textCase(nil)
                    }
                }
            }
        }
        #if os(iOS)
            .listStyle(.insetGrouped)
        #endif
        .refreshable { store.refresh() }
    }

    private var closingShown: Binding<Bool> {
        Binding(get: { closing != nil }, set: { if !$0 { closing = nil } })
    }

    private func close(_ t: Terminal) {
        Task {
            do {
                try await store.close(t.id)
                Haptics.success()
            } catch {
                Haptics.warning()
                problem = "Couldn't close \(t.title ?? "the terminal")"
            }
        }
    }

    private func copy(_ text: String) {
        #if os(iOS)
            UIPasteboard.general.string = text
        #else
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(text, forType: .string)
        #endif
    }
}
