import DesignSystem
import Domain
import Stores
import SwiftUI

/// A launch's steps in its sheet. Hide, or a swipe down, leaves it running as a row under its project; if it starts
/// while shown, its chat opens.
@MainActor
struct LaunchProgress: View {
    let host: HostModel
    let mac: String
    let requestId: String
    @Environment(\.dismiss) private var dismiss
    @Environment(\.openURL) private var openURL

    private var workspace: WorkspaceStore { host.workspace }

    var body: some View {
        Form {
            CreationProgress(creation: workspace.creations[requestId], retry: { workspace.runKept(requestId) }, dismiss: {
                workspace.forget(creation: requestId)
                dismiss()
            })
        }
        .formStyle(.grouped)
        .toolbar {
            ToolbarItem(placement: .cancellationAction) { Button("Hide") { dismiss() } }
        }
        .onAppear { workspace.watch(requestId, true) }
        .onDisappear { workspace.watch(requestId, false) }
        .onChange(of: workspace.creations[requestId]?.error != nil) { _, failed in
            if failed { Haptics.warning() }
        }
        .onChange(of: workspace.creations[requestId]?.agentId, initial: true) { _, agentId in
            guard let agentId else { return }
            Haptics.success()
            workspace.forget(creation: requestId)
            dismiss()
            openURL(DeepLink.agent(mac: mac, agentId: agentId).url)
        }
    }
}
