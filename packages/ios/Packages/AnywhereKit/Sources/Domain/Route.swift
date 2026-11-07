/// A screen, as a value features navigate with without knowing each other. `mac` is the Mac's `hello.ok` serverId.
public enum Route: Hashable, Codable, Sendable {
    case sessions(mac: String)
    case project(mac: String, path: String)
    case worktree(mac: String, path: String)
    case local(mac: String, id: String)
    case chat(mac: String, agentId: String)
    /// `place` is a worktree's path or a Local's id, as `.worktree` and `.local` carry them; `prompt` fills the draft.
    case newSession(mac: String, project: String?, place: String? = nil, prompt: String? = nil)
    case terminals(mac: String)
    case terminal(mac: String, terminalId: String)
    case changes(mac: String, tree: String)
    case diff(mac: String, tree: String, path: String, staged: Bool)
    case history(mac: String, tree: String)
    case commit(mac: String, tree: String, sha: String)
    case branches(mac: String, tree: String)
    case pullRequest(mac: String, tree: String)
    case pullRequests(mac: String, project: String)
    /// The review threads of the tree's pull request.
    case reviewThreads(mac: String, tree: String)
    /// `chat` is the agent whose chat opened Files, which takes its mentions and quotes.
    case files(mac: String, tree: String, dir: String, chat: String? = nil)
    /// `line` is 1-based, for a link that names one.
    case file(mac: String, tree: String, path: String, line: Int? = nil, chat: String? = nil)
    case automations(mac: String)
    case pairing
    case settings

    /// The Mac a route is on; pairing and settings belong to none.
    public var mac: String? {
        switch self {
        case .sessions(let mac), .project(let mac, _), .worktree(let mac, _), .local(let mac, _), .chat(let mac, _), .newSession(let mac, _, _, _),
            .terminals(let mac), .terminal(let mac, _), .changes(let mac, _), .diff(let mac, _, _, _), .history(let mac, _),
            .commit(let mac, _, _), .branches(let mac, _), .pullRequest(let mac, _), .pullRequests(let mac, _), .files(let mac, _, _, _),
            .reviewThreads(let mac, _), .file(let mac, _, _, _, _), .automations(let mac):
            mac
        case .pairing, .settings:
            nil
        }
    }
}
