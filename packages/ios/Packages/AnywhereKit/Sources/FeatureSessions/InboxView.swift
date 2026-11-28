import DesignSystem
import Domain
import Stores
import SwiftUI

/// Every paired Mac's sessions that want a look, in the order to handle them: the Inbox tab's root.
@MainActor
public struct InboxView: View {
    let hosts: [HostModel]
    let open: ((Route) -> Void)?

    public init(hosts: [HostModel], open: ((Route) -> Void)? = nil) {
        self.hosts = hosts
        self.open = open
    }

    public var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                ScreenTitle("Inbox")
                ForEach(hosts) { host in
                    let ids = host.sessions.upNext
                    if !ids.isEmpty {
                        GroupHeader(host.name)
                        VStack(spacing: 0) {
                            ForEach(Array(ids.enumerated()), id: \.element) { index, id in
                                if let session = host.sessions.session(id) {
                                    PushRow(value: Route.chat(mac: host.mac, agentId: id)) {
                                        HStack(spacing: 0) {
                                            StatusGlyph(session.status).frame(width: Inset.textColumn)
                                            Text(session.summary.title).lineLimit(1).padding(.vertical, 12)
                                            Spacer(minLength: Inset.margin)
                                        }
                                        .rowDivider(index > 0)
                                    }
                                    .buttonStyle(.plain)
                                }
                            }
                        }
                        .insetGroup()
                    }
                }
            }
        }
        .screenBackground()
    }
}
