import Stores
import SwiftUI

extension View {
    /// Tells pocketd and the push centre the chat is on screen while it or one of its sub-agents shows. One opening over
    /// another may appear before the other goes, so the session stays visible until the last of them goes.
    func viewed(_ store: ChatStore, live: Bool) -> some View {
        onAppear {
            guard live else { return }
            store.appear()
            PushCenter.shared.visibleAgent = store.agentId
        }
        .onDisappear {
            guard live else { return }
            store.disappear()
            if !store.viewing, PushCenter.shared.visibleAgent == store.agentId { PushCenter.shared.visibleAgent = nil }
        }
    }
}
