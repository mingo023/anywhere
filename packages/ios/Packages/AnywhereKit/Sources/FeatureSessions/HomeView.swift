import DesignSystem
import Domain
import Stores
import SwiftUI
import Wire

/// What a sheet over the session lists makes.
enum Making: Identifiable, Hashable {
    case session(project: String?, place: NewSessionDraft.Place?)
    case worktree(project: String)

    var id: Self { self }
}

/// One Mac's home: the Mac and its link, Up next, then each project's trees with their sessions. `only` narrows it to one project.
@MainActor
struct HomeView: View {
    let host: HostModel
    let mac: String
    var only: String?
    @State private var query = ""
    @AppStorage("sessions.sort") private var sort = Sidebar.SessionSort.newest
    @AppStorage("sessions.projectSort") private var projectSort = HomeList.ProjectSort.manual
    @State private var collapsed: Set<String> = []
    @State private var action: RowAction?
    @State private var making: Making?

    var body: some View {
        let built = host.homeList(query: query, sort: sort, projectSort: projectSort)
        let sections = only.map { p in built.sections.filter { $0.id == p } } ?? built.sections
        List {
            if only == nil {
                Section {
                    MacRow(host: host)
                    // A link inside the toolbar's Menu doesn't push onto the stack.
                    if host.workspace.automations != nil {
                        NavigationLink(value: Route.automations(mac: mac)) { Label("Automations", systemImage: "clock.arrow.circlepath") }
                    }
                }
                if query.isEmpty && !host.sessions.upNext.isEmpty {
                    Section("Up next") {
                        ForEach(host.sessions.upNext, id: \.self) { sessionRow($0) }
                    }
                }
            }
            ForEach(sections) { section in
                Section(isExpanded: expanded(section.id)) {
                    ForEach(section.trees) { tree in
                        treeRow(tree, project: section.id)
                        ForEach(tree.sessions, id: \.self) { sessionRow($0).padding(.leading, Space.l) }
                    }
                } header: {
                    ProjectHeader(section: section, canManage: host.workspace.canManage, making: $making, action: $action)
                }
            }
            if only == nil && !built.elsewhere.isEmpty {
                Section("Other folders") {
                    ForEach(built.elsewhere, id: \.self) { sessionRow($0) }
                }
            }
        }
        .listStyle(.sidebar)
        .overlay { empty(sections) }
        .searchable(text: $query, prompt: "Projects, worktrees and sessions")
        .navigationTitle(only.flatMap { host.workspace.project($0)?.name } ?? host.name)
        .toolbar { toolbar }
        .refreshable {
            try? await host.sessions.refresh()
            try? await host.workspace.refreshProjects()
        }
        .sheet(item: $making) { making in MakingSheet(host: host, mac: mac, making: making) }
        .rowActions(host, $action)
    }

    @ToolbarContentBuilder
    private var toolbar: some ToolbarContent {
        ToolbarItem(placement: .primaryAction) {
            Button { making = .session(project: only, place: nil) } label: { Label("New Session", systemImage: "square.and.pencil") }
                .disabled(host.workspace.projects.isEmpty)
        }
        ToolbarItem(placement: .secondaryAction) {
            Menu {
                Picker("Sessions", selection: $sort) {
                    Text("Newest").tag(Sidebar.SessionSort.newest)
                    Text("Status").tag(Sidebar.SessionSort.status)
                    Text("Recent activity").tag(Sidebar.SessionSort.activity)
                    Text("Name").tag(Sidebar.SessionSort.name)
                }
                if only == nil {
                    Picker("Projects", selection: $projectSort) {
                        Text("As on the Mac").tag(HomeList.ProjectSort.manual)
                        Text("Name").tag(HomeList.ProjectSort.name)
                        Text("Recent activity").tag(HomeList.ProjectSort.recent)
                    }
                }
            } label: {
                Label("Sort", systemImage: "line.3.horizontal.decrease.circle")
            }
        }
    }

    @ViewBuilder
    private func empty(_ sections: [HomeList.Section]) -> some View {
        if !host.workspace.loaded && host.workspace.projects.isEmpty {
            ProgressView()
        } else if sections.isEmpty && !query.isEmpty {
            ContentUnavailableView.search(text: query)
        } else if host.workspace.projects.isEmpty && host.sessions.sessions.isEmpty {
            ContentUnavailableView("No projects yet", systemImage: "folder", description: Text("Add a project on your Mac to start sessions here."))
        }
    }

    private func expanded(_ project: String) -> Binding<Bool> {
        Binding(get: { !query.isEmpty || !collapsed.contains(project) }, set: { open in
            if open { collapsed.remove(project) } else { collapsed.insert(project) }
        })
    }

    private func treeRow(_ tree: HomeList.Tree, project: String) -> some View {
        NavigationLink(value: TreeRow.route(tree, mac: mac)) {
            TreeRow(tree: tree)
        }
        .contextMenu { TreeMenu(tree: tree, project: project, canManage: host.workspace.canManage, making: $making, action: $action) }
    }

