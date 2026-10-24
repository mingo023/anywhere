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

    @Test("progress lands on the creation a sheet watches, and others are ignored")
    func progress() async {
        let s = store()
        await #expect(throws: DaemonError.self) { try await s.launch(.init(spec: .init(project: "/p", checkout: .worktree("/p"), provider: .claude, access: .edits, plan: false)), requestId: "r1") }
        #expect(s.creations["r1"]?.error?.message == "This Mac isn't connected")
        #expect(s.apply(.agentProgress(.init(requestId: "r1", step: "worktree"))))
        #expect(s.apply(.agentCreating(.init(requestId: "r1", terminalId: "t9", cwd: "/p", setup: false))))
        #expect(s.creations["r1"]?.current == "worktree")
        #expect(s.creations["r1"]?.terminalId == "t9")
        #expect(!s.apply(.worktreeProgress(.init(requestId: "other", step: "fetch"))))
        s.forget(creation: "r1")
        #expect(s.creations.isEmpty)
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

    @Test("a new Local a launch couldn't make leaves the list, and blank Local names change nothing")
    func failedLocal() async {
        let s = store()
        let plan = LaunchPlan(local: .init(localId: "local-9", project: Sample.projects[0].path, name: "Local 1"),
                              spec: .init(project: Sample.projects[0].path, checkout: .local("local-9"), provider: .claude, access: .edits, plan: false))
        await #expect(throws: DaemonError.self) { try await s.launch(plan, requestId: "r2") }
        #expect(s.locals == Sample.locals)
        #expect(s.creations["r2"]?.madeLocal == nil)
        try? await s.renameLocal(id: "review", title: "  ")
        #expect(s.locals == Sample.locals)
    }
}

