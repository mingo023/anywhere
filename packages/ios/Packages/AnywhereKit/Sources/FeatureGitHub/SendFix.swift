import DesignSystem
import Domain
import Stores
import SwiftUI

/// A prompt on its way to the worktree's agent, with the threads, checks or conflict it covers.
struct FixRequest: Hashable {
    let prompt: String
    var threads: [String] = []
    var checks: [String] = []
    var conflict = false
}

extension View {
    /// Sends `request` to the session `context` targets, asking which when the worktree has several, marks what it
    /// covers as sent and opens that chat. A worktree without a session starts one with the prompt.
    func sendsFixes(_ request: Binding<FixRequest?>, store: GitHubStore, tree: String, context: FixContext) -> some View {
        modifier(FixHost(request: request, store: store, tree: tree, context: context))
    }
}

@MainActor
private struct FixHost: ViewModifier {
    @Binding var request: FixRequest?
    let store: GitHubStore
    let tree: String
    let context: FixContext
    @State private var choices: [ChatSession] = []

    func body(content: Content) -> some View {
        content
            .onChange(of: request) { _, request in
                if let request { route(request) }
            }
            .confirmationDialog("Send to", isPresented: choosing, titleVisibility: .visible, presenting: request) { request in
                ForEach(choices) { session in
                    Button(session.title) { send(request, to: session) }
                }
            }
    }

    private var choosing: Binding<Bool> {
        Binding(get: { request != nil && !choices.isEmpty }, set: { if !$0 { request = nil } })
    }

    private func route(_ fix: FixRequest) {
        choices = []
        let target = context.retarget?() ?? context.target
        switch ChatHandoff(target, mac: context.mac, project: context.project, tree: tree, prompt: fix.prompt) {
        case .send(let session):
            send(fix, to: session)
        case .choose(let sessions):
            choices = sessions
        case .start(let route):
            request = nil
            if let route { context.open?(route) }
        }
    }

    private func send(_ fix: FixRequest, to session: ChatSession) {
        request = nil
        guard context.send?(session.id, fix.prompt) == true else {
            Haptics.warning()
            store.pullRequest(tree).error = "\(session.title) can't take a prompt right now."
            return
        }
        store.markSent(tree, threads: fix.threads, checks: fix.checks, conflict: fix.conflict)
        Haptics.success()
        context.open?(.chat(mac: context.mac, agentId: session.id))
    }
}