    @ViewBuilder
    private func sessionRow(_ id: String) -> some View {
        if let session = host.sessions.session(id) {
            NavigationLink(value: Route.chat(mac: mac, agentId: id)) { SessionRow(session: session) }
                .modifier(SessionActions(host: host, session: session, action: $action))
        }
    }
}

/// A worktree's or Local's row: its kind, name, branch, and its most urgent sessions.
@MainActor
struct TreeRow: View {
    let tree: HomeList.Tree

    var body: some View {
        HStack(spacing: Space.s) {
            Image(systemName: Self.symbol(tree.kind)).foregroundStyle(Palette.text3.color).frame(width: 18)
            VStack(alignment: .leading, spacing: 0) {
                Text(tree.label).font(Typography.body.weight(.medium)).foregroundStyle(Palette.text.color).lineLimit(1)
                if let place = tree.place, place != tree.label {
                    Text(place).font(Typography.caption).foregroundStyle(Palette.text3.color).lineLimit(1)
                }
            }
            Spacer(minLength: Space.s)
            if let roll = tree.rollUp, let label = roll.status.label {
                Pill(roll.count > 1 ? "\(roll.count) \(label.lowercased())" : label, tone: SessionText.tone(roll.status))
            }
        }
    }

    static func route(_ tree: HomeList.Tree, mac: String) -> Route {
        tree.kind == .local ? .local(mac: mac, id: tree.id) : .worktree(mac: mac, path: tree.id)
    }

    static func symbol(_ kind: HomeList.Tree.Kind) -> String {
        switch kind {
        case .own: "laptopcomputer"
        case .local: "folder"
        case .worktree: "arrow.triangle.branch"
        }
    }
}

/// What a tree's menu offers: a session in it, and renaming or deleting it when the Mac lets this phone.
@MainActor
struct TreeMenu: View {
    let tree: HomeList.Tree
    let project: String
    let canManage: Bool
    @Binding var making: Making?
    @Binding var action: RowAction?

    var body: some View {
        Button {
            making = .session(project: project, place: tree.kind == .local ? .local(tree.id) : .tree(tree.id))
        } label: { Label("New Session Here", systemImage: "square.and.pencil") }
        if canManage {
            Button { action = .rename(tree) } label: { Label("Rename", systemImage: "pencil") }
            if tree.kind != .own {
                Button(role: .destructive) { action = .delete(tree) } label: { Label("Delete", systemImage: "trash") }
            }
        }
    }
}

@MainActor
struct ProjectHeader: View {
    let section: HomeList.Section
    let canManage: Bool
    @Binding var making: Making?
    @Binding var action: RowAction?

    var body: some View {
        HStack(spacing: Space.s) {
            Text(section.name)
            if let roll = section.rollUp { StatusGlyph(roll.status, size: 8) }
            Spacer()
            Menu {
                Button { making = .session(project: section.id, place: nil) } label: { Label("New Session", systemImage: "square.and.pencil") }
                if canManage {
                    Button { making = .worktree(project: section.id) } label: { Label("New Worktree", systemImage: "plus.square.on.square") }
                    Button { action = .newLocal(project: section.id) } label: { Label("New Local", systemImage: "folder.badge.plus") }
                }
            } label: {
                Image(systemName: "plus").imageScale(.small)
            }
            .menuIndicator(.hidden)
            .fixedSize()
            .accessibilityLabel("Add to \(section.name)")
        }
    }
}

/// The Mac this list is of, with its link.
@MainActor
struct MacRow: View {
    let host: HostModel

    var body: some View {
        let link = SessionText.link(host.link)
        HStack(spacing: Space.m) {
            Image(systemName: "laptopcomputer").font(.title3).foregroundStyle(Palette.text2.color)
            VStack(alignment: .leading, spacing: 0) {
                Text(host.name).font(Typography.headline).foregroundStyle(Palette.text.color)
                if let summary = Order.summary(host.sessions.sessions.values.map(\.status)) {
                    Text(summary).font(Typography.footnote).foregroundStyle(Palette.text3.color)
                }
            }
            Spacer()
            Pill(link.text, tone: link.tone)
        }
    }
}

/// A new session, or a new worktree, over the list it was asked from.
@MainActor
struct MakingSheet: View {
    let host: HostModel
    let mac: String
    let making: Making

    var body: some View {
        NavigationStack {
            switch making {
            case .session(let project, let place):
                NewSessionView(host: host, mac: mac, project: project, place: place)
            case .worktree(let project):
                NewWorktreeView(host: host, project: project)
            }
        }
    }
}

extension HostModel {
    /// The home list from what the stores hold now.
    func homeList(query: String = "", sort: Sidebar.SessionSort = .newest, projectSort: HomeList.ProjectSort = .manual, open: String? = nil)
        -> HomeList.Built
    {
        let w = workspace
        let listed = sessions.recent.compactMap { sessions.session($0)?.listing }
        return HomeList.build(projects: w.projects, names: w.names, locals: w.locals, localOfTerminal: w.localOfTerminal, sessions: listed,
                              terminalCwds: w.terminalCwds, open: open, query: query, sort: sort, projectSort: projectSort)
    }
}
