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
    let selected: Value?
    let projectIndent: CGFloat
    let treeIndent: CGFloat
    let value: (Route) -> Value
    @AppStorage("sessions.projectSort") private var projectSort = HomeList.ProjectSort.manual
    @State private var collapsed: Set<String> = []

    public init(host: HostModel, selected: Value?, projectIndent: CGFloat, treeIndent: CGFloat, value: @escaping (Route) -> Value) {
        self.host = host
        self.selected = selected
        self.projectIndent = projectIndent
        self.treeIndent = treeIndent
        self.value = value
    }

    public var body: some View {
        ForEach(host.homeList(projectSort: projectSort).sections) { section in
            let project = value(.project(mac: host.mac, path: section.id))
            let open = !collapsed.contains(section.id)
            SidebarRow(section.name, strong: true, selected: project == selected) {
                ProjectAvatar(name: section.name, statuses: host.statuses(section), side: 20)
            } trailing: {
                fold(section, open: open)
            }
            .sidebarListRow(selected: project == selected, indent: projectIndent)
            .tag(project)
            if open {
                ForEach(section.trees) { tree in
                    let item = value(TreeRow.route(tree, mac: host.mac))
                    SidebarRow(tree.label, detail: tree.place == tree.label ? nil : tree.place, selected: item == selected) {
                        Image(systemName: TreeRow.symbol(tree.kind))
                    } trailing: {
                        if let roll = tree.rollUp { StatusGlyph(roll.status) }
                    }
                    .sidebarListRow(selected: item == selected, indent: treeIndent)
                    .tag(item)
                }
            }
        }
    }

    private func fold(_ section: HomeList.Section, open: Bool) -> some View {
        Button {
            if open { collapsed.insert(section.id) } else { collapsed.remove(section.id) }
        } label: {
            // Tappable past its 16pt slot.
            SidebarChevron(open: open).padding(Space.s).contentShape(Rectangle()).padding(-Space.s)
        }
        .buttonStyle(.plain)
        .accessibilityLabel(open ? "Fold \(section.name)" : "Open \(section.name)")
    }
}
