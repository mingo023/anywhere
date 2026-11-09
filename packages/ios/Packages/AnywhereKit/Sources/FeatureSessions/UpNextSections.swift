import DesignSystem
import Domain
import Stores
import SwiftUI
import Wire

/// Up next, the sessions that want a look in the order to handle them, then those Working. A plain ask is answered where
/// it's shown; anything with a choice opens its chat. `places` names each session's project and branch.
@MainActor
struct UpNextSections: View {
    let host: HostModel
    let mac: String
    let places: [String: SessionPlace]
    @Binding var action: RowAction?

    var body: some View {
        let upNext = host.sessions.upNext
        let working = Workspaces.working(host.sessions.recent.compactMap { host.sessions.session($0)?.listing.card })
        if !upNext.isEmpty {
            Section {
                ForEach(upNext, id: \.self, content: row)
            } header: {
                HStack {
                    Text("Up next").sectionTitle()
                    Spacer()
                    let asking = host.sessions.needsYouCount
                    if asking > 0 {
                        Text("\(asking) need\(asking == 1 ? "s" : "") you").font(Typography.footnote.weight(.semibold)).foregroundStyle(Palette.text.color).textCase(nil)
                    }
                }
            }
        }
        if !working.isEmpty {
            Section {
                ForEach(working, id: \.self) { id in
                    if let session = host.sessions.session(id) {
                        PushRow(value: Route.chat(mac: mac, agentId: id)) { WorkingRow(session: session, place: places[id]) }
                            .modifier(SessionActions(host: host, session: session, action: $action))
                            .bareRow()
                    }
                }
            } header: {
                Text("Working").sectionTitle()
            }
        }
    }

    @ViewBuilder
    private func row(_ id: String) -> some View {
        if let session = host.sessions.session(id) {
            let ask = session.requests.first.flatMap { SessionText.answersInPlace($0) && host.canApprove ? $0 : nil }
            PushRow(value: Route.chat(mac: mac, agentId: id)) { ActiveRow(session: session, place: places[id]?.line, whole: ask != nil) }
                .modifier(SessionActions(host: host, session: session, action: $action))
                .listRowBackground(Palette.surface.color)
                .listRowSeparator(ask == nil ? .automatic : .hidden, edges: .bottom)
            if let ask { AnswerRow(host: host, request: ask).listRowBackground(Palette.surface.color) }
        }
    }
}

/// Deny and Approve under an ask, on its own list row so a press can't also open the chat above it.
@MainActor
private struct AnswerRow: View {
    let host: HostModel
    let request: PermissionRequest
    /// By request id: the next ask queued on the session takes this row over and needs its own answer.
    @State private var answering: String?
    @State private var failed: String?

    var body: some View {
        VStack(alignment: .leading, spacing: Space.xxs) {
            HStack(spacing: Space.s) {
                Button("Deny") { answer(.deny) }.buttonStyle(CapsuleButtonStyle(primary: false, compact: true))
                Button("Approve") { answer(.allow) }.buttonStyle(CapsuleButtonStyle(compact: true))
            }
            .disabled(answering == request.requestId || host.link != .online)
            if failed == request.requestId {
                Text("Couldn't send your answer. Try again.").font(Typography.footnote).foregroundStyle(Palette.failedText.color)
            }
        }
        .padding(.leading, 16 + Space.m)
        .padding(.bottom, Space.xs)
        .listRowInsets(EdgeInsets(top: 0, leading: Space.l, bottom: Space.xs, trailing: Space.l))
    }

    private func answer(_ decision: Decision) {
        let id = request.requestId
        answering = id
        failed = nil
        Haptics.tap()
        Task {
            do {
                try await host.answer(request, decision)
                Haptics.success()
            } catch {
                if answering == id { answering = nil }
                failed = id
                Haptics.warning()
            }
        }
    }
}
