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
        case .quote: "Added to \(session.agent)'s input"
        }
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

    func body(content: Content) -> some View {
        content
            .onChange(of: request) { _, request in
                if let request { route(request) }
            }
            .confirmationDialog(title, isPresented: choosing, titleVisibility: .visible, presenting: request) { request in
                if case .choose(let sessions) = context.target {
                    ForEach(sessions) { session in
                        Button(session.title) { send(request, to: session) }
                    }
                }
            }
    }

    private var title: String {
        if case .quote = request { "Ask in" } else { "Mention in" }
    }

    private var choosing: Binding<Bool> {
        Binding(
            get: {
                guard request != nil, case .choose = context.target else { return false }
                return true
            },
            set: { if !$0 { request = nil } })
    }

    private func route(_ handoff: Handoff) {
        switch context.target {
        case .one(let session):
            send(handoff, to: session)
        case .choose:
            break
        case .none:
            request = nil
            if case .quote(let text) = handoff {
                context.open?(.newSession(mac: context.mac, project: context.project, place: tree, prompt: text))
            }
        }
    }

    private func send(_ handoff: Handoff, to session: ChatSession) {
        request = nil
        switch handoff {
        case .mention(let path): files.mention(tree: tree, path: path, for: session.id)
        case .quote(let text): files.quote(text, tree: tree, for: session.id)
        }
        Haptics.success()
        guard let open = context.open else { return toast = Toast(handoff.done(session)) }
        let chat = Route.chat(mac: context.mac, agentId: session.id)
        toast = Toast(handoff.done(session), actionTitle: "Go to chat") { open(chat) }
    }
}
