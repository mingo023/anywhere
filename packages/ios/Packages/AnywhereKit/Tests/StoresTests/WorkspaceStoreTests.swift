import Client
import Fixtures
import Testing
import Wire
@testable import Stores

@MainActor
private func store(_ messages: [ServerMessage] = Sample.snapshot) -> WorkspaceStore {
    let store = WorkspaceStore()
    for m in messages { _ = store.apply(m) }
    return store
}

@Suite("Workspace store")
@MainActor
struct WorkspaceStoreTests {
    @Test("the snapshot fills projects, names, Locals and automations, and a repeat changes nothing")
    func snapshot() {
        let s = store()
        #expect(s.loaded)
        #expect(s.projects == Sample.projects)
        #expect(s.names == Sample.worktreeNames)
        #expect(s.locals == Sample.locals)
        #expect(s.automations == Sample.automations)
        #expect(!Sample.snapshot.contains { s.apply($0) })
    }

    @Test("a terminal list tells which Local each terminal was opened in")
    func terminals() {
        let s = store([.terminalList(.init(terminals: Sample.terminals))])
        #expect(s.localOfTerminal == ["t-review": "review"])
    }

    @Test("rows are named as the sidebar names them")
    func labels() {
        let s = store()
        let anywhere = Sample.projects[0]
        #expect(s.label(project: anywhere, key: anywhere.path) == "Local")
        #expect(s.label(project: anywhere, key: "review") == "Review")
        #expect(s.label(project: anywhere, key: "/Users/mingo/.worktrees/anywhere/calm-otter") == "Phone terminal")
        #expect(s.label(project: anywhere, key: "/Users/mingo/.worktrees/anywhere/fix-login") == "fix/login")
        #expect(s.nextLocalName(project: anywhere.path) == "Local 1")
    }

    @Test("a phone manages worktrees and Locals only with workspace.v1 and spawn")
    func canManage() {
        let s = store()
        #expect(!s.canManage)
        s.connected(Sample.hello)
        #expect(s.canManage)
        var observer = Sample.hello
        observer.scopes = [.observe, .drive]
        s.connected(observer)
        #expect(!s.canManage)
    }

    @Test("progress lands on the creation it is for, and others are ignored")
    func progress() async {
        let s = store()
        s.add(.init(spec: .init(project: "/p", checkout: .worktree("/p"), provider: .claude, access: .edits, plan: false)), requestId: "r1", title: "Fix it", place: "/p")
        #expect(s.creations["r1"]?.project == "/p")
        #expect(s.apply(.agentProgress(.init(requestId: "r1", step: "worktree"))))
        #expect(s.apply(.agentCreating(.init(requestId: "r1", terminalId: "t9", cwd: "/p", setup: false))))
        #expect(s.creations["r1"]?.current == "worktree")
        #expect(s.creations["r1"]?.terminalId == "t9")
        #expect(!s.apply(.worktreeProgress(.init(requestId: "other", step: "fetch"))))
        s.forget(creation: "r1")
        #expect(s.creations.isEmpty)
    }

    @Test("a launch that loses its Mac waits for it instead of failing, and goes on when the Mac lists its projects again")
    func waits() async {
        let s = store()
        s.add(.init(spec: .init(project: "/p", checkout: .worktree("/p"), provider: .claude, access: .edits, plan: false)), requestId: "r1", title: "", place: "/p")
        await s.run("r1")
        #expect(s.creations["r1"]?.waiting == true)
        #expect(s.creations["r1"]?.error == nil)
        _ = s.apply(.projectList(.init(projects: Sample.projects)))
        #expect(s.creations["r1"]?.waiting == false, "taken up again at once, so a second list doesn't run it twice")
    }

    @Test("an agent on the terminal a launch opened is the agent it started, even when its reply was lost")
    func startedByTerminal() {
        let s = store()
        s.add(.init(spec: .init(project: "/p", checkout: .worktree("/p"), provider: .claude, access: .edits, plan: false)), requestId: "r1", title: "", place: "/p")
        _ = s.apply(.agentCreating(.init(requestId: "r1", terminalId: "t-a-working", cwd: "/p", setup: false)))
        let other = Sample.agents.first { $0.terminalId != "t-a-working" }!
        #expect(!s.apply(.agentUpdate(.init(agent: other))))
        let agent = Sample.agents.first { $0.terminalId == "t-a-working" }!
        #expect(s.apply(.agentList(.init(agents: Sample.agents))))
        #expect(s.creations["r1"]?.agentId == agent.id)
        #expect(s.creations["r1"]?.done == true)
    }

