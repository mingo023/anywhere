/// Everything the phone sends, keyed by `type`. Payloads hold neither `type` nor `id`; `ClientFrame` carries the id.
public enum ClientMessage: Codable, Hashable, Sendable {
    case hello(ClientMessage.Hello)
    case pair(ClientMessage.Pair)
    case agentList
    case pairBegin
    case agentPrompt(ClientMessage.AgentText)
    case agentInterrupt(ClientMessage.AgentRef)
    case agentSendNow(ClientMessage.AgentRef)
    case agentCompact(ClientMessage.AgentRef)
    case agentClose(ClientMessage.AgentRef)
    case agentCommands(ClientMessage.AgentRef)
    case agentMcp(ClientMessage.AgentMcp)
    case agentBtw(ClientMessage.AgentText)
    case agentSettings(ClientMessage.AgentSettings)
    case agentPin(ClientMessage.AgentPin)
    case agentView(ClientMessage.AgentIds)
    case agentSeen(ClientMessage.AgentIds)
    case agentTimeline(ClientMessage.AgentTimeline)
    case permissionResolve(ClientMessage.PermissionResolve)
    case projectList
    case agentCreate(ClientMessage.AgentCreate)
    case agentProviders
    case configSet(ClientMessage.ConfigSet)
    case worktreeRename(ClientMessage.WorktreeRename)
    case localCreate(ClientMessage.LocalCreate)
    case localRename(ClientMessage.LocalRename)
    case localDelete(ClientMessage.LocalRef)
    case automationSave(ClientMessage.AutomationSave)
    case automationEnable(ClientMessage.AutomationEnable)
    case automationDelete(ClientMessage.AutomationRef)
    case automationRun(ClientMessage.AutomationRef)
    case terminalList
    case terminalAttach(ClientMessage.TerminalRef)
    case terminalDetach(ClientMessage.TerminalRef)
    case terminalInput(ClientMessage.TerminalInput)
    case terminalResize(ClientMessage.TerminalResize)
    case terminalCreate(ClientMessage.TerminalCreate)
    case terminalClose(ClientMessage.TerminalRef)
    case worktreeCreate(ClientMessage.WorktreeCreate)
    case worktreeDelete(ClientMessage.WorktreeDelete)
    case branchList(ClientMessage.TreeRef)
    case accountList(ClientMessage.AccountList)
    case agentAccount(ClientMessage.AgentAccount)
    case gitStatus(ClientMessage.GitStatus)
    case gitWatch(ClientMessage.GitWatch)
    case gitDiff(ClientMessage.GitDiff)
    case gitLog(ClientMessage.GitLog)
    case gitCommit(ClientMessage.GitCommit)
    case gitStage(ClientMessage.GitStage)
    case gitDiscard(ClientMessage.GitDiscard)
    case gitCommitCreate(ClientMessage.GitCommitCreate)
    case gitPush(ClientMessage.GitPush)
    case gitPull(ClientMessage.TreeRef)
    case ghStatus(ClientMessage.GhStatus)
    case ghPr(ClientMessage.TreeRef)
    case ghPrs(ClientMessage.GhPrs)
    case ghPrCreate(ClientMessage.GhPrCreate)
    case ghPrMerge(ClientMessage.GhPrMerge)
    case ghPrReady(ClientMessage.GhPrReady)
    case ghThreadReply(ClientMessage.GhThreadReply)
    case ghThreadResolve(ClientMessage.GhThreadResolve)
    case fsList(ClientMessage.FsList)
    case fsRead(ClientMessage.FsRead)
    case fsSearch(ClientMessage.FsSearch)
    case attachPut(ClientMessage.AttachPut)
    case pushRegister(ClientMessage.PushRegister)
    case pushActivity(ClientMessage.PushActivity)
    case pushPrefs(ClientMessage.PushPrefsUpdate)
    case pushUnregister
    case pushTest

