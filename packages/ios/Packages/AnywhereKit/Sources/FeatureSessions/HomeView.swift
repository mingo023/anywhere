import DesignSystem
import Domain
import Stores
import SwiftUI
import Wire

/// What a sheet over the session lists makes, or the steps of a session being started.
enum Making: Identifiable, Hashable {
    case session(project: String?, place: NewSessionDraft.Place?, prompt: String = "")
    case worktree(project: String)
    case progress(requestId: String)

    var id: Self { self }
}

/// One Mac's sessions: Up next and Working, then each project with its worktrees and Locals, as the desktop's sidebar
/// groups them; a worktree or Local opens onto its sessions. While searching, matching sessions lead. `only` narrows it
/// to one project; without `projects` they're left out, for a column beside a sidebar that holds them.
@MainActor
struct HomeView: View {
    let host: HostModel
    let mac: String
    var only: String?
    /// Pushes a route from where a link can't, as a menu.
    var open: ((Route) -> Void)?
    var projects = true
    @State private var query = ""
    @AppStorage("sessions.sort") private var sort = Sidebar.SessionSort.newest
    @AppStorage("sessions.projectSort") private var projectSort = HomeList.ProjectSort.manual
    @AppStorage("sessions.folds") private var folds = Folds()
    @State private var action: RowAction?
    @State private var making: Making?