    @Test("a started launch is handed out once, unless a sheet watches it")
    func takeStarted() {
        let s = store()
        let plan = LaunchPlan(spec: .init(project: "/p", checkout: .worktree("/p"), provider: .claude, access: .edits, plan: false))
        s.add(plan, requestId: "seen", title: "", place: nil)
        s.add(plan, requestId: "hidden", title: "", place: nil)
        s.watch("seen", true)
        _ = s.apply(.agentCreating(.init(requestId: "seen", terminalId: "t-a-working", cwd: "/p", setup: false)))
        _ = s.apply(.agentCreating(.init(requestId: "hidden", terminalId: "t-a-needs", cwd: "/p", setup: false)))
        _ = s.apply(.agentList(.init(agents: Sample.agents)))
        #expect(s.takeStarted().map(\.agentId) == ["a-needs"])
        #expect(s.takeStarted().isEmpty)
        #expect(s.creations["seen"]?.done == true)
    }

    @Test("a worktree whose create reply a dropped link lost is found by its folder")
    func landed() {
        let anywhere = Sample.projects[0]
        #expect(WorkspaceStore.landed("calm-otter", in: anywhere) == "/Users/mingo/.worktrees/anywhere/calm-otter")
        #expect(WorkspaceStore.landed("anywhere", in: anywhere) == nil, "never the project's own checkout")
        #expect(WorkspaceStore.landed("brave-heron", in: anywhere) == nil)
    }

    @Test("a tree being deleted hides at once, a second delete is ignored, and a failure brings it back once with why")
    func removal() async {
        let s = store()
        let path = "/Users/mingo/.worktrees/anywhere/calm-otter"
        await s.remove(Removal(id: path, label: "Phone terminal", local: false))
        #expect(s.hidden.isEmpty)
        let failed = s.takeFailedRemoval()
        #expect(failed?.error == "Not connected to your Mac")
        #expect(failed?.label == "Phone terminal")
        #expect(s.takeFailedRemoval() == nil)
        #expect(s.projects == Sample.projects, "the worktree stays listed")
    }

    @Test("a Local or name shown at once is put back when the Mac can't take it")
    func optimistic() async {
        let s = store()
        let path = "/Users/mingo/.worktrees/anywhere/calm-otter"
        await #expect(throws: DaemonError.self) { try await s.renameWorktree(path: path, title: "Other") }
        #expect(s.names[path] == "Phone terminal")
        await #expect(throws: DaemonError.self) { try await s.createLocal(project: Sample.projects[0].path) }
        #expect(s.locals == Sample.locals)
        await #expect(throws: DaemonError.self) { try await s.renameLocal(id: "review", title: "Later") }
        #expect(s.locals == Sample.locals)
    }

    @Test("a delete refused for unpushed commits says how many a forced one would lose, and only then goes out with force")
    func unpushed() {
        let refused = DaemonError.server(ServerError(message: "2 commits are only on this branch", code: .unpushedCommits, count: 2))
        #expect(WorkspaceStore.unpushedCommits(refused) == 2)
        #expect(WorkspaceStore.unpushedCommits(DaemonError.server(ServerError(message: "x", code: .gitFailed))) == nil)
        #expect(WorkspaceStore.unpushedCommits(DaemonError.timedOut) == nil)
        #expect(WorkspaceStore.deletion(path: "/w", deleteBranch: true, force: false) == .worktreeDelete(.init(path: "/w", deleteBranch: true)))
        #expect(WorkspaceStore.deletion(path: "/w", deleteBranch: true, force: true)
            == .worktreeDelete(.init(path: "/w", deleteBranch: true, force: true)))
        #expect(WorkspaceStore.deletion(path: "/w", deleteBranch: false, force: false) == .worktreeDelete(.init(path: "/w")))
    }

    @Test("a new Local a launch couldn't make leaves the list, and blank Local names change nothing")
    func failedLocal() async {
        let s = store()
        let plan = LaunchPlan(local: .init(localId: "local-9", project: Sample.projects[0].path, name: "Local 1"),
                              spec: .init(project: Sample.projects[0].path, checkout: .local("local-9"), provider: .claude, access: .edits, plan: false))
        s.add(plan, requestId: "r2", title: "", place: nil)
        await s.run("r2")
        #expect(s.locals == Sample.locals)
        #expect(s.creations["r2"]?.madeLocal == nil)
        try? await s.renameLocal(id: "review", title: "  ")
        #expect(s.locals == Sample.locals)
    }
}

