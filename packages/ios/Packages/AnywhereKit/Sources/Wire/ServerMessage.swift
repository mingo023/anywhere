/// Everything pocketd sends as a text frame, keyed by `type`. Payloads hold neither `type` nor the reply `id`;
/// `ServerFrame` carries the id.
public enum ServerMessage: Codable, Hashable, Sendable {
    case helloOk(HelloOk)
    case pairOk(PairOk)
    case pairOffer(PairOffer)
    case pairDone(PairDone)
    case agentList(AgentList)
    case agentUpdate(AgentUpdate)
    case agentCommands(AgentCommands)
    case agentMcp(AgentMcp)
    case agentBtw(AgentBtwAnswer)
    case agentStream(AgentStream)
    case agentTimeline(AgentTimeline)
    case permissionRequest(PermissionAsk)
    case permissionResolved(PermissionResolved)
    case ack
    case agentCreating(AgentCreating)
    case agentProgress(LaunchProgress)
    case agentCreated(AgentCreated)
    case agentProviders(AgentProviders)
    case error(ServerError)
    case hostChanged(HostChanged)
    case worktreeNames(WorktreeNames)
    case namingFailed(NamingFailed)
    case localList(LocalList)
    case automations(Automations)
    case projectList(ProjectList)
    case terminalList(TerminalList)
    case terminalAttached(TerminalAttached)
    case terminalResized(TerminalResized)
    case terminalExited(TerminalExited)
    case terminalCreated(TerminalCreated)
    case worktreeProgress(LaunchProgress)
    case worktreeCreated(WorktreeCreated)
    case branchList(BranchList)
    case accountList(AccountList)
    case gitStatus(GitStatus)
    case gitChanged(GitChanged)
    case ghChanged(GhChanged)
    case gitDiff(GitDiff)
    case gitLog(GitLog)
    case gitCommit(CommitDetail)
    case gitCommitted(GitCommitted)
    case ghStatus(GhStatus)
    case ghPr(GhPr)
    case ghPrs(GhPrs)
    case ghPrCreated(GhPrCreated)
    case fsList(FsList)
    case fsRead(FsRead)
    case fsSearch(FsSearch)
    case attachDone(AttachDone)
    /// A message this build doesn't know, from a newer pocketd; ignored.
    case unknown(String)

