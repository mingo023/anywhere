import DesignSystem
import Domain
import Stores
import SwiftUI

/// Every paired Mac with its working sessions and its projects, under a search and filter chips: the Sessions tab's root.
@MainActor
public struct SessionsHome: View {
    let hosts: [HostModel]
    let open: ((Route) -> Void)?
    @State private var query = ""
    @State private var filter = SessionsFilter.all
    /// The Mac the list is narrowed to, by host id.
    @State private var macId: String?
    @State private var actions: [String: RowAction] = [:]
    @State private var toast: Toast?
    @AppStorage("sessions.sort") private var sort = Sidebar.SessionSort.newest
    @AppStorage("sessions.projectSort") private var projectSort = HomeList.ProjectSort.manual

    public init(hosts: [HostModel], open: ((Route) -> Void)? = nil) {
        self.hosts = hosts
        self.open = open
    }

    public var body: some View {
        let marks = hosts.flatMap(\.reachabilityMarks).sorted()
        TimelineView(.explicit(marks)) { _ in
            let now = Date.now
            let shown = hosts.filter { macId == nil || $0.id == macId }
            let blocks = shown.map { Block(host: $0, now: now, query: query, filter: filter, sort: sort, projectSort: projectSort) }
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 0) {
                    HStack(spacing: 0) {
                        ScreenTitle("Sessions")
                        TitleMenu { menu }.padding(.trailing, 20)
                    }
                    SearchField("Search sessions, branches", text: $query).padding(.horizontal, Inset.margin).padding(.top, Space.s)
                    chips.padding(.top, Space.m)
                    ForEach(blocks, id: \.host.id) { block in
                        if !block.rows.isEmpty || !block.host.workspace.loaded && query.isEmpty && filter == .all { mac(block) }
                    }
                    if blocks.allSatisfy({ $0.rows.isEmpty && $0.host.workspace.loaded }) { empty }
                }
                .padding(.bottom, Space.xl)
            }
        }
        .screenBackground()
        .cornerButtons { NewSessionButton(hosts: hosts, open: open) }
        .refreshable { await refresh() }
        .toast($toast)
    }

    private var chips: some View {
        HStack(spacing: Space.s) {
            ForEach(SessionsFilter.allCases, id: \.self) { item in
                Button { filter = item } label: { Chip(title: item.rawValue, selected: filter == item) }.buttonStyle(.plain)
            }
            Menu {
                Picker("Mac", selection: $macId) {
                    Text("All Macs").tag(String?.none)
                    ForEach(hosts) { Text($0.name).tag(Optional($0.id)) }
                }
            } label: {
                Chip(title: hosts.first { $0.id == macId }?.name ?? "Mac", selected: macId != nil, chevron: true)
            }
            .buttonStyle(.plain)
        }
        .padding(.horizontal, Inset.margin)
    }

    @ViewBuilder
    private var menu: some View {
        Picker("Sessions", selection: $sort) {
            Text("Newest").tag(Sidebar.SessionSort.newest)
            Text("Status").tag(Sidebar.SessionSort.status)
            Text("Recent activity").tag(Sidebar.SessionSort.activity)
            Text("Name").tag(Sidebar.SessionSort.name)
        }
        Picker("Projects", selection: $projectSort) {
            Text("As on the Mac").tag(HomeList.ProjectSort.manual)
            Text("Name").tag(HomeList.ProjectSort.name)
            Text("Recent first").tag(HomeList.ProjectSort.recent)
        }
        if let open {
            ForEach(hosts.filter { $0.workspace.automations != nil }) { host in
                // A link inside a Menu doesn't push onto the stack.
                Button { open(.automations(mac: host.mac)) } label: {
                    Label(hosts.count > 1 ? "Automations · \(host.name)" : "Automations", lucide: .clock)
                }
            }
        }
    }

    private func mac(_ block: Block) -> some View {
        let host = block.host
        let link = InboxLayout.Link(block.reachability)
        return VStack(alignment: .leading, spacing: 0) {
            VStack(alignment: .leading, spacing: Space.xxs) {
                Text(host.name).font(.system(size: 15, weight: .semibold)).foregroundStyle(Palette.text.color).lineLimit(1)
                HStack(spacing: 6) {
                    LinkDot(SessionsText.dot(block.reachability))
                    Text(SessionsText.connection(block.reachability, asOf: block.asOf)).font(.system(size: 13)).foregroundStyle(Palette.text2.color).lineLimit(1)
                }
            }
            .padding(.horizontal, Inset.margin * 2)
            .padding(.top, Space.xl)
            .padding(.bottom, Space.s)
            .accessibilityElement(children: .combine)
            if block.rows.isEmpty {
                FirstLoad(host: host).frame(maxWidth: .infinity, minHeight: 96)
            } else {
                LazyVStack(spacing: 0) {
                    ForEach(Array(block.rows.enumerated()), id: \.element.id) { index, row in
                        self.row(row, host: host).rowDivider(index > 0)
                    }
                }
                .insetGroup()
                .environment(\.staleSince, block.asOf)
                .opacity(link.dim)
            }
        }
        .rowActions(host, Binding(get: { actions[host.id] }, set: { actions[host.id] = $0 }))
    }

    @ViewBuilder
    private func row(_ row: Block.Row, host: HostModel) -> some View {
        switch row {
        case .session(let id, let place):
            if let session = host.sessions.session(id) {
                PushRow(value: Route.chat(mac: host.mac, agentId: id)) { SessionRow(session: session, place: place) }
                    .buttonStyle(.plain)
                    .modifier(SessionActions(host: host, session: session, action: Binding(get: { actions[host.id] }, set: { actions[host.id] = $0 })))
            }
        case .project(let section):
            PushRow(value: Route.project(mac: host.mac, path: section.id)) {
                ProjectRow(name: section.name, line: Workspaces.line(worktrees: section.trees.filter { $0.kind == .worktree }.count,
                                                                      locals: section.trees.filter { $0.kind != .worktree }.count),
                           status: section.rollUp?.status)
            }
            .buttonStyle(.plain)
        }
    }

    @ViewBuilder
    private var empty: some View {
        if !query.trimmingCharacters(in: .whitespaces).isEmpty {
            ContentUnavailableView.search(text: query).padding(.top, 60)
        } else if filter != .all {
            ContentUnavailableView { Label("Nothing \(filter.rawValue.lowercased())", lucide: .sessions, size: 28) } description: {
                Text("Sessions that are \(filter.rawValue.lowercased()) show here.")
            }
            .padding(.top, 60)
        } else {
            ContentUnavailableView { Label("No projects yet", lucide: .folder, size: 28) } description: {
                Text("Add a project on your Mac to start sessions here.")
            }
            .padding(.top, 60)
        }
    }

    private func refresh() async {
        for host in hosts {
            var failed: (any Error)?
            do { try await host.sessions.refresh() } catch { failed = error }
            do { try await host.workspace.refreshProjects() } catch { failed = failed ?? error }
            if let note = HomeView.refreshNote(failed, mac: host.name, online: host.link == .online) { toast = Toast(note) }
        }
    }

    /// One Mac's block on the screen, worked out from what its stores hold now.
    @MainActor
    struct Block {
        enum Row: Identifiable {
            case session(String, place: String?)
            case project(HomeList.Section)

            var id: String {
                switch self {
                case .session(let id, _): "s:" + id
                case .project(let section): "p:" + section.id
                }
            }
        }

        let host: HostModel
        let reachability: Reachability
        let asOf: Int?
        let rows: [Row]

        init(host: HostModel, now: Date, query: String, filter: SessionsFilter, sort: Sidebar.SessionSort, projectSort: HomeList.ProjectSort) {
            self.host = host
            reachability = host.reachability(at: now)
            asOf = host.staleAsOf(at: now)
            let built = host.homeList(query: query, sort: sort, projectSort: projectSort)
            if !query.trimmingCharacters(in: .whitespaces).isEmpty {
                let kept = filter == .all ? nil : Set(filter.sessions(built.matches.compactMap { host.sessions.session($0.id)?.listing.card }))
                rows = built.matches.filter { kept?.contains($0.id) ?? true }.map { .session($0.id, place: "\($0.project) · \($0.tree)") }
                    + (filter.showsProjects ? built.sections.map(Row.project) : [])
            } else {
                let cards = host.sessions.recent.compactMap { host.sessions.session($0)?.listing.card }
                rows = filter.sessions(cards).map { Row.session($0, place: nil) } + (filter.showsProjects ? built.sections.map(Row.project) : [])
            }
        }
    }
}

