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

/// One project of a Mac: its worktrees and Locals, each opening onto its sessions. Without `only`, the sessions of the
/// Mac that are in no project: Other folders.
@MainActor
struct HomeView: View {
    let host: HostModel
    let mac: String
    var only: String?
    /// Pushes a route from where a link can't, as a menu.
    var open: ((Route) -> Void)?
    @AppStorage("sessions.sort") private var sort = Sidebar.SessionSort.newest
    @AppStorage("sessions.projectSort") private var projectSort = HomeList.ProjectSort.manual
    @State private var action: RowAction?
    @State private var making: Making?
    @State private var toast: Toast?

    var body: some View {
        TimelineView(.explicit(host.reachabilityMarks)) { _ in
            screen(asOf: host.staleAsOf(at: .now))
        }
        .toast($toast)
        .sheet(item: $making) { making in MakingSheet(host: host, mac: mac, making: making) }
        .rowActions(host, $action)
    }

    @ViewBuilder
    private func screen(asOf: Int?) -> some View {
        let built = host.homeList(sort: sort, projectSort: projectSort)
        let section = only.flatMap { project in built.sections.first { $0.id == project } }
        Group {
            if !host.workspace.loaded && host.workspace.projects.isEmpty {
                FirstLoad(host: host)
            } else if let section {
                project(section, built: built)
            } else if only == nil {
                otherFolders(built.elsewhere)
            } else {
                Placeholder("It's gone", icon: .folder, detail: "This project is no longer on the Mac.")
            }
        }
        .environment(\.staleSince, asOf)
        .opacity(asOf == nil ? 1 : Staleness.dim)
        .screenBackground()
        .refreshable {
            var failed: (any Error)?
            do { try await host.sessions.refresh() } catch { failed = error }
            do { try await host.workspace.refreshProjects() } catch { failed = failed ?? error }
            if let note = Self.refreshNote(failed, mac: host.name, online: host.link == .online) { toast = Toast(note) }
        }
        .task(id: only) {
            guard let only, host.hello?.has(Caps.gh) == true else { return }
            try? await host.github.refreshList(only)
        }
    }

    private func project(_ section: HomeList.Section, built: HomeList.Built) -> some View {
        let worktrees = section.trees.filter { $0.kind == .worktree }
        let locals = section.trees.filter { $0.kind != .worktree }
        let creations = host.workspace.creations.filter { $0.value.project == section.id && !$0.value.done }.sorted { $0.key < $1.key }
        let pulls = host.github.lists[section.id] ?? []
        return ScrollView {
            LazyVStack(alignment: .leading, spacing: 0) {
                HStack(alignment: .top, spacing: 0) {
                    ScreenTitle(section.name, detail: host.name)
                    TitleMenu {
                        ProjectMenu(section: section, canManage: host.workspace.canManage, making: $making, action: $action, pullRequests: pullRequests(section.id))
                    }
                    .padding(.top, 5)
                    .padding(.trailing, 20)
                }
                if !creations.isEmpty {
                    VStack(spacing: 0) {
                        ForEach(Array(creations.enumerated()), id: \.element.key) { index, creation in
                            CreationRow(creation: creation.value, retry: { host.workspace.runKept(creation.key) }, dismiss: { host.workspace.forget(creation: creation.key) })
                                .onTapGesture { making = .progress(requestId: creation.key) }
                                .rowDivider(index > 0)
                        }
                    }
                    .insetGroup()
                    .padding(.top, Space.xl)
                }
                trees("Worktrees", worktrees, project: section, pulls: pulls)
                trees("Local", locals, project: section, pulls: pulls)
                if !built.elsewhere.isEmpty {
                    PushRow(value: Route.sessions(mac: mac)) {
                        HStack(spacing: 0) {
                            LucideIcon(.folderOpen, size: 20).foregroundStyle(Palette.text2.color).frame(width: Inset.textColumn)
                            Text("Other folders").font(.system(size: 17)).foregroundStyle(Palette.text.color).frame(maxWidth: .infinity, alignment: .leading)
                            LucideIcon(.chevronRight, size: 16).foregroundStyle(Palette.text4.color).padding(.trailing, Space.m)
                        }
                        .frame(height: HitTarget.minimum)
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .insetGroup()
                    .padding(.top, Space.xl)
                }
            }
            .padding(.bottom, Space.xl)
        }
        .cornerButtons(back: true) {
            Button { making = .session(project: section.id, place: nil) } label: { LucideIcon(.plus, size: 20).foregroundStyle(Palette.text.color) }
                .disabled(host.link != .online)
                .accessibilityLabel("New session")
        }
    }

    @ViewBuilder
    private func trees(_ title: String, _ trees: [HomeList.Tree], project: HomeList.Section, pulls: [PrItem]) -> some View {
        if !trees.isEmpty {
            GroupHeader(title)
            VStack(spacing: 0) {
                ForEach(Array(trees.enumerated()), id: \.element.id) { index, tree in
                    let checkout = Self.checkout(tree, project: host.workspace.project(project.id))
                    PushRow(value: TreeRow.route(tree, mac: mac)) {
                        WorktreeRow(tree: tree, name: tree.kind == .own ? project.name : tree.label, head: host.git.changes(checkout).head, pr: pulls.first { $0.tree == tree.id })
                    }
                    .buttonStyle(.plain)
                    .onAppear { host.git.show(checkout) }
                    .onDisappear { host.git.hide(checkout) }
                    .contextMenu { TreeMenu(tree: tree, project: project.id, canManage: host.workspace.canManage, making: $making, action: $action) }
                    .rowDivider(index > 0)
                }
            }
            .insetGroup()
        }
    }

    private func otherFolders(_ ids: [String]) -> some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 0) {
                ScreenTitle("Other folders", detail: host.name)
                if ids.isEmpty {
                    Text("Nothing here.").font(.system(size: 15)).foregroundStyle(Palette.text2.color).frame(maxWidth: .infinity).padding(.top, 60)
                } else {
                    VStack(spacing: 0) {
                        ForEach(Array(ids.enumerated()), id: \.element) { index, id in
                            if let session = host.sessions.session(id) {
                                PushRow(value: Route.chat(mac: mac, agentId: id)) { SessionRow(session: session) }
                                    .buttonStyle(.plain)
                                    .modifier(SessionActions(host: host, session: session, action: $action))
                                    .rowDivider(index > 0)
                            }
                        }
                    }
                    .insetGroup()
                    .padding(.top, Space.xl)
                }
            }
            .padding(.bottom, Space.xl)
        }
        .cornerButtons(back: true) {
            Button { making = .session(project: nil, place: nil) } label: { LucideIcon(.plus, size: 20).foregroundStyle(Palette.text.color) }
                .disabled(host.link != .online || host.workspace.projects.isEmpty)
                .accessibilityLabel("New session")
        }
    }

    /// Opens the project's pull requests, when the Mac reads GitHub.
    private func pullRequests(_ project: String) -> (() -> Void)? {
        guard let open, host.hello?.has(Caps.gh) == true else { return nil }
        return { open(.pullRequests(mac: mac, project: project)) }
    }

    /// The folder a tree's git is read in: a Local's is its project's own checkout.
    static func checkout(_ tree: HomeList.Tree, project: Project?) -> String {
        guard tree.kind == .local, let project else { return tree.id }
        return project.worktrees.first(where: \.isMain)?.path ?? project.path
    }

    /// What a pull to refresh that failed says; nothing while offline, which the Mac's strip already says, or when cancelled.
    nonisolated static func refreshNote(_ error: (any Error)?, mac: String, online: Bool) -> String? {
        guard let error, online, let why = ErrorText.describe(error, mac: mac) else { return nil }
        return "Couldn't refresh · \(why)"
    }
}