    var body: some View {
        let built = host.homeList(query: query, sort: sort, projectSort: projectSort)
        let sections = projects ? only.map { p in built.sections.filter { $0.id == p } } ?? built.sections : []
        let searching = !query.trimmingCharacters(in: .whitespaces).isEmpty
        let onlyName = only.flatMap { host.workspace.project($0)?.name }
        let places = Dictionary((projects ? sections : built.sections).flatMap { s in
            s.trees.flatMap { t in t.sessions.map { ($0, SessionPlace(project: s.name, tree: t.branch ?? t.label)) } }
        }, uniquingKeysWith: { a, _ in a })
        let now = Int(Date().timeIntervalSince1970 * 1000)
        List {
            Section {
                VStack(alignment: .leading, spacing: Space.m) {
                    if only == nil { HomeHeader(host: host) }
                    SearchField("Search sessions, branches", text: $query)
                }
            }
            .listRowBackground(Color.clear)
            .listRowInsets(EdgeInsets(top: 0, leading: Space.xs, bottom: Space.s, trailing: Space.xs))
            .listRowSeparator(.hidden)
            if only == nil && !searching {
                UpNextSections(host: host, mac: mac, places: places, action: $action)
            }
            if searching && !built.matches.isEmpty {
                Section {
                    ForEach(built.matches.filter { onlyName == nil || $0.project == onlyName }) { match in
                        sessionRow(match.id, place: "\(match.project) · \(match.tree)")
                    }
                } header: {
                    Text("Sessions").sectionTitle()
                }
            }
            if !sections.isEmpty {
                Section {
                    ForEach(sections) { projectRows($0, now: now, open: searching || only != nil || folds.isOpen($0.id, lively: $0.rollUp != nil)).bareRow() }
                } header: {
                    HStack {
                        Text("Projects").sectionTitle()
                        Spacer()
                        if only == nil { projectSortMenu.font(Typography.footnote).foregroundStyle(Palette.text3.color).textCase(nil) }
                    }
                }
            }
            if only == nil && !built.elsewhere.isEmpty {
                Section {
                    ForEach(built.elsewhere, id: \.self) { id in
                        if let session = host.sessions.session(id) {
                            PushRow(value: Route.chat(mac: mac, agentId: id)) { SessionRow(session: session) }
                                .modifier(SessionActions(host: host, session: session, action: $action))
                                .bareRow()
                        }
                    }
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
        .scrollContentBackground(.hidden)
        .screenBackground()
        .overlay { empty(sections, built: built, searching: searching) }
        .navigationTitle(only.flatMap { host.workspace.project($0)?.name } ?? "Sessions")
        #if os(iOS)
            .navigationBarTitleDisplayMode(only == nil ? .inline : .large)
        #endif
        .toolbar { toolbar }
        .refreshable {
            try? await host.sessions.refresh()
            try? await host.workspace.refreshProjects()
        }
        .task(id: pullRequestProjects(sections, searching: searching)) {
            guard host.hello?.has(Caps.gh) == true else { return }
            for project in pullRequestProjects(sections, searching: searching) { try? await host.github.refreshList(project) }
        }
        .stale(host)
        .sheet(item: $making) { making in MakingSheet(host: host, mac: mac, making: making) }
        .rowActions(host, $action)
    }

    @ToolbarContentBuilder
    private var toolbar: some ToolbarContent {
        if only == nil { NoBarTitle() }
        ToolbarItem(placement: .primaryAction) {
            Menu {
                Picker("Sessions", selection: $sort) {
                    Text("Newest").tag(Sidebar.SessionSort.newest)
                    Text("Status").tag(Sidebar.SessionSort.status)
                    Text("Recent activity").tag(Sidebar.SessionSort.activity)
                    Text("Name").tag(Sidebar.SessionSort.name)
                }
                if only == nil, let open, host.workspace.automations != nil {
                    // A link inside a Menu doesn't push onto the stack.
                    Button { open(.automations(mac: mac)) } label: { Label("Automations", systemImage: "clock.arrow.circlepath") }
                }
            } label: {
                Label("Options", systemImage: "line.3.horizontal.decrease")
            }
        }
        #if os(iOS) && compiler(>=6.2)
            if #available(iOS 26, *) { ToolbarSpacer(.fixed, placement: .primaryAction) }
        #endif
        ToolbarItem(placement: .primaryAction) { newSession }
    }

    private var newSession: some View {
        Button { making = .session(project: only, place: nil) } label: { Label("New Session", systemImage: "plus") }
            .disabled(host.workspace.projects.isEmpty)
    }

    private var projectSortMenu: some View {
        Menu {
            Picker("Projects", selection: $projectSort) {
                Text("As on the Mac").tag(HomeList.ProjectSort.manual)
                Text("Name").tag(HomeList.ProjectSort.name)
                Text("Recent first").tag(HomeList.ProjectSort.recent)
            }
        } label: {
            Text(projectSort == .recent ? "Recent first" : projectSort == .name ? "By name" : "As on the Mac").hitTarget()
        }
    }

    @ViewBuilder
    private func empty(_ sections: [HomeList.Section], built: HomeList.Built, searching: Bool) -> some View {
        if !host.workspace.loaded && host.workspace.projects.isEmpty {
            FirstLoad(host: host)
        } else if sections.isEmpty && built.matches.isEmpty && searching {
            ContentUnavailableView.search(text: query)
        } else if host.workspace.projects.isEmpty && host.sessions.sessions.isEmpty {
            ContentUnavailableView("No projects yet", systemImage: "folder", description: Text("Add a project on your Mac to start sessions here."))
        } else if !projects && !searching && built.elsewhere.isEmpty && host.sessions.upNext.isEmpty
            && !host.sessions.sessions.values.contains(where: { $0.status == .working })
        {
            ContentUnavailableView("Nothing needs you", systemImage: "checkmark.circle", description: Text("Sessions that need you, fail or finish show here."))
        }
    }

    /// The open projects with worktrees, whose pull requests badge their rows.
    private func pullRequestProjects(_ sections: [HomeList.Section], searching: Bool) -> [String] {
        sections.filter { s in
            s.trees.contains { $0.kind == .worktree } && (searching || only != nil || folds.isOpen(s.id, lively: s.rollUp != nil))
        }.map(\.id)
    }

    private func latest(_ ids: [String]) -> Int {
        ids.compactMap { host.sessions.session($0)?.summary.updatedAt }.max() ?? 0
    }

    @ViewBuilder
    private func projectRows(_ section: HomeList.Section, now: Int, open: Bool) -> some View {
        let lively = section.rollUp != nil
        let statuses = host.statuses(section)
        Button {
            Haptics.tap()
            withAnimation(.snappy(duration: 0.25)) { folds.toggle(section.id, lively: lively) }
        } label: {
            ProjectRow(section: section,
                       line: Workspaces.line(worktrees: section.trees.filter { $0.kind == .worktree }.count, locals: section.trees.filter { $0.kind != .worktree }.count),
                       ago: TimeText.ago(ms: latest(section.trees.flatMap(\.sessions)), now: now), open: open, statuses: statuses,
                       breakdown: Workspaces.breakdown(statuses))
        }
        .buttonStyle(.plain)
        .contextMenu {
            ProjectMenu(section: section, canManage: host.workspace.canManage, making: $making, action: $action, pullRequests: pullRequests(section.id))
        }
        ForEach(creations(section.id), id: \.id) { creation in
            CreationRow(creation: creation.value, retry: { host.workspace.runKept(creation.id) }, dismiss: { host.workspace.forget(creation: creation.id) })
                .onTapGesture { making = .progress(requestId: creation.id) }
                .listRowInsets(WorkspaceRow.insets)
        }
        if open {
            let pulls = host.github.lists[section.id] ?? []
            ForEach(section.trees) { tree in
                PushRow(value: TreeRow.route(tree, mac: mac)) {
                    WorkspaceRow(tree: tree, ago: TimeText.ago(ms: latest(tree.sessions), now: now), pr: pulls.first { $0.tree == tree.id })
                }
                .listRowInsets(WorkspaceRow.insets)
                .listRowSeparator(.hidden)
                .modifier(TreeActions(tree: tree, project: section.id, canManage: host.workspace.canManage, making: $making, action: $action))
            }
        }
    }

    /// Opens the project's pull requests, when the Mac reads GitHub.
    private func pullRequests(_ project: String) -> (() -> Void)? {
        guard let open, host.hello?.has(Caps.gh) == true else { return nil }
        return { open(.pullRequests(mac: mac, project: project)) }
    }

    /// The sessions of a project being started, oldest request first.
    private func creations(_ project: String) -> [(id: String, value: Creation)] {
        host.workspace.creations.filter { $0.value.project == project && !$0.value.done }.sorted { $0.key < $1.key }.map { ($0.key, $0.value) }
    }

    @ViewBuilder
    private func sessionRow(_ id: String, place: String) -> some View {
        if let session = host.sessions.session(id) {
            PushRow(value: Route.chat(mac: mac, agentId: id)) { ActiveRow(session: session, place: place, titled: true) }
                .modifier(SessionActions(host: host, session: session, action: $action))
        }
    }
}

/// The top of the list: the Mac, which switches Macs where there are several, and how it's connected.
@MainActor
struct HomeHeader: View {
    let host: HostModel
    @Environment(\.macSwitch) private var macSwitch

    var body: some View {
        VStack(alignment: .leading, spacing: Space.xs) {
            if let macSwitch {
                Menu { macSwitch.menu() } label: { title }
                    .buttonStyle(.plain)
                    .accessibilityLabel("Mac: \(host.name)")
                    .accessibilityValue(macSwitch.othersWaiting ? "Another Mac has sessions up next" : "")
            } else {
                title
            }
            TimelineView(.explicit(host.reachabilityMarks)) { _ in
                HStack(spacing: Space.xs + 2) {
                    Circle().fill((host.link == .online ? Palette.success : Palette.text4).color).frame(width: 7, height: 7)
                    Text(Self.status(host.reachability(at: .now), sessions: host.sessions.sessions.count))
                        .font(Typography.footnote).foregroundStyle(Palette.text3.color).lineLimit(1)
                }
            }
        }
    }

    private var title: some View {
        HStack(spacing: Space.s) {
            Text(host.name).font(Typography.largeTitle).foregroundStyle(Palette.text.color).lineLimit(1).minimumScaleFactor(0.7)
            if macSwitch?.othersWaiting == true { Circle().fill(Palette.text.color).frame(width: 7, height: 7) }
            if macSwitch != nil { Image(systemName: "chevron.down").font(Typography.caption.weight(.semibold)).foregroundStyle(Palette.text3.color) }
        }
    }

    /// "Connected over Tailscale · 40 sessions".
    nonisolated static func status(_ reachability: Reachability, sessions: Int) -> String {
        let link = reachability == .online ? "Connected over Tailscale" : reachability.label
        return "\(link) · \(sessions) session\(sessions == 1 ? "" : "s")"
    }
}

/// A worktree's or Local's row: its kind, name, branch, and its most urgent sessions.
@MainActor
struct TreeRow: View {
    let tree: HomeList.Tree

    var body: some View {
        HStack(spacing: Space.s) {
            Image.named(Self.symbol(tree.kind), size: 17).foregroundStyle(Palette.text3.color).frame(width: 18)
            VStack(alignment: .leading, spacing: 0) {
                Text(tree.label).font(Typography.body.weight(.medium)).foregroundStyle(Palette.text.color).lineLimit(1)
                if let place = tree.place, place != tree.label {
                    Text(place).font(Typography.caption).foregroundStyle(Palette.text3.color).lineLimit(1)
                }
            }
            Spacer(minLength: Space.s)
            if let roll = tree.rollUp, let label = Workspaces.label(roll.status, count: roll.count) {
                Pill(label, tone: SessionText.tone(roll.status))
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
        case .worktree: "worktree"
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
        Button { making = .session(project: project, place: Self.place(tree)) } label: { Label("New Session Here", systemImage: "square.and.pencil") }
        if canManage {
            Button { action = .rename(tree) } label: { Label("Rename", systemImage: "pencil") }
            if tree.kind != .own {
                Button(role: .destructive) { action = .delete(tree) } label: { Label("Delete", systemImage: "trash") }
            }
        }
    }

    static func place(_ tree: HomeList.Tree) -> NewSessionDraft.Place {
        tree.kind == .local ? .local(tree.id) : .tree(tree.id)
    }
}

/// What a project's menu offers: a session in it, a worktree or Local when the Mac lets this phone, and its pull requests.
@MainActor
struct ProjectMenu: View {
    let section: HomeList.Section
    let canManage: Bool
    @Binding var making: Making?
    @Binding var action: RowAction?
    let pullRequests: (() -> Void)?

    var body: some View {
        Button { making = .session(project: section.id, place: nil) } label: { Label("New Session", systemImage: "square.and.pencil") }
        if canManage {
            Button { making = .worktree(project: section.id) } label: { Label("New Worktree", systemImage: "plus.square.on.square") }
            Button { action = .newLocal(project: section.id) } label: { Label("New Local", systemImage: "folder.badge.plus") }
        }
        if let pullRequests {
            Button(action: pullRequests) { Label("Pull Requests", systemImage: "arrow.triangle.pull") }
        }
    }
}

/// A new session or worktree, or a session's steps, over the list it was asked from.
@MainActor
struct MakingSheet: View {
    let host: HostModel
    let mac: String
    let making: Making

    var body: some View {
        NavigationStack {
            switch making {
            case .session(let project, let place, let prompt):
                NewSessionView(host: host, mac: mac, project: project, place: place, prompt: prompt)
            case .worktree(let project):
                NewWorktreeView(host: host, project: project)
            case .progress(let requestId):
                LaunchProgress(host: host, mac: mac, requestId: requestId)
            }
        }
    }
}

extension HostModel {
    /// The chats of the worktree or Local `route` names, in its list's order; of a project, those of its main checkout.
    public func chats(in route: Route) -> [Route] {
        let trees = homeList().sections.flatMap { section in section.trees.map { (section, $0) } }
        let found = trees.first { section, tree in
            if case .project(_, let path) = route { return section.id == path && tree.kind == .own }
            return TreeRow.route(tree, mac: mac) == route
        }
        return found?.1.sessions.map { .chat(mac: mac, agentId: $0) } ?? []
    }

    /// The status of each session in a project, for its tile's badges.
    func statuses(_ section: HomeList.Section) -> [Status] {
        section.trees.flatMap(\.sessions).compactMap { sessions.session($0)?.status }
    }

    /// The home list from what the stores hold now, without the trees being deleted.
    func homeList(query: String = "", sort: Sidebar.SessionSort = .newest, projectSort: HomeList.ProjectSort = .manual, open: String? = nil)
        -> HomeList.Built
    {
        let w = workspace
        let listed = sessions.recent.compactMap { sessions.session($0)?.listing }
        return HomeList.build(projects: w.projects, names: w.names, locals: w.locals, localOfTerminal: w.localOfTerminal, sessions: listed,
                              terminalCwds: w.terminalCwds, open: open, query: query, sort: sort, projectSort: projectSort, hiding: w.hidden)
    }
}
