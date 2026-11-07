import Domain
import FeatureChat
import Stores
import SwiftUI

/// A stack over a Mac's chats as the app pushes them, so a scene opens with Back and can push a sub-agent.
@MainActor
struct BenchStack: View {
    let host: HostModel
    @State var path: [Route]

    var body: some View {
        NavigationStack(path: $path) {
            Text("Sessions")
                .navigationDestination(for: Route.self) { route in
                    switch route {
                    case .chat(_, let agentId): ChatView(host: host, agentId: agentId) { path.append($0) }
                    case .subAgent(_, let agentId, let callId): SubAgentView(host: host, agentId: agentId, callId: callId) { path.append($0) }
                    default: EmptyView()
                    }
                }
        }
    }
}
