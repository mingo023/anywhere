import ChatModel
import DesignSystem
import Domain
import MarkdownView
import Stores
import SwiftUI

/// One agent's chat.
@MainActor
public struct ChatView: View {
    let host: HostModel
    let agentId: String
    let open: (Route) -> Void
    @State private var store: ChatStore?

    /// `open` takes the screens the chat links to, such as a file it names.
    public init(host: HostModel, agentId: String, open: @escaping (Route) -> Void = { _ in }) {
        self.host = host
        self.agentId = agentId
        self.open = open
    }

    public var body: some View {
        if let store {
            ChatScreen(store: store, machine: host.name) { link in
                switch link {
                case .web: break
                case .file(let path, _):
                    guard let cwd = store.summary?.cwd else { return }
                    open(.file(mac: host.hello?.serverId ?? host.id, tree: cwd, path: path))
                }
            }
        } else {
            Color.clear.screenBackground().onAppear { store = host.chat(agentId) }
        }
    }
}

/// The chat over a store: the transcript, and the composer under it.
@MainActor
struct ChatScreen: View {
    let store: ChatStore
    let machine: String
    /// A file link; web links open in the browser.
    let openFile: (MarkdownLink) -> Void
    /// Off for gallery scenes, which have no Mac to tell.
    var live = true
    @State private var mapOpen = false
    @State private var showing: String?
    @Environment(\.openURL) private var openURL

    var body: some View {
        VStack(spacing: 0) {
            TranscriptView(store: store, showing: $showing, openLink: openLink)
            ComposerView(store: store, machine: machine, show: { showing = $0 }, mapOpen: $mapOpen)
        }
        .screenBackground()
        .navigationTitle(store.summary?.title ?? "Chat")
        #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
        #endif
        .onAppear {
            guard live else { return }
            store.appear()
            PushCenter.shared.visibleAgent = store.agentId
        }
        .onDisappear {
            guard live else { return }
            store.disappear()
            if PushCenter.shared.visibleAgent == store.agentId { PushCenter.shared.visibleAgent = nil }
        }
        .sheet(isPresented: $mapOpen) {
            AgentMapSheet(transcript: store.session.transcript, provider: store.provider) { showing = $0 }
        }
    }

    private func openLink(_ link: MarkdownLink) {
        if case .web(let url) = link { openURL(url) } else { openFile(link) }
    }
}
