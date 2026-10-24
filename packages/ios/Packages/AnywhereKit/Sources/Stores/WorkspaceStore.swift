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
    /// The terminal an agent.create opened, once it exists.
    public var terminalId: String?
    public var error: ServerError?
    public var done = false
    /// The Local and worktree a launch made before a later step failed, so trying again starts there.
    public var madeLocal: String?
    public var madeTree: String?

    /// The step running now, or the last one.
    public var current: String? { steps.last?.step }
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
    /// Creations by request id, while their sheet watches them.
    public private(set) var creations: [String: Creation] = [:]
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
            guard var c = creations[p.requestId] else { return false }
            c.steps.append(.init(step: p.step, note: p.note))
            creations[p.requestId] = c
            return true
        case .agentCreating(let m):
            guard creations[m.requestId] != nil else { return false }
            creations[m.requestId]?.terminalId = m.terminalId
            return true
        case .agentProviders(let m):
            guard providers != m else { return false }
            providers = m
            return true
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
            creations[create.requestId]?.error = Self.serverError(error)
            throw error
        }
    }

    public func deleteWorktree(path: String, deleteBranch: Bool) async throws {
        _ = try await request(.worktreeDelete(.init(path: path, deleteBranch: deleteBranch ? true : nil)), timeout: .seconds(600))
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

    /// Runs a launch plan: its Local, then its worktree, then the agent in it. Each step's progress lands in
    /// `creations[requestId]`.
    public func launch(_ plan: LaunchPlan, requestId: String) async throws -> AgentCreated {
        creations[requestId] = Creation(kind: .session)
        do {
            if let local = plan.local {
                let shown = !locals.contains(where: { $0.id == local.localId })
                if shown { locals.append(Local(id: local.localId, project: local.project, name: local.name)) }
                do {
                    _ = try await request(.localCreate(local))
                } catch {
                    if shown { locals.removeAll { $0.id == local.localId } }
                    throw error
                }
                creations[requestId]?.madeLocal = local.localId
            }
            var spec = plan.spec
            if var create = plan.worktree {
                create.requestId = requestId
                guard case .worktreeCreated(let made) = try await request(.worktreeCreate(create), timeout: .seconds(600)) else {
                    throw DaemonError.closed
                }
                creations[requestId]?.madeTree = made.path
                spec.checkout = .worktree(made.path)
            }
            guard case .agentCreated(let created) = try await request(.agentCreate(.init(requestId: requestId, spec: spec)), timeout: .seconds(600))
            else { throw DaemonError.closed }
            creations[requestId]?.done = true
            return created
        } catch {
            creations[requestId]?.error = Self.serverError(error)
            throw error
        }
    }

    /// A sheet that stopped watching a creation lets it go.
    public func forget(creation requestId: String) {
        creations[requestId] = nil
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

    public static func newRequestId() -> String {
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

    public static func serverError(_ error: any Error) -> ServerError {
        if case DaemonError.server(let e) = error { return e }
        switch error as? DaemonError {
        case .offline: return ServerError(message: "This Mac isn't connected")
        case .timedOut: return ServerError(message: "Your Mac didn't answer in time")
        case .closed: return ServerError(message: "The connection dropped")
        default: return ServerError(message: error.localizedDescription)
        }
    }
}