/// A worktree or Local of a project: its icon, name, where its branch stands, its pull request and what its sessions
/// are doing.
@MainActor
struct WorktreeRow: View {
    let tree: HomeList.Tree
    let name: String
    let head: GitHead?
    var pr: PrItem?

    var body: some View {
        HStack(spacing: 0) {
            LucideIcon(TreeRow.symbol(tree.kind), size: 20).foregroundStyle(Palette.text2.color).frame(width: Inset.textColumn)
            HStack(spacing: Space.s) {
                VStack(alignment: .leading, spacing: Space.xxs) {
                    Text(name).font(.system(size: 17)).foregroundStyle(Palette.text.color).lineLimit(1)
                    Self.line(branch: tree.branch ?? tree.place, standing: TreeText.standing(head)).lineLimit(1)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                if let pr { PrBadge(item: pr) }
                if let roll = tree.rollUp { StatusGlyph(roll.status) }
                LucideIcon(.chevronRight, size: 16).foregroundStyle(Palette.text4.color)
            }
            .padding(.vertical, Space.m)
            .padding(.trailing, Space.m)
        }
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
    }

    /// "flaky-checkout · 3 ahead", the branch in code.
    static func line(branch: String?, standing: String?) -> Text {
        let parts = [branch.map { Text($0).font(.system(size: 12.5, design: .monospaced)) }, standing.map { Text($0) }].compactMap(\.self)
        return parts.dropFirst().reduce(parts.first ?? Text("")) { $0 + Text(" · ") + $1 }.font(.system(size: 13)).foregroundStyle(Palette.text2.color)
    }
}

/// Where a worktree or Local goes, and how it is drawn.
enum TreeRow {
    static func route(_ tree: HomeList.Tree, mac: String) -> Route {
        tree.kind == .local ? .local(mac: mac, id: tree.id) : .worktree(mac: mac, path: tree.id)
    }

    static func symbol(_ kind: HomeList.Tree.Kind) -> Lucide {
        switch kind {
        case .own, .local: .folder
        case .worktree: .gitBranch
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
        Button { making = .session(project: project, place: Self.place(tree)) } label: { Label("New Session Here", lucide: .compose) }
        if canManage {
            Button { action = .rename(tree) } label: { Label("Rename", lucide: .pencil) }
            if tree.kind != .own {
                Button(role: .destructive) { action = .delete(tree) } label: { Label("Delete", lucide: .trash) }
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
        Button { making = .session(project: section.id, place: nil) } label: { Label("New Session", lucide: .compose) }
        if canManage {
            Button { making = .worktree(project: section.id) } label: { Label("New Worktree", lucide: .worktree) }
            Button { action = .newLocal(project: section.id) } label: { Label("New Local", lucide: .folderPlus) }
        }
        if let pullRequests {
            Button(action: pullRequests) { Label("Pull Requests", lucide: .pullRequest) }
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
