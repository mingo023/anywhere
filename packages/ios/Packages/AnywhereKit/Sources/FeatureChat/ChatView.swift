import DesignSystem
import Domain
import Stores
import SwiftUI

/// One agent's chat.
@MainActor
public struct ChatView: View {
    let host: HostModel
    let agentId: String

    public init(host: HostModel, agentId: String) {
        self.host = host
        self.agentId = agentId
    }

    public var body: some View {
        Placeholder(host.sessions.session(agentId)?.summary.title ?? "Chat", systemImage: "bubble.left.and.text.bubble.right")
    }
}
