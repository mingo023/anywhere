import DesignSystem
import Domain
import Stores
import SwiftUI
import Wire

/// The pull requests of every paired Mac's projects: the PRs tab's root.
@MainActor
public struct PullRequestsHome: View {
    let hosts: [HostModel]
    let open: ((Route) -> Void)?

    public init(hosts: [HostModel], open: ((Route) -> Void)? = nil) {
        self.hosts = hosts
        self.open = open
    }

    public var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                ScreenTitle("Pull requests")
                ForEach(hosts) { host in
                    let items = host.workspace.projects.flatMap { host.github.lists[$0.path] ?? [] }
                    if !items.isEmpty {
                        CountedHeader(host.name, count: items.count)
                        VStack(spacing: 0) {
                            ForEach(Array(items.enumerated()), id: \.element.tree) { index, item in
                                PushRow(value: Route.pullRequest(mac: host.mac, tree: item.tree)) {
                                    HStack(spacing: 0) {
                                        LucideIcon(.pullRequest, size: 17).foregroundStyle(Palette.text2.color).frame(width: Inset.textColumn)
                                        Text(item.title).lineLimit(2).padding(.vertical, 12)
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
        }
        .screenBackground()
        .task(id: hosts.map(\.id)) {
            for host in hosts {
                for project in host.workspace.projects { try? await host.github.refreshList(project.path) }
            }
        }
    }
}

extension PullRequestsHome {
    /// How many pull requests across `hosts` need action, for the PRs tab's badge.
    @MainActor
    public static func needingAction(_ hosts: [HostModel]) -> Int {
        0
    }
}
