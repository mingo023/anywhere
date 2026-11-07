import DesignSystem
import Domain
import Stores
import SwiftUI

/// A file or lines on their way to a chat's draft.
enum Handoff: Hashable {
    case mention(path: String)
    /// `CodeQuote.payload`: the path, the lines and their text.
    case quote(String)

    /// What the toast says once it is in the session's draft.
    func done(_ session: ChatSession) -> String {
        switch self {
        case .mention: "Mentioned in \(session.title)"
        case .quote: ChatHandoff.added(to: session)
        }
    }

    /// What a new session starts with on a tree without one; a mention starts none.
    var prompt: String? {
        if case .quote(let text) = self { text } else { nil }
    }
}

extension View {
    /// Hands `request` to the session `context` targets, asking which when the tree has several, and confirms it with a
    /// toast that goes to the chat. Lines on a tree without a session start one.
    func handoff(_ request: Binding<Handoff?>, files: FilesStore, tree: String, context: FilesContext, toast: Binding<Toast?>) -> some View {
        modifier(HandoffHost(request: request, toast: toast, files: files, tree: tree, context: context))
    }
}

@MainActor
private struct HandoffHost: ViewModifier {
    @Binding var request: Handoff?
    @Binding var toast: Toast?
    let files: FilesStore
    let tree: String
    let context: FilesContext
    @State private var choices: [ChatSession] = []

    func body(content: Content) -> some View {
        content
            .onChange(of: request) { _, request in
                if let request { route(request) }
            }
            .confirmationDialog(title, isPresented: choosing, titleVisibility: .visible, presenting: request) { request in
                ForEach(choices) { session in
                    Button(session.title) { send(request, to: session) }
                }
            }
    }

    private var title: String {
        if case .quote = request { "Ask in" } else { "Mention in" }
    }

    private var choosing: Binding<Bool> {
        Binding(get: { request != nil && !choices.isEmpty }, set: { if !$0 { request = nil } })
    }

    private func route(_ handoff: Handoff) {
        choices = []
        let target = context.retarget?() ?? context.target
        switch ChatHandoff(target, mac: context.mac, project: context.project, tree: tree, prompt: handoff.prompt) {
        case .send(let session):
            send(handoff, to: session)
        case .choose(let sessions):
            choices = sessions
        case .start(let route):
            request = nil
            if let route { context.open?(route) }
        }
    }

    private func send(_ handoff: Handoff, to session: ChatSession) {
        request = nil
        switch handoff {
        case .mention(let path): files.mention(tree: tree, path: path, for: session.id)
        case .quote(let text): files.quote(text, tree: tree, for: session.id)
        }
        Haptics.success()
        toast = .sent(handoff.done(session), chat: .chat(mac: context.mac, agentId: session.id), open: context.open)
    }
}
