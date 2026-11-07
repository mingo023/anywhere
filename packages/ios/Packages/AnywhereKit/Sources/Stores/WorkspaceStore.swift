import Client
import Domain
import Foundation
import Observation
import Wire

/// A worktree or session being made: the steps pocketd reported, in order, and how it ended.
public struct Creation: Hashable, Sendable {
    public enum Kind: Hashable, Sendable { case worktree, session }
    public struct Step: Hashable, Sendable {
        public var step: String
        public var note: String?
    }

    public var kind: Kind
    public var steps: [Step] = []
    /// The terminal an agent.create opened, once it exists, and the folder it opened in.
    public var terminalId: String?
    public var cwd: String?
    /// The request id it goes out with when not its own, after pocketd answered the first with an error.
    public var requestId: String?
    public var error: ServerError?
    public var done = false
    /// The Local and worktree a launch made before a later step failed, so trying again starts there.
    public var madeLocal: String?
    public var madeTree: String?
    /// What a session launch runs, so trying again runs the rest of it with the same request id.
    public var plan: LaunchPlan?
    /// What its row says: the prompt, or the agent.
    public var title = ""
    /// The worktree path or Local id it starts in, when it isn't a new one.
    public var place: String?
    /// The link dropped mid-launch; it goes on once the Mac is back.
    public var waiting = false
    /// The agent it started.
    public var agentId: String?
    /// A progress sheet shows it, and opens its chat when it starts.
    public var watched = false

    /// The step running now, or the last one.
    public var current: String? { steps.last?.step }
    public var project: String? { plan?.spec.project }

    /// The agent.create it sends: the same spec under the same request id each time, as pocketd joins or replays a
    /// request it has seen only when the spec matches.
    func agentCreate(requestId: String) -> ClientMessage.AgentCreate? {
        guard var spec = plan?.spec else { return nil }
        if let madeTree { spec.checkout = .worktree(madeTree) }
        return .init(requestId: requestId, spec: spec)
    }

    /// pocketd answered with an error, and would give the same answer to the same request id for 10 minutes; trying
    /// again takes a new one, in the worktree a new-worktree start made before it failed.
    mutating func answered(_ error: ServerError) {
        self.error = error
        if case .new = plan?.spec.checkout, madeTree == nil { madeTree = cwd }
        requestId = WorkspaceStore.newRequestId()
        terminalId = nil
        cwd = nil
    }
}

/// A worktree or Local on its way out: hidden from the lists until it's gone, or back with why it couldn't go.
public struct Removal: Hashable, Sendable {
    public var id: String
    public var label: String
    public var local: Bool
    /// A worktree's branch, which `deleteBranch` deletes too.
    public var branch: String?
    public var deleteBranch = false
    public var force = false
    public var error: String?
    /// Deleting the branch was refused: this many commits are on it alone.
    public var unpushed: Int?

    public init(id: String, label: String, local: Bool, branch: String? = nil, deleteBranch: Bool = false, force: Bool = false) {
        self.id = id
        self.label = label
        self.local = local
        self.branch = branch
        self.deleteBranch = deleteBranch
        self.force = force
    }

    public var failed: Bool { error != nil || unpushed != nil }
}

/// Projects, worktrees, Locals, worktree names, automations, branches and accounts of one Mac.
@MainActor
@Observable
public final class WorkspaceStore: HostStore {
    public private(set) var projects: [Project] = []
    /// A project list has come since the store was made.
    public private(set) var loaded = false
    /// Display names by worktree path.
    public private(set) var names: [String: String] = [:]
    public private(set) var locals: [Local] = []
    /// Sent only to the owner.
    public private(set) var automations: Automations?
    /// The Local each terminal was opened in, from terminal.list; a session belongs to its terminal's Local.
    public private(set) var localOfTerminal: [String: String] = [:]
    /// The folders of terminals outside any Local, which keep a project's own checkout listed.
    public private(set) var terminalCwds: [String] = []
    /// Branches by tree, as last listed.
    public private(set) var branches: [String: BranchList] = [:]
    /// Accounts by provider, as last listed.
    public private(set) var accounts: [String: [Account]] = [:]
    public private(set) var providers: AgentProviders?
    /// Creations by request id, until they are seen started or dismissed.
    public private(set) var creations: [String: Creation] = [:]
    /// Worktrees and Locals being deleted, or that couldn't be, by path or id.
    public private(set) var removals: [String: Removal] = [:]
    /// What the Mac lets this phone do, from hello.ok.
    public private(set) var hello: HelloOk?

    @ObservationIgnored let connection: DaemonConnection?