    public init(from decoder: any Decoder) throws {
        switch try decoder.container(keyedBy: TypeKey.self).decode(String.self, forKey: .type) {
        case "hello.ok": self = .helloOk(try HelloOk(from: decoder))
        case "pair.ok": self = .pairOk(try PairOk(from: decoder))
        case "pair.offer": self = .pairOffer(try PairOffer(from: decoder))
        case "pair.done": self = .pairDone(try PairDone(from: decoder))
        case "agent.list": self = .agentList(try AgentList(from: decoder))
        case "agent.update": self = .agentUpdate(try AgentUpdate(from: decoder))
        case "agent.commands": self = .agentCommands(try AgentCommands(from: decoder))
        case "agent.mcp": self = .agentMcp(try AgentMcp(from: decoder))
        case "agent.btw": self = .agentBtw(try AgentBtwAnswer(from: decoder))
        case "agent.stream": self = .agentStream(try AgentStream(from: decoder))
        case "agent.timeline": self = .agentTimeline(try AgentTimeline(from: decoder))
        case "permission.request": self = .permissionRequest(try PermissionAsk(from: decoder))
        case "permission.resolved": self = .permissionResolved(try PermissionResolved(from: decoder))
        case "ack": self = .ack
        case "agent.creating": self = .agentCreating(try AgentCreating(from: decoder))
        case "agent.progress": self = .agentProgress(try LaunchProgress(from: decoder))
        case "agent.created": self = .agentCreated(try AgentCreated(from: decoder))
        case "agent.providers": self = .agentProviders(try AgentProviders(from: decoder))
        case "error": self = .error(try ServerError(from: decoder))
        case "host.changed": self = .hostChanged(try HostChanged(from: decoder))
        case "worktree.names": self = .worktreeNames(try WorktreeNames(from: decoder))
        case "naming.failed": self = .namingFailed(try NamingFailed(from: decoder))
        case "local.list": self = .localList(try LocalList(from: decoder))
        case "automations": self = .automations(try Automations(from: decoder))
        case "project.list": self = .projectList(try ProjectList(from: decoder))
        case "terminal.list": self = .terminalList(try TerminalList(from: decoder))
        case "terminal.attached": self = .terminalAttached(try TerminalAttached(from: decoder))
        case "terminal.resized": self = .terminalResized(try TerminalResized(from: decoder))
        case "terminal.exited": self = .terminalExited(try TerminalExited(from: decoder))
        case "terminal.created": self = .terminalCreated(try TerminalCreated(from: decoder))
        case "worktree.progress": self = .worktreeProgress(try LaunchProgress(from: decoder))
        case "worktree.created": self = .worktreeCreated(try WorktreeCreated(from: decoder))
        case "branch.list": self = .branchList(try BranchList(from: decoder))
        case "account.list": self = .accountList(try AccountList(from: decoder))
        case "git.status": self = .gitStatus(try GitStatus(from: decoder))
        case "git.changed": self = .gitChanged(try GitChanged(from: decoder))
        case "gh.changed": self = .ghChanged(try GhChanged(from: decoder))
        case "git.diff": self = .gitDiff(try GitDiff(from: decoder))
        case "git.log": self = .gitLog(try GitLog(from: decoder))
        case "git.commit": self = .gitCommit(try CommitDetail(from: decoder))
        case "git.committed": self = .gitCommitted(try GitCommitted(from: decoder))
        case "gh.status": self = .ghStatus(try GhStatus(from: decoder))
        case "gh.pr": self = .ghPr(try GhPr(from: decoder))
        case "gh.prs": self = .ghPrs(try GhPrs(from: decoder))
        case "gh.pr.created": self = .ghPrCreated(try GhPrCreated(from: decoder))
        case "fs.list": self = .fsList(try FsList(from: decoder))
        case "fs.read": self = .fsRead(try FsRead(from: decoder))
        case "fs.search": self = .fsSearch(try FsSearch(from: decoder))
        case "attach.done": self = .attachDone(try AttachDone(from: decoder))
        case let type: self = .unknown(type)
        }
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: TypeKey.self)
        try c.encode(type, forKey: .type)
        switch self {
        case .helloOk(let m): try m.encode(to: encoder)
        case .pairOk(let m): try m.encode(to: encoder)
        case .pairOffer(let m): try m.encode(to: encoder)
        case .pairDone(let m): try m.encode(to: encoder)
        case .agentList(let m): try m.encode(to: encoder)
        case .agentUpdate(let m): try m.encode(to: encoder)
        case .agentCommands(let m): try m.encode(to: encoder)
        case .agentMcp(let m): try m.encode(to: encoder)
        case .agentBtw(let m): try m.encode(to: encoder)
        case .agentStream(let m): try m.encode(to: encoder)
        case .agentTimeline(let m): try m.encode(to: encoder)
        case .permissionRequest(let m): try m.encode(to: encoder)
        case .permissionResolved(let m): try m.encode(to: encoder)
        case .ack: break
        case .agentCreating(let m): try m.encode(to: encoder)
        case .agentProgress(let m): try m.encode(to: encoder)
        case .agentCreated(let m): try m.encode(to: encoder)
        case .agentProviders(let m): try m.encode(to: encoder)
        case .error(let m): try m.encode(to: encoder)
        case .hostChanged(let m): try m.encode(to: encoder)
        case .worktreeNames(let m): try m.encode(to: encoder)
        case .namingFailed(let m): try m.encode(to: encoder)
        case .localList(let m): try m.encode(to: encoder)
        case .automations(let m): try m.encode(to: encoder)
        case .projectList(let m): try m.encode(to: encoder)
        case .terminalList(let m): try m.encode(to: encoder)
        case .terminalAttached(let m): try m.encode(to: encoder)
        case .terminalResized(let m): try m.encode(to: encoder)
        case .terminalExited(let m): try m.encode(to: encoder)
        case .terminalCreated(let m): try m.encode(to: encoder)
        case .worktreeProgress(let m): try m.encode(to: encoder)
        case .worktreeCreated(let m): try m.encode(to: encoder)
        case .branchList(let m): try m.encode(to: encoder)
        case .accountList(let m): try m.encode(to: encoder)
        case .gitStatus(let m): try m.encode(to: encoder)
        case .gitChanged(let m): try m.encode(to: encoder)
        case .ghChanged(let m): try m.encode(to: encoder)
        case .gitDiff(let m): try m.encode(to: encoder)
        case .gitLog(let m): try m.encode(to: encoder)
        case .gitCommit(let m): try m.encode(to: encoder)
        case .gitCommitted(let m): try m.encode(to: encoder)
        case .ghStatus(let m): try m.encode(to: encoder)
        case .ghPr(let m): try m.encode(to: encoder)
        case .ghPrs(let m): try m.encode(to: encoder)
        case .ghPrCreated(let m): try m.encode(to: encoder)
        case .fsList(let m): try m.encode(to: encoder)
        case .fsRead(let m): try m.encode(to: encoder)
        case .fsSearch(let m): try m.encode(to: encoder)
        case .attachDone(let m): try m.encode(to: encoder)
        case .unknown: break
        }
    }

    public var type: String {
        switch self {
        case .helloOk: "hello.ok"
        case .pairOk: "pair.ok"
        case .pairOffer: "pair.offer"
        case .pairDone: "pair.done"
        case .agentList: "agent.list"
        case .agentUpdate: "agent.update"
        case .agentCommands: "agent.commands"
        case .agentMcp: "agent.mcp"
        case .agentBtw: "agent.btw"
        case .agentStream: "agent.stream"
        case .agentTimeline: "agent.timeline"
        case .permissionRequest: "permission.request"
        case .permissionResolved: "permission.resolved"
        case .ack: "ack"
        case .agentCreating: "agent.creating"
        case .agentProgress: "agent.progress"
        case .agentCreated: "agent.created"
        case .agentProviders: "agent.providers"
        case .error: "error"
        case .hostChanged: "host.changed"
        case .worktreeNames: "worktree.names"
        case .namingFailed: "naming.failed"
        case .localList: "local.list"
        case .automations: "automations"
        case .projectList: "project.list"
        case .terminalList: "terminal.list"
        case .terminalAttached: "terminal.attached"
        case .terminalResized: "terminal.resized"
        case .terminalExited: "terminal.exited"
        case .terminalCreated: "terminal.created"
        case .worktreeProgress: "worktree.progress"
        case .worktreeCreated: "worktree.created"
        case .branchList: "branch.list"
        case .accountList: "account.list"
        case .gitStatus: "git.status"
        case .gitChanged: "git.changed"
        case .ghChanged: "gh.changed"
        case .gitDiff: "git.diff"
        case .gitLog: "git.log"
        case .gitCommit: "git.commit"
        case .gitCommitted: "git.committed"
        case .ghStatus: "gh.status"
        case .ghPr: "gh.pr"
        case .ghPrs: "gh.prs"
        case .ghPrCreated: "gh.pr.created"
        case .fsList: "fs.list"
        case .fsRead: "fs.read"
        case .fsSearch: "fs.search"
        case .attachDone: "attach.done"
        case .unknown(let type): type
        }
    }
}