/// A filter chip: ink while it's the one chosen, a quiet fill beside it.
@MainActor
private struct Chip: View {
    let title: String
    let selected: Bool
    var chevron = false

    var body: some View {
        HStack(spacing: Space.xxs) {
            Text(title).font(.system(size: 15, weight: selected ? .semibold : .regular)).lineLimit(1)
            if chevron { LucideIcon(.chevronDown, size: 14).foregroundStyle(selected ? Palette.onText.color : Palette.text2.color) }
        }
        .foregroundStyle((selected ? Palette.onText : Palette.text).color)
        .padding(.leading, 14)
        .padding(.trailing, chevron ? 10 : 14)
        .frame(height: 32)
        .background((selected ? Palette.text : Palette.fill3).color, in: Capsule())
        .contentShape(Capsule())
    }
}

/// A project on the Sessions home: its initials, name and how many worktrees and Locals, and the status that most wants
/// a look in it.
@MainActor
struct ProjectRow: View {
    let name: String
    let line: String
    let status: Status?

    var body: some View {
        HStack(spacing: 0) {
            Text(Workspaces.initials(name))
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(Palette.textBody.color)
                .frame(width: 30, height: 30)
                .background(Palette.fill3.color, in: RoundedRectangle(cornerRadius: Radius.small, style: .continuous))
                .frame(width: Inset.textColumn)
            HStack(spacing: 10) {
                VStack(alignment: .leading, spacing: Space.xxs) {
                    Text(name).font(.system(size: 17)).foregroundStyle(Palette.text.color).lineLimit(1)
                    Text(line).font(.system(size: 13)).foregroundStyle(Palette.text2.color).lineLimit(1)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                if let status { StatusGlyph(status) }
                LucideIcon(.chevronRight, size: 16).foregroundStyle(Palette.text4.color)
            }
            .padding(.vertical, Space.m)
            .padding(.trailing, Space.m)
        }
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
    }
}
