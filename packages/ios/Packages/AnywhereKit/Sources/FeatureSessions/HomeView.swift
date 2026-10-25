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

/// One Mac's workspaces: what is active now, then each project with its worktrees and Locals, as the desktop's
/// sidebar groups them; a workspace opens onto its sessions. `only` narrows it to one project.
@MainActor
struct HomeView: View {
    let host: HostModel
    let mac: String
    var only: String?
    @State private var query = ""
    @AppStorage("sessions.sort") private var sort = Sidebar.SessionSort.newest
    @AppStorage("sessions.projectSort") private var projectSort = HomeList.ProjectSort.manual
    /// Projects opened or closed against their default, which is open while one of their sessions is active.
    @State private var toggled: Set<String> = []
    @State private var action: RowAction?
    @State private var making: Making?
    @State private var automations = false

    var body: some View {
        let built = host.homeList(query: query, sort: sort, projectSort: projectSort)
        let sections = only.map { p in built.sections.filter { $0.id == p } } ?? built.sections
        let active = query.isEmpty && only == nil ? Workspaces.active(host.sessions.recent.compactMap { host.sessions.session($0)?.listing.card }) : []
        let projectOf = Dictionary(sections.flatMap { s in s.trees.flatMap { $0.sessions.map { ($0, s.name) } } }, uniquingKeysWith: { a, _ in a })
        let now = Int(Date().timeIntervalSince1970 * 1000)
        List {
            Section {
                if only == nil {
                    HStack(alignment: .firstTextBaseline) {
                        Text("Workspaces").font(Typography.largeTitle).foregroundStyle(Palette.text.color)
                        Spacer(minLength: Space.s)
                        Text("\(built.sections.count) projects · \(host.sessions.sessions.count) sessions")
                            .font(Typography.footnote).foregroundStyle(Palette.text3.color).lineLimit(1)
                    }
                }
                SearchField(text: $query, prompt: "Search workspaces and sessions")
            }
            .listRowBackground(Color.clear)
            .listRowInsets(EdgeInsets(top: 0, leading: Space.xs, bottom: Space.s, trailing: Space.xs))
            .listRowSeparator(.hidden)
            if !active.isEmpty {
                Section {
                    ForEach(active, id: \.self) { activeRow($0, project: projectOf[$0], now: now) }
                } header: {
                    HStack(spacing: Space.s) { Text("Active now"); Text("\(active.count)").foregroundStyle(Palette.text4.color) }.sectionTitle()
                }
            }
            if !sections.isEmpty {
                Section {
                    ForEach(sections) { projectRows($0, now: now) }
                } header: {
                    HStack {
                        Text("Projects")
                        Spacer()
                        if only == nil { projectSortMenu }
                    }
                    .sectionTitle()
                }
            }
            if only == nil && !built.elsewhere.isEmpty {
                Section {
                    ForEach(built.elsewhere, id: \.self) { sessionRow($0) }
                } header: {
                    Text("Other folders").sectionTitle()
                }
            }
        }
        #if os(iOS)
            .listStyle(.insetGrouped)
        #else
            .listStyle(.inset)
        #endif
        .overlay { empty(sections) }
        .navigationTitle(only.flatMap { host.workspace.project($0)?.name } ?? "Workspaces")
        #if os(iOS)
            .navigationBarTitleDisplayMode(only == nil ? .inline : .large)
        #endif
        .toolbar { toolbar }
        .refreshable {
            try? await host.sessions.refresh()
            try? await host.workspace.refreshProjects()
        }
        .sheet(item: $making) { making in MakingSheet(host: host, mac: mac, making: making) }
        .navigationDestination(isPresented: $automations) { AutomationsView(host: host, mac: mac) }
        .rowActions(host, $action)
    }

    @ToolbarContentBuilder
    private var toolbar: some ToolbarContent {
        if only == nil {
            // The list draws its own large title, beside its counts; the bar keeps the name for the back button.
            ToolbarItem(placement: .principal) { Color.clear.frame(width: 1, height: 1) }
        }
        ToolbarItem(placement: .secondaryAction) {
            Menu {
                Picker("Sessions", selection: $sort) {
                    Text("Newest").tag(Sidebar.SessionSort.newest)
                    Text("Status").tag(Sidebar.SessionSort.status)
                    Text("Recent activity").tag(Sidebar.SessionSort.activity)
                    Text("Name").tag(Sidebar.SessionSort.name)
                }
                if only == nil && host.workspace.automations != nil {
                    // A link inside a Menu doesn't push onto the stack.
                    Button { automations = true } label: { Label("Automations", systemImage: "clock.arrow.circlepath") }
                }
            } label: {
                Label("More", systemImage: "ellipsis")
            }
        }
        ToolbarItem(placement: .primaryAction) {
            Button { making = .session(project: only, place: nil) } label: { Label("New Session", systemImage: "square.and.pencil") }
                .disabled(host.workspace.projects.isEmpty)
        }
    }

