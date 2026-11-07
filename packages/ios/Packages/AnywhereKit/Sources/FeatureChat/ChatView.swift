import ChatModel
import DesignSystem
import Domain
import MarkdownView
import Stores
import SwiftUI
import Wire

/// One agent's chat.
@MainActor
public struct ChatView: View {
    let host: HostModel
    let agentId: String
    let open: (Route) -> Void

    /// `open` takes the screens the chat links to, such as a file it names.
    public init(host: HostModel, agentId: String, open: @escaping (Route) -> Void = { _ in }) {
        self.host = host
        self.agentId = agentId
        self.open = open
    }

    public var body: some View {
        // A Mac paired again is a new host: the chat takes its store, not the dead connection's.
        HostedChat(host: host, agentId: agentId, open: open).id(ObjectIdentifier(host))
    }
}

@MainActor
private struct HostedChat: View {
    let host: HostModel
    let agentId: String
    let open: (Route) -> Void
    @State private var store: ChatStore?
    @Environment(\.openURL) private var openURL

    private var mac: String { host.hello?.serverId ?? host.id }

    var body: some View {
        if let store {
            ChatScreen(store: store, machine: host.name) { follow($0, in: store) }
                .toolbar {
                    if let cwd = store.summary?.cwd {
                        ToolbarItemGroup(placement: .primaryAction) {
                            Button { open(.changes(mac: mac, tree: cwd)) } label: { Label("Changes", systemImage: "plusminus") }
                            Button { open(.files(mac: mac, tree: cwd, dir: "", chat: agentId)) } label: { Label("Files", systemImage: "folder") }
                            Button { open(.pullRequest(mac: mac, tree: cwd)) } label: {
                                Label("Pull request", systemImage: "arrow.triangle.pull")
                            }
                        }
                    }
                }
                .onChange(of: host.link == .online, initial: true) { _, online in store.follow(online: online, mac: host.name) }
                .onChange(of: store.newSession) { _, ask in
                    guard let ask else { return }
                    store.openedNewSession()
                    open(.newSession(mac: mac, project: ask.project, place: ask.place, prompt: ask.prompt))
                }
        } else {
            Color.clear.screenBackground().onAppear { store = host.chat(agentId) }
        }
    }

    /// A file opens in Files at its line; a link to the worktree's own pull request opens it here, any other on the web.
    private func follow(_ link: MarkdownLink, in store: ChatStore) {
        guard let cwd = store.summary?.cwd else {
            if case .web(let url) = link { openURL(url) }
            return
        }
        switch link {
        case .file(let path, let line):
            open(.file(mac: mac, tree: cwd, path: FileText.linked(path, from: "", tree: cwd) ?? path, line: line, chat: agentId))
        case .web(let url):
            guard let pr = PrLink(url.absoluteString), host.hello?.has(Caps.gh) == true else { return openURL(url) }
            let tree = host.github.pullRequest(cwd)
            Task {
                if !tree.loaded { await host.github.refresh(cwd) }
                if let own = tree.pr.flatMap({ PrLink($0.url) }), own.same(pr) {
                    open(.pullRequest(mac: mac, tree: cwd))
                } else {
                    openURL(url)
                }
            }
        }
    }
}
