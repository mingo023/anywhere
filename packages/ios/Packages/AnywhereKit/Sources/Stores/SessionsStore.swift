import Client
import Domain
import Observation
import Wire

/// Every agent on one Mac, in Up next and recency order, with the asks they wait on.
@MainActor
@Observable
public final class SessionsStore: HostStore {
    public private(set) var sessions: [String: SessionModel] = [:]
    /// Sessions that want a look, in the order to handle them.
    public private(set) var upNext: [String] = []
    /// Every session, newest first.
    public private(set) var recent: [String] = []
    /// Open asks by request id.
    public private(set) var requests: [String: PermissionRequest] = [:]

    @ObservationIgnored let connection: DaemonConnection?

    public init(connection: DaemonConnection? = nil) {
        self.connection = connection
    }

    public func apply(_ message: ServerMessage) -> Bool {
        switch message {
        case .agentList(let list):
            return replace(list.agents)
        case .agentUpdate(let update):
            return upsert(update.agent)
        case .permissionRequest(let ask):
            return add(ask.request)
        case .permissionResolved(let resolved):
            return resolve(resolved.requestId)
        case .namingFailed(let failed):
            guard let session = sessions[failed.agentId], !session.namingFailed else { return false }
            session.namingFailed = true
            return true
        default:
            return false
        }
    }

    /// pocketd re-sends every open ask after hello, so held ones may have been answered meanwhile.
    public func connected(_ hello: HelloOk) {
        guard !requests.isEmpty else { return }
        requests = [:]
        for session in sessions.values where !session.requests.isEmpty { session.requests = [] }
    }

    public func session(_ id: String) -> SessionModel? { sessions[id] }

    /// Asks pocketd for every agent again; the reply replaces the list.
    public func refresh() async throws {
        guard let connection else { throw DaemonError.offline }
        _ = apply(try await connection.request(.agentList))
    }

    public func pin(_ id: String, _ pinned: Bool) async throws {
        guard let connection else { throw DaemonError.offline }
        _ = try await connection.request(.agentPin(.init(agentId: id, pinned: pinned)))
    }

    /// Ends the agent and its terminal.
    public func close(_ id: String) async throws {
        guard let connection else { throw DaemonError.offline }
        _ = try await connection.request(.agentClose(.init(agentId: id)))
    }

    private func replace(_ agents: [AgentSummary]) -> Bool {
        var changed = false
        var moved = false
        let ids = Set(agents.map(\.id))
        for id in sessions.keys where !ids.contains(id) {
            sessions[id] = nil
            changed = true
            moved = true
        }
        for agent in agents where agent.status != .closed {
            let (c, m) = put(agent)
            changed = changed || c
            moved = moved || m
        }
        for agent in agents where agent.status == .closed && sessions[agent.id] != nil {
            sessions[agent.id] = nil
            changed = true
            moved = true
        }
        dropOrphanedRequests()
        if moved { reorder() }
        return changed
    }

    private func upsert(_ agent: AgentSummary) -> Bool {
        if agent.status == .closed {
            guard sessions.removeValue(forKey: agent.id) != nil else { return false }
            dropOrphanedRequests()
            reorder()
            return true
        }
        let (changed, moved) = put(agent)
        if moved { reorder() }
        return changed
    }

    private func put(_ agent: AgentSummary) -> (changed: Bool, moved: Bool) {
        if let session = sessions[agent.id] {
            let before = session.summary
            let moved = session.update(agent)
            return (before != agent, moved)
        }
        let session = SessionModel(agent)
        session.requests = requests.values.filter { $0.agentId == agent.id }.sorted { $0.requestId < $1.requestId }
        sessions[agent.id] = session
        return (true, true)
    }

    private func add(_ request: PermissionRequest) -> Bool {
        guard requests[request.requestId] != request else { return false }
        requests[request.requestId] = request
        if let session = sessions[request.agentId] {
            if let at = session.requests.firstIndex(where: { $0.requestId == request.requestId }) {
                session.requests[at] = request
            } else {
                session.requests.append(request)
            }
        }
        return true
    }

    private func resolve(_ requestId: String) -> Bool {
        guard let request = requests.removeValue(forKey: requestId) else { return false }
        sessions[request.agentId]?.requests.removeAll { $0.requestId == requestId }
        return true
    }

    private func dropOrphanedRequests() {
        for (id, request) in requests where sessions[request.agentId] == nil { requests[id] = nil }
    }

    private func reorder() {
        let ranked = sessions.values.map(\.ranked)
        let upNext = Order.upNext(ranked)
        let recent = Order.recent(ranked)
        if upNext != self.upNext { self.upNext = upNext }
        if recent != self.recent { self.recent = recent }
    }
}