    public init(connection: DaemonConnection? = nil) {
        self.connection = connection
    }

    public func apply(_ message: ServerMessage) -> Bool {
        switch message {
        case .projectList(let list):
            loaded = true
            // pocketd lists its projects after each hello, so a launch the link dropped goes on once they show what it made.
            for id in creations.keys where creations[id]?.waiting == true {
                creations[id]?.waiting = false
                Task { await run(id) }
            }
            return set(\.projects, list.projects)
        case .worktreeNames(let m):
            return set(\.names, m.names)
        case .localList(let m):
            return set(\.locals, m.locals)
        case .automations(let m):
            guard automations != m else { return false }
            automations = m
            return true
        case .terminalList(let m):
            var map: [String: String] = [:]
            for t in m.terminals { if let local = t.localId { map[t.id] = local } }
            let cwds = m.terminals.filter { $0.localId == nil }.map(\.cwd)
            let placed = set(\.localOfTerminal, map)
            return set(\.terminalCwds, cwds) || placed
        case .worktreeProgress(let p), .agentProgress(let p):
            guard let id = creation(sentAs: p.requestId) else { return false }
            creations[id]?.steps.append(.init(step: p.step, note: p.note))
            return true
        case .agentCreating(let m):
            guard let id = creation(sentAs: m.requestId) else { return false }
            creations[id]?.terminalId = m.terminalId
            creations[id]?.cwd = m.cwd
            return true
        case .agentProviders(let m):
            guard providers != m else { return false }
            providers = m
            return true
        case .agentList(let m):
            return started(m.agents)
        case .agentUpdate(let m):
            return started([m.agent])
        default:
            return false
        }
    }

    /// An older pocketd doesn't push its projects, so ask; a newer one sends them after hello.
    public func connected(_ hello: HelloOk) {
        self.hello = hello
        guard !hello.has(Caps.workspace) else { return }
        Task { try? await refreshProjects() }
    }

    /// The Mac lets this phone make and remove worktrees and Locals.
    public var canManage: Bool {
        guard let hello, hello.has(Caps.workspace) else { return false }
        return hello.scopes?.contains(.spawn) ?? true
    }

    /// A worktree's or Local's name as the sidebar shows it.
    public func label(project: Project, key: String) -> String {
        if let local = locals.first(where: { $0.id == key }) { return local.name }
        guard let tree = project.worktrees.first(where: { $0.path == key }) else { return key }
        if tree.isMain { return Sidebar.ownLocalName(given: names[tree.path]) }
        return Sidebar.treeLabel(path: tree.path, branch: tree.branch, given: names[tree.path]).label
    }

    public func project(_ path: String) -> Project? { projects.first { $0.path == path } }

    // MARK: Requests

    public func refreshProjects() async throws {
        _ = apply(try await request(.projectList))
    }

    @discardableResult
    public func loadProviders() async throws -> AgentProviders {
        guard case .agentProviders(let m) = try await request(.agentProviders) else { throw DaemonError.closed }
        _ = apply(.agentProviders(m))
        return m
    }

    @discardableResult
    public func loadBranches(tree: String) async throws -> BranchList {
        guard case .branchList(let list) = try await request(.branchList(.init(tree: tree))) else { throw DaemonError.closed }
        if branches[tree] != list { branches[tree] = list }
        return list
    }

    @discardableResult
    public func loadAccounts(provider: String) async throws -> [Account] {
        guard case .accountList(let list) = try await request(.accountList(.init(provider: provider)), timeout: .seconds(15)) else {
            throw DaemonError.closed
        }
        if accounts[provider] != list.accounts { accounts[provider] = list.accounts }
        return list.accounts
    }

    /// Makes a worktree; its steps land in `creations[requestId]` as pocketd reports them.
    public func createWorktree(_ create: ClientMessage.WorktreeCreate) async throws -> WorktreeCreated {
        creations[create.requestId] = Creation(kind: .worktree)
        do {
            guard case .worktreeCreated(let made) = try await request(.worktreeCreate(create), timeout: .seconds(600)) else {
                throw DaemonError.closed
            }
            creations[create.requestId]?.done = true
            return made
        } catch {
            creations[create.requestId]?.error = failure(error)
            throw error
        }
    }

    /// Deletes a worktree, and its branch when asked. Without `force`, a branch with commits nowhere else is refused:
    /// `unpushedCommits(error)` says how many.
    public func deleteWorktree(path: String, deleteBranch: Bool, force: Bool = false) async throws {
        _ = try await request(Self.deletion(path: path, deleteBranch: deleteBranch, force: force), timeout: .seconds(600))
    }