    public init(from decoder: any Decoder) throws {
        switch try decoder.container(keyedBy: TypeKey.self).decode(String.self, forKey: .type) {
        case "hello": self = .hello(try ClientMessage.Hello(from: decoder))
        case "pair": self = .pair(try ClientMessage.Pair(from: decoder))
        case "agent.list": self = .agentList
        case "pair.begin": self = .pairBegin
        case "agent.prompt": self = .agentPrompt(try ClientMessage.AgentText(from: decoder))
        case "agent.interrupt": self = .agentInterrupt(try ClientMessage.AgentRef(from: decoder))
        case "agent.send_now": self = .agentSendNow(try ClientMessage.AgentRef(from: decoder))
        case "agent.compact": self = .agentCompact(try ClientMessage.AgentRef(from: decoder))
        case "agent.close": self = .agentClose(try ClientMessage.AgentRef(from: decoder))
        case "agent.commands": self = .agentCommands(try ClientMessage.AgentRef(from: decoder))
        case "agent.mcp": self = .agentMcp(try ClientMessage.AgentMcp(from: decoder))
        case "agent.btw": self = .agentBtw(try ClientMessage.AgentText(from: decoder))
        case "agent.settings": self = .agentSettings(try ClientMessage.AgentSettings(from: decoder))
        case "agent.pin": self = .agentPin(try ClientMessage.AgentPin(from: decoder))
        case "agent.view": self = .agentView(try ClientMessage.AgentIds(from: decoder))
        case "agent.seen": self = .agentSeen(try ClientMessage.AgentIds(from: decoder))
        case "agent.timeline": self = .agentTimeline(try ClientMessage.AgentTimeline(from: decoder))
        case "permission.resolve": self = .permissionResolve(try ClientMessage.PermissionResolve(from: decoder))
        case "project.list": self = .projectList
        case "agent.create": self = .agentCreate(try ClientMessage.AgentCreate(from: decoder))
        case "agent.providers": self = .agentProviders
        case "config.set": self = .configSet(try ClientMessage.ConfigSet(from: decoder))
        case "worktree.rename": self = .worktreeRename(try ClientMessage.WorktreeRename(from: decoder))
        case "local.create": self = .localCreate(try ClientMessage.LocalCreate(from: decoder))
        case "local.rename": self = .localRename(try ClientMessage.LocalRename(from: decoder))
        case "local.delete": self = .localDelete(try ClientMessage.LocalRef(from: decoder))
        case "automation.save": self = .automationSave(try ClientMessage.AutomationSave(from: decoder))
        case "automation.enable": self = .automationEnable(try ClientMessage.AutomationEnable(from: decoder))
        case "automation.delete": self = .automationDelete(try ClientMessage.AutomationRef(from: decoder))
        case "automation.run": self = .automationRun(try ClientMessage.AutomationRef(from: decoder))
        case "terminal.list": self = .terminalList
        case "terminal.attach": self = .terminalAttach(try ClientMessage.TerminalRef(from: decoder))
        case "terminal.detach": self = .terminalDetach(try ClientMessage.TerminalRef(from: decoder))
        case "terminal.input": self = .terminalInput(try ClientMessage.TerminalInput(from: decoder))
        case "terminal.resize": self = .terminalResize(try ClientMessage.TerminalResize(from: decoder))
        case "terminal.create": self = .terminalCreate(try ClientMessage.TerminalCreate(from: decoder))
        case "terminal.close": self = .terminalClose(try ClientMessage.TerminalRef(from: decoder))
        case "worktree.create": self = .worktreeCreate(try ClientMessage.WorktreeCreate(from: decoder))
        case "worktree.delete": self = .worktreeDelete(try ClientMessage.WorktreeDelete(from: decoder))
        case "branch.list": self = .branchList(try ClientMessage.TreeRef(from: decoder))
        case "account.list": self = .accountList(try ClientMessage.AccountList(from: decoder))
        case "agent.account": self = .agentAccount(try ClientMessage.AgentAccount(from: decoder))
        case "git.status": self = .gitStatus(try ClientMessage.GitStatus(from: decoder))
        case "git.watch": self = .gitWatch(try ClientMessage.GitWatch(from: decoder))
        case "git.diff": self = .gitDiff(try ClientMessage.GitDiff(from: decoder))
        case "git.log": self = .gitLog(try ClientMessage.GitLog(from: decoder))
        case "git.commit": self = .gitCommit(try ClientMessage.GitCommit(from: decoder))
        case "git.stage": self = .gitStage(try ClientMessage.GitStage(from: decoder))
        case "git.discard": self = .gitDiscard(try ClientMessage.GitDiscard(from: decoder))
        case "git.commit.create": self = .gitCommitCreate(try ClientMessage.GitCommitCreate(from: decoder))
        case "git.push": self = .gitPush(try ClientMessage.GitPush(from: decoder))
        case "git.pull": self = .gitPull(try ClientMessage.TreeRef(from: decoder))
        case "gh.status": self = .ghStatus(try ClientMessage.GhStatus(from: decoder))
        case "gh.pr": self = .ghPr(try ClientMessage.TreeRef(from: decoder))
        case "gh.prs": self = .ghPrs(try ClientMessage.GhPrs(from: decoder))
        case "gh.pr.create": self = .ghPrCreate(try ClientMessage.GhPrCreate(from: decoder))
        case "gh.pr.merge": self = .ghPrMerge(try ClientMessage.GhPrMerge(from: decoder))
        case "gh.pr.ready": self = .ghPrReady(try ClientMessage.GhPrReady(from: decoder))
        case "gh.thread.reply": self = .ghThreadReply(try ClientMessage.GhThreadReply(from: decoder))
        case "gh.thread.resolve": self = .ghThreadResolve(try ClientMessage.GhThreadResolve(from: decoder))
        case "fs.list": self = .fsList(try ClientMessage.FsList(from: decoder))
        case "fs.read": self = .fsRead(try ClientMessage.FsRead(from: decoder))
        case "fs.search": self = .fsSearch(try ClientMessage.FsSearch(from: decoder))
        case "attach.put": self = .attachPut(try ClientMessage.AttachPut(from: decoder))
        case "push.register": self = .pushRegister(try ClientMessage.PushRegister(from: decoder))
        case "push.activity": self = .pushActivity(try ClientMessage.PushActivity(from: decoder))
        case "push.prefs": self = .pushPrefs(try ClientMessage.PushPrefsUpdate(from: decoder))
        case "push.unregister": self = .pushUnregister
        case "push.test": self = .pushTest
        case let type: throw malformed(decoder, "unknown message type \(type)")
        }
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: TypeKey.self)
        try c.encode(type, forKey: .type)
        switch self {
        case .hello(let m): try m.encode(to: encoder)
        case .pair(let m): try m.encode(to: encoder)
        case .agentList: break
        case .pairBegin: break
        case .agentPrompt(let m): try m.encode(to: encoder)
        case .agentInterrupt(let m): try m.encode(to: encoder)
        case .agentSendNow(let m): try m.encode(to: encoder)
        case .agentCompact(let m): try m.encode(to: encoder)
        case .agentClose(let m): try m.encode(to: encoder)
        case .agentCommands(let m): try m.encode(to: encoder)
        case .agentMcp(let m): try m.encode(to: encoder)
        case .agentBtw(let m): try m.encode(to: encoder)
        case .agentSettings(let m): try m.encode(to: encoder)
        case .agentPin(let m): try m.encode(to: encoder)
        case .agentView(let m): try m.encode(to: encoder)
        case .agentSeen(let m): try m.encode(to: encoder)
        case .agentTimeline(let m): try m.encode(to: encoder)
        case .permissionResolve(let m): try m.encode(to: encoder)
        case .projectList: break
        case .agentCreate(let m): try m.encode(to: encoder)
        case .agentProviders: break
        case .configSet(let m): try m.encode(to: encoder)
        case .worktreeRename(let m): try m.encode(to: encoder)
        case .localCreate(let m): try m.encode(to: encoder)
        case .localRename(let m): try m.encode(to: encoder)
        case .localDelete(let m): try m.encode(to: encoder)
        case .automationSave(let m): try m.encode(to: encoder)
        case .automationEnable(let m): try m.encode(to: encoder)
        case .automationDelete(let m): try m.encode(to: encoder)
        case .automationRun(let m): try m.encode(to: encoder)
        case .terminalList: break
        case .terminalAttach(let m): try m.encode(to: encoder)
        case .terminalDetach(let m): try m.encode(to: encoder)
        case .terminalInput(let m): try m.encode(to: encoder)
        case .terminalResize(let m): try m.encode(to: encoder)
        case .terminalCreate(let m): try m.encode(to: encoder)
        case .terminalClose(let m): try m.encode(to: encoder)
        case .worktreeCreate(let m): try m.encode(to: encoder)
        case .worktreeDelete(let m): try m.encode(to: encoder)
        case .branchList(let m): try m.encode(to: encoder)
        case .accountList(let m): try m.encode(to: encoder)
        case .agentAccount(let m): try m.encode(to: encoder)
        case .gitStatus(let m): try m.encode(to: encoder)
        case .gitWatch(let m): try m.encode(to: encoder)
        case .gitDiff(let m): try m.encode(to: encoder)
        case .gitLog(let m): try m.encode(to: encoder)
        case .gitCommit(let m): try m.encode(to: encoder)
        case .gitStage(let m): try m.encode(to: encoder)
        case .gitDiscard(let m): try m.encode(to: encoder)
        case .gitCommitCreate(let m): try m.encode(to: encoder)
        case .gitPush(let m): try m.encode(to: encoder)
        case .gitPull(let m): try m.encode(to: encoder)
        case .ghStatus(let m): try m.encode(to: encoder)
        case .ghPr(let m): try m.encode(to: encoder)
        case .ghPrs(let m): try m.encode(to: encoder)
        case .ghPrCreate(let m): try m.encode(to: encoder)
        case .ghPrMerge(let m): try m.encode(to: encoder)
        case .ghPrReady(let m): try m.encode(to: encoder)
        case .ghThreadReply(let m): try m.encode(to: encoder)
        case .ghThreadResolve(let m): try m.encode(to: encoder)
        case .fsList(let m): try m.encode(to: encoder)
        case .fsRead(let m): try m.encode(to: encoder)
        case .fsSearch(let m): try m.encode(to: encoder)
        case .attachPut(let m): try m.encode(to: encoder)
        case .pushRegister(let m): try m.encode(to: encoder)
        case .pushActivity(let m): try m.encode(to: encoder)
        case .pushPrefs(let m): try m.encode(to: encoder)
        case .pushUnregister: break
        case .pushTest: break
        }
    }

    public var type: String {
        switch self {
        case .hello: "hello"
        case .pair: "pair"
        case .agentList: "agent.list"
        case .pairBegin: "pair.begin"
        case .agentPrompt: "agent.prompt"
        case .agentInterrupt: "agent.interrupt"
        case .agentSendNow: "agent.send_now"
        case .agentCompact: "agent.compact"
        case .agentClose: "agent.close"
        case .agentCommands: "agent.commands"
        case .agentMcp: "agent.mcp"
        case .agentBtw: "agent.btw"
        case .agentSettings: "agent.settings"
        case .agentPin: "agent.pin"
        case .agentView: "agent.view"
        case .agentSeen: "agent.seen"
        case .agentTimeline: "agent.timeline"
        case .permissionResolve: "permission.resolve"
        case .projectList: "project.list"
        case .agentCreate: "agent.create"
        case .agentProviders: "agent.providers"
        case .configSet: "config.set"
        case .worktreeRename: "worktree.rename"
        case .localCreate: "local.create"
        case .localRename: "local.rename"
        case .localDelete: "local.delete"
        case .automationSave: "automation.save"
        case .automationEnable: "automation.enable"
        case .automationDelete: "automation.delete"
        case .automationRun: "automation.run"
        case .terminalList: "terminal.list"
        case .terminalAttach: "terminal.attach"
        case .terminalDetach: "terminal.detach"
        case .terminalInput: "terminal.input"
        case .terminalResize: "terminal.resize"
        case .terminalCreate: "terminal.create"
        case .terminalClose: "terminal.close"
        case .worktreeCreate: "worktree.create"
        case .worktreeDelete: "worktree.delete"
        case .branchList: "branch.list"
        case .accountList: "account.list"
        case .agentAccount: "agent.account"
        case .gitStatus: "git.status"
        case .gitWatch: "git.watch"
        case .gitDiff: "git.diff"
        case .gitLog: "git.log"
        case .gitCommit: "git.commit"
        case .gitStage: "git.stage"
        case .gitDiscard: "git.discard"
        case .gitCommitCreate: "git.commit.create"
        case .gitPush: "git.push"
        case .gitPull: "git.pull"
        case .ghStatus: "gh.status"
        case .ghPr: "gh.pr"
        case .ghPrs: "gh.prs"
        case .ghPrCreate: "gh.pr.create"
        case .ghPrMerge: "gh.pr.merge"
        case .ghPrReady: "gh.pr.ready"
        case .ghThreadReply: "gh.thread.reply"
        case .ghThreadResolve: "gh.thread.resolve"
        case .fsList: "fs.list"
        case .fsRead: "fs.read"
        case .fsSearch: "fs.search"
        case .attachPut: "attach.put"
        case .pushRegister: "push.register"
        case .pushActivity: "push.activity"
        case .pushPrefs: "push.prefs"
        case .pushUnregister: "push.unregister"
        case .pushTest: "push.test"
        }
    }
}
