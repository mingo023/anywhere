import DesignSystem
import Domain
import Stores
import SwiftUI

/// A Mac's projects, each opening onto its worktrees and Locals, for the iPad sidebar. `value` tags a row with what the
/// sidebar selects: a project as its `.project` route, whose screen holds what can be made in it, and a tree as its
/// `.worktree` or `.local` route.
@MainActor
public struct TreeSidebar<Value: Hashable>: View {
    let host: HostModel
    let value: (Route) -> Value
    @AppStorage("sessions.projectSort") private var projectSort = HomeList.ProjectSort.manual
    @State private var collapsed: Set<String> = []

    public init(host: HostModel, value: @escaping (Route) -> Value) {
        self.host = host
        self.value = value
    }

    public var body: some View {
        ForEach(host.homeList(projectSort: projectSort).sections) { section in
            DisclosureGroup(isExpanded: expanded(section.id)) {
                ForEach(section.trees) { tree in
                    NavigationLink(value: value(TreeRow.route(tree, mac: host.mac))) { TreeRow(tree: tree) }
                }
            } label: {
                NavigationLink(value: value(.project(mac: host.mac, path: section.id))) {
                    HStack(spacing: Space.s) {
                        Label(section.name, systemImage: "shippingbox").lineLimit(1)
                        if let roll = section.rollUp { StatusGlyph(roll.status, size: 8) }
                    }
                }
            }
        }
    }

    private func expanded(_ project: String) -> Binding<Bool> {
        Binding(get: { !collapsed.contains(project) }, set: { open in
            if open { collapsed.remove(project) } else { collapsed.insert(project) }
        })
    }
}