    private var projectSortMenu: some View {
        Menu {
            Picker("Projects", selection: $projectSort) {
                Text("As on the Mac").tag(HomeList.ProjectSort.manual)
                Text("Name").tag(HomeList.ProjectSort.name)
                Text("Recent first").tag(HomeList.ProjectSort.recent)
            }
        } label: {
            Text(projectSort == .recent ? "Recent first" : projectSort == .name ? "By name" : "As on the Mac")
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

    private func isOpen(_ section: HomeList.Section) -> Bool {
        if !query.isEmpty || only != nil { return true }
        let busy = section.rollUp.map { $0.status == .working || $0.status == .needsYou } ?? false
        return busy != toggled.contains(section.id)
    }

    private func latest(_ ids: [String]) -> Int {
        ids.compactMap { host.sessions.session($0)?.summary.updatedAt }.max() ?? 0
    }

    @ViewBuilder
    private func projectRows(_ section: HomeList.Section, now: Int) -> some View {
        let open = isOpen(section)
        Button {
            Haptics.tap()
            withAnimation(.snappy(duration: 0.25)) {
                if toggled.contains(section.id) { toggled.remove(section.id) } else { toggled.insert(section.id) }
            }
        } label: {
            ProjectRow(section: section, line: Workspaces.line(workspaces: section.trees.count, latest: latest(section.trees.flatMap(\.sessions)), now: now),
                       open: open)
        }
        .buttonStyle(.plain)
        .contextMenu { ProjectMenu(section: section, canManage: host.workspace.canManage, making: $making, action: $action) }
        if open {
            ForEach(section.trees) { tree in
                PushRow(value: TreeRow.route(tree, mac: mac)) {
                    WorkspaceRow(tree: tree, ago: TimeText.ago(ms: latest(tree.sessions), now: now))
                }
                .listRowInsets(WorkspaceRow.insets)
                .listRowSeparator(.hidden, edges: .top)
                .contextMenu { TreeMenu(tree: tree, project: section.id, canManage: host.workspace.canManage, making: $making, action: $action) }
            }
            Button {
                making = host.workspace.canManage ? .worktree(project: section.id) : .session(project: section.id, place: nil)
            } label: {
                NewWorkspaceRow(title: host.workspace.canManage ? "New workspace" : "New session")
            }
            .buttonStyle(.plain)
            .listRowInsets(WorkspaceRow.insets)
            .listRowSeparator(.hidden, edges: .top)
        }
    }

    private func activeRow(_ id: String, project: String?, now: Int) -> some View {
        Group {
            if let session = host.sessions.session(id) {
                PushRow(value: Route.chat(mac: mac, agentId: id)) { ActiveRow(session: session, project: project) }
                    .modifier(SessionActions(host: host, session: session, action: $action))
            }
        }
    }

    @ViewBuilder
    private func sessionRow(_ id: String) -> some View {
        if let session = host.sessions.session(id) {
            PushRow(value: Route.chat(mac: mac, agentId: id)) { SessionRow(session: session) }
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

/// What a project's menu offers: a session in it, and a worktree or Local when the Mac lets this phone.
@MainActor
struct ProjectMenu: View {
    let section: HomeList.Section
    let canManage: Bool
    @Binding var making: Making?
    @Binding var action: RowAction?

    var body: some View {
        Button { making = .session(project: section.id, place: nil) } label: { Label("New Session", systemImage: "square.and.pencil") }
        if canManage {
            Button { making = .worktree(project: section.id) } label: { Label("New Worktree", systemImage: "plus.square.on.square") }
            Button { action = .newLocal(project: section.id) } label: { Label("New Local", systemImage: "folder.badge.plus") }
        }
    }
}

/// A row that pushes `value`, without the disclosure chevron a `NavigationLink` row draws: the list's rows are
/// plainly places to go, and the chevron costs every title its width.
@MainActor
struct PushRow<Content: View>: View {
    let value: Route
    @ViewBuilder let content: Content

    var body: some View {
        #if os(iOS)
            if #available(iOS 26, *) {
                NavigationLink(value: value) { content }.navigationLinkIndicatorVisibility(.hidden)
            } else {
                // Before iOS 26 only a link hidden behind the row drops the chevron; VoiceOver reads it as a button.
                ZStack(alignment: .leading) {
                    NavigationLink(value: value) { EmptyView() }.opacity(0)
                    content
                }
                .accessibilityElement(children: .combine)
                .accessibilityAddTraits(.isButton)
            }
        #else
            NavigationLink(value: value) { content }
        #endif
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