    nonisolated static func deletion(path: String, deleteBranch: Bool, force: Bool) -> ClientMessage {
        .worktreeDelete(.init(path: path, deleteBranch: deleteBranch ? true : nil, force: force ? true : nil))
    }

    /// How many commits a forced delete would lose, when `error` is the refusal that says so.
    public nonisolated static func unpushedCommits(_ error: any Error) -> Int? {
        guard case DaemonError.server(let e) = error, e.code == .unpushedCommits else { return nil }
        return e.count ?? 0
    }

    /// Names a worktree at once; a refusal puts the old name back.
    public func renameWorktree(path: String, title: String) async throws {
        let before = names[path]
        let title = title.trimmingCharacters(in: .whitespacesAndNewlines)
        names[path] = title.isEmpty ? nil : title
        do {
            _ = try await request(.worktreeRename(.init(path: path, title: title)))
        } catch {
            names[path] = before
            throw error
        }
    }

    /// Adds a Local to `project`, named after the first free number unless `name` is given. Shown at once; pocketd's
    /// list replaces it either way.
    @discardableResult
    public func createLocal(project: String, name: String? = nil) async throws -> Local {
        let local = Local(id: Sidebar.newLocalId(), project: project, name: name ?? nextLocalName(project: project))
        locals.append(local)
        do {
            _ = try await request(.localCreate(.init(localId: local.id, project: project, name: local.name)))
        } catch {
            locals.removeAll { $0.id == local.id }
            throw error
        }
        return local
    }

    /// A blank title changes nothing, as pocketd's rename.
    public func renameLocal(id: String, title: String) async throws {
        let title = title.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !title.isEmpty, let at = locals.firstIndex(where: { $0.id == id }) else { return }
        let before = locals[at]
        locals[at].name = title
        do {
            _ = try await request(.localRename(.init(localId: id, title: title)))
        } catch {
            if let at = locals.firstIndex(where: { $0.id == id }) { locals[at] = before }
            throw error
        }
    }

    public func deleteLocal(id: String) async throws {
        _ = try await request(.localDelete(.init(localId: id)))
        locals.removeAll { $0.id == id }
    }

    public func nextLocalName(project: String) -> String {
        Sidebar.nextLocalName(locals.filter { $0.project == project }.map(\.name))
    }

    /// Lists a launch at once, as a row under its project; `run` carries it out. `place` is the worktree path or Local id
    /// it starts in, when it isn't a new one.
    public func add(_ plan: LaunchPlan, requestId: String, title: String, place: String?) {
        creations[requestId] = Creation(kind: .session, plan: plan, title: title, place: place)
    }

    /// Runs what is left of a launch: its Local, then its worktree, then the agent in it, each step's progress landing in
    /// `creations[id]`. A dropped link leaves it waiting; once the Mac is back, a Local or worktree made by then is taken
    /// up rather than made twice, and agent.create goes out again as before, which pocketd joins or answers as it did
    /// the first. Trying again does the same, under a new request id when pocketd answered with an error.
    public func run(_ id: String) async {
        guard var c = creations[id], let plan = c.plan, !c.done else { return }
        c.error = nil
        c.waiting = false
        creations[id] = c
        let requestId = c.requestId ?? id
        let project = project(plan.spec.project)
        do {
            if let local = plan.local, c.madeLocal == nil {
                if !locals.contains(where: { $0.id == local.localId }) {
                    locals.append(Local(id: local.localId, project: local.project, name: local.name))
                    do {
                        _ = try await request(.localCreate(local))
                    } catch {
                        locals.removeAll { $0.id == local.localId }
                        throw error
                    }
                }
                creations[id]?.madeLocal = local.localId
            }
            if var create = plan.worktree, creations[id]?.madeTree == nil {
                if let made = create.branch == nil && create.pr == nil ? Self.landed(create.name, in: project) : nil {
                    creations[id]?.madeTree = made
                } else {
                    create.requestId = requestId
                    guard case .worktreeCreated(let made) = try await request(.worktreeCreate(create), timeout: .seconds(600)) else {
                        throw DaemonError.closed
                    }
                    creations[id]?.madeTree = made.path
                }
            }
            guard let now = creations[id], !now.done, let create = now.agentCreate(requestId: requestId) else { return }
            guard case .agentCreated(let created) = try await request(.agentCreate(create), timeout: .seconds(600)) else {
                throw DaemonError.closed
            }
            creations[id]?.agentId = created.agentId
            creations[id]?.done = true
        } catch DaemonError.closed, DaemonError.offline {
            creations[id]?.waiting = true
        } catch DaemonError.server(let e) {
            creations[id]?.answered(e)
        } catch {
            creations[id]?.error = failure(error)
        }
    }

