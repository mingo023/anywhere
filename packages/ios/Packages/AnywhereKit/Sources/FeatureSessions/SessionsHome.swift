import DesignSystem
import Domain
import Stores
import SwiftUI

/// Every paired Mac with its projects: the Sessions tab's root.
@MainActor
public struct SessionsHome: View {
    let hosts: [HostModel]
    let open: ((Route) -> Void)?

    public init(hosts: [HostModel], open: ((Route) -> Void)? = nil) {
        self.hosts = hosts
        self.open = open
    }

    public var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                ScreenTitle("Sessions")
                ForEach(hosts) { host in
                    GroupHeader(host.name)
                    VStack(spacing: 0) {
                        ForEach(Array(host.workspace.projects.enumerated()), id: \.element.id) { index, project in
                            PushRow(value: Route.project(mac: host.mac, path: project.path)) {
                                HStack(spacing: 0) {
                                    LucideIcon(.folder, size: 17).foregroundStyle(Palette.text2.color).frame(width: Inset.textColumn)
                                    Text(project.name).lineLimit(1).padding(.vertical, 12)
                                    Spacer(minLength: Inset.margin)
                                }
                                .rowDivider(index > 0)
                            }
                            .buttonStyle(.plain)
                        }
                    }
                    .insetGroup()
                }
            }
        }
        .screenBackground()
    }
}
