import DesignSystem
import Domain
import MarkdownView
import Stores
import SwiftUI

/// A sub-agent of an agent's chat, on its own screen.
@MainActor
public struct SubAgentView: View {
    let host: HostModel
    let agentId: String
    let callId: String
    let open: (Route) -> Void
    @State private var store: ChatStore?
    @Environment(\.openURL) private var openURL

    /// `open` takes the screens it links to: the sub-agents it spawned, and the files it names.
    public init(host: HostModel, agentId: String, callId: String, open: @escaping (Route) -> Void = { _ in }) {
        self.host = host
        self.agentId = agentId
        self.callId = callId
        self.open = open
    }

    private var mac: String { host.hello?.serverId ?? host.id }

    public var body: some View {
        if let store {
            SubAgentScreen(store: store, callId: callId, openAgent: { open(.subAgent(mac: mac, agentId: agentId, callId: $0)) }) { link in
                switch link {
                case .file(let path, let line):
                    guard let cwd = store.summary?.cwd else { return }
                    open(.file(mac: mac, tree: cwd, path: FileText.linked(path, from: "", tree: cwd) ?? path, line: line, chat: agentId))
                case .web(let url):
                    openURL(url)
                }
            }
        } else {
            Color.clear.screenBackground().onAppear { store = host.chat(agentId) }
        }
    }
}