    /// The worktree of `project` a new branch named `name` made: its folder takes the name, so a create whose reply a
    /// dropped link lost is found by it.
    nonisolated static func landed(_ name: String, in project: Project?) -> String? {
        project?.worktrees.first { !$0.isMain && $0.path.split(separator: "/").last.map(String.init) == name }?.path
    }

    /// The creation a request id now names.
    private func creation(sentAs requestId: String) -> String? {
        creations.first { ($0.value.requestId ?? $0.key) == requestId }?.key
    }

    /// An agent on the terminal a launch opened is the one it started, whether or not its reply came.
    private func started(_ agents: [AgentSummary]) -> Bool {
        var changed = false
        for (id, c) in creations where !c.done {
            guard let terminal = c.terminalId, let agent = agents.first(where: { $0.terminalId == terminal }) else { continue }
            creations[id]?.agentId = agent.id
            creations[id]?.done = true
            creations[id]?.waiting = false
            changed = true
        }
        return changed
    }

    /// Whether a progress sheet shows the creation, and so opens its chat once it starts.
    public func watch(_ requestId: String, _ on: Bool) {
        guard creations[requestId] != nil, creations[requestId]?.watched != on else { return }
        creations[requestId]?.watched = on
    }

    /// Launches that started while no sheet watched them, handed out once so one list says so.
    public func takeStarted() -> [Creation] {
        let started = creations.filter { $0.value.done && !$0.value.watched && $0.value.agentId != nil }
        for id in started.keys { creations[id] = nil }
        return started.values.sorted { $0.title < $1.title }
    }

    /// Dismisses a creation, or lets go of one a sheet saw through.
    public func forget(creation requestId: String) {
        creations[requestId] = nil
    }

    /// Deletes a worktree or Local, hiding it at once. A second ask while one is under way changes nothing; a failure
    /// brings the row back with why, for `takeFailedRemoval`.
    public func remove(_ removal: Removal) async {
        guard removals[removal.id].map(\.failed) ?? true else { return }
        var removal = removal
        removal.error = nil
        removal.unpushed = nil
        removals[removal.id] = removal
        do {
            if removal.local {
                try await deleteLocal(id: removal.id)
            } else {
                try await deleteWorktree(path: removal.id, deleteBranch: removal.deleteBranch, force: removal.force)
                // pocketd's next list drops it; until then it would come back.
                for i in projects.indices { projects[i].worktrees.removeAll { $0.path == removal.id } }
            }
            removals[removal.id] = nil
        } catch {
            removal.unpushed = Self.unpushedCommits(error)
            removal.error = removal.unpushed == nil ? failure(error)?.message : nil
            removals[removal.id] = removal.failed ? removal : nil
        }
    }

    /// The trees `remove` is deleting now, which the lists leave out.
    public var hidden: Set<String> { Set(removals.values.filter { !$0.failed }.map(\.id)) }

    /// A removal that failed, handed out once so one screen says so.
    public func takeFailedRemoval() -> Removal? {
        guard let failed = removals.values.first(where: \.failed) else { return nil }
        removals[failed.id] = nil
        return failed
    }

    public func switchAccount(agentId: String, account: String) async throws {
        _ = try await request(.agentAccount(.init(agentId: agentId, account: account)))
    }

    public func setAutomation(_ id: String, enabled: Bool) async throws {
        _ = try await request(.automationEnable(.init(automationId: id, enabled: enabled)))
    }

    public func runAutomation(_ id: String) async throws {
        _ = try await request(.automationRun(.init(automationId: id)))
    }

    public nonisolated static func newRequestId() -> String {
        "r-" + UUID().uuidString.prefix(13).lowercased()
    }

    private func request(_ message: ClientMessage, timeout: Duration? = nil) async throws -> ServerMessage {
        guard let connection else { throw DaemonError.offline }
        return try await connection.request(message, timeout: timeout)
    }

    private func set<T: Equatable>(_ key: ReferenceWritableKeyPath<WorkspaceStore, T>, _ value: T) -> Bool {
        guard self[keyPath: key] != value else { return false }
        self[keyPath: key] = value
        return true
    }

    /// A server's own error, with its detail; else what `ErrorText` says, naming the Mac. Nil when cancelled.
    func failure(_ error: any Error) -> ServerError? {
        if case DaemonError.server(let e) = error { return e }
        return ErrorText.describe(error, mac: hello?.hostname ?? "your Mac").map { ServerError(message: $0) }
    }
}
