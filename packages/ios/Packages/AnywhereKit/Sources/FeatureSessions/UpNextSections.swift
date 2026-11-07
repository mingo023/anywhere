import DesignSystem
import Domain
import Stores
import SwiftUI

/// Up next, the sessions that want a look in the order to handle them, then those Working. Up next counts what the tab
/// badge counts. `projectOf` names each session's project.
@MainActor
struct UpNextSections: View {
    let host: HostModel
    let mac: String
    let projectOf: [String: String]
    @Binding var action: RowAction?

    var body: some View {
        let upNext = host.sessions.upNext
        let working = Workspaces.working(host.sessions.recent.compactMap { host.sessions.session($0)?.listing.card })
        if !upNext.isEmpty {
            Section {
                ForEach(upNext, id: \.self, content: row)
            } header: {
                header("Up next", count: upNext.count)
            }
        }
        if !working.isEmpty {
            Section {
                ForEach(working, id: \.self, content: row)
            } header: {
                header("Working", count: working.count)
            }
        }
    }

    private func header(_ title: String, count: Int) -> some View {
        HStack(spacing: Space.s) { Text(title); Text("\(count)").foregroundStyle(Palette.text4.color) }.sectionTitle()
    }

    @ViewBuilder
    private func row(_ id: String) -> some View {
        if let session = host.sessions.session(id) {
            PushRow(value: Route.chat(mac: mac, agentId: id)) { ActiveRow(session: session, place: projectOf[id]) }
                .modifier(SessionActions(host: host, session: session, action: $action))
        }
    }
}
