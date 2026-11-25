import Domain
import Fixtures
import Foundation
import Testing
import Wire
@testable import Stores

private func session(
    _ id: String, _ cwd: String, status: Status = .idle, created: Int = 0, pinned: Bool = false, terminal: String? = nil, parent: String? = nil,
    projectId: String? = nil, tree: String? = nil
) -> HomeList.Session {
    HomeList.Session(card: .init(id: id, title: "Session \(id)", status: status, createdAt: created, updatedAt: created, pinned: pinned),
                     cwd: cwd, terminalId: terminal ?? "t-\(id)", parent: parent, projectId: projectId, tree: tree)
}

private func build(_ sessions: [HomeList.Session], query: String = "", localOfTerminal: [String: String] = [:]) -> HomeList.Built {
    HomeList.build(projects: Sample.projects, names: Sample.worktreeNames, locals: Sample.locals, localOfTerminal: localOfTerminal,
                   sessions: sessions, query: query)
}

private let anywhere = "/Users/mingo/dev/anywhere"
private let otter = "/Users/mingo/.worktrees/anywhere/calm-otter"

@Suite("Home list")
struct HomeListTests {
    @Test("each project lists its own checkout as Local while in use, then its Locals, then its worktrees by name or branch")
    func trees() {
        let built = build([])
        #expect(built.sections.map(\.name) == ["anywhere", "website", "notes"])
        let trees = built.sections[0].trees
        #expect(trees.map(\.label) == ["Review", "Phone terminal", "fix/login"])
        #expect(trees.map(\.kind) == [.local, .worktree, .worktree])
        #expect(trees[1].place == "calm-otter")
        #expect(built.sections[1].trees[0].label == "dark-mode", "a detached worktree goes by its folder")
        #expect(trees.map(\.branch) == [nil, "calm-otter", "fix/login"])
        #expect(built.sections[1].trees[0].branch == nil, "a detached worktree has no branch to delete with it")
        #expect(built.sections[2].trees.isEmpty)
        let inUse = HomeList.build(projects: Sample.projects, names: [:], locals: [], localOfTerminal: [:], sessions: [],
                                   terminalCwds: [anywhere + "/src"])
        #expect(inUse.sections[0].trees.first.map { [$0.label, $0.place] } == ["Local", "main"])
        let open = HomeList.build(projects: Sample.projects, names: [:], locals: [], localOfTerminal: [:], sessions: [], open: "/Users/mingo/notes")
        #expect(open.sections[2].trees.map(\.kind) == [.own], "the one the phone has open stays while it does")
    }

    @Test("projects keep pocketd's order, or go by name, or by their latest status step")
    func projectSort() {
        let sessions = [session("w", "/Users/mingo/dev/website", created: 5), session("n", "/Users/mingo/notes", created: 9)]
        let order = { (sort: HomeList.ProjectSort) in
            HomeList.build(projects: Sample.projects.reversed(), names: [:], locals: [], localOfTerminal: [:], sessions: sessions, projectSort: sort)
                .sections.map(\.name)
        }
        #expect(order(.manual) == ["notes", "website", "anywhere"])
        #expect(order(.name) == ["anywhere", "notes", "website"])
        #expect(order(.recent) == ["notes", "website", "anywhere"])
    }

    @Test("a session sits in the tree holding its folder, its Local's when its terminal was opened in one, else elsewhere")
    func placing() {
        let built = build(
            [session("deep", otter + "/src/ui"), session("own", anywhere + "/docs"), session("in-local", anywhere, terminal: "t-review"),
             session("lost", "/tmp/x")],
            localOfTerminal: ["t-review": "review", "t-gone": "deleted-local"])
        let trees = Dictionary(uniqueKeysWithValues: built.sections[0].trees.map { ($0.id, $0.sessions) })
        #expect(trees[otter] == ["deep"])
        #expect(trees[anywhere] == ["own"])
        #expect(trees["review"] == ["in-local"])
        #expect(built.elsewhere == ["lost"])
    }

    @Test("a delegated session shows only in its chat, and every worktree pocketd lists stays listed")
    func delegated() {
        let login = "/Users/mingo/.worktrees/anywhere/fix-login"
        let built = build([
            session("chat", anywhere), session("child", otter, parent: "chat"), session("helper", login, parent: "chat"),
            session("mine", login), session("orphan", "/Users/mingo/dev/website", parent: "gone"),
        ], query: "")
        let trees = Dictionary(uniqueKeysWithValues: built.sections.flatMap(\.trees).map { ($0.id, $0.sessions) })
        #expect(trees[otter] == [], "pocketd leaves out a worktree made for an agent; one it lists stays")
        #expect(trees[login] == ["mine"])
        #expect(trees[anywhere] == ["chat"])
        #expect(trees["/Users/mingo/dev/website"] == ["orphan"], "a child whose chat isn't listed stays reachable")
        #expect(build([session("chat", anywhere), session("child", otter, parent: "chat")], query: "child").matches.isEmpty)
    }

    @Test("a folder inside a project but outside its worktrees goes to the project's own checkout")
    func projectFolder() {
        let projects = [Project(path: "/p", name: "p", worktrees: [Worktree(name: "main", path: "/p/main", branch: "main", isMain: true)])]
        let built = HomeList.build(projects: projects, names: [:], locals: [], localOfTerminal: [:], sessions: [session("s", "/p/other")])
        #expect(built.sections[0].trees[0].sessions == ["s"])
    }

    @Test("two projects with the same name and different paths each get the sessions of their id")
    func sameNameProjects() {
        let projects = [
            Project(path: "/a/app", name: "app", worktrees: [Worktree(name: "main", path: "/a/app", branch: "main", isMain: true)], projectId: "pa"),
            Project(path: "/b/app", name: "app", worktrees: [Worktree(name: "main", path: "/b/app", branch: "main", isMain: true)], projectId: "pb"),
        ]
        let built = HomeList.build(projects: projects, names: [:], locals: [], localOfTerminal: [:], sessions: [
            session("one", "/elsewhere", projectId: "pa", tree: "/a/app"), session("two", "/elsewhere", projectId: "pb", tree: "/b/app"),
        ])
        #expect(built.sections.map { $0.trees.flatMap(\.sessions) } == [["one"], ["two"]])
        #expect(built.elsewhere.isEmpty)
    }

    @Test("a session whose folder lies in a worktree the list filtered out lands in its project's main tree by id")
    func filteredWorktree() {
        let projects = [Project(path: "/p", name: "p", worktrees: [Worktree(name: "main", path: "/p/main", branch: "main", isMain: true)], projectId: "pp")]
        let built = HomeList.build(projects: projects, names: [:], locals: [], localOfTerminal: [:], sessions: [
            session("s", "/w/gone/src", projectId: "pp", tree: "/w/gone"), session("c", "/w/gone", parent: "s", projectId: "pp", tree: "/w/gone"),
        ])
        #expect(built.sections[0].trees.map(\.id) == ["/p/main"])
        #expect(built.sections[0].trees[0].sessions == ["s"])
    }

    @Test("a session goes to its tree when its project lists it, and to the project's main tree when it doesn't")
    func projectTree() {
        let projects = [Project(path: "/p", name: "p", worktrees: [
            Worktree(name: "main", path: "/p", branch: "main", isMain: true), Worktree(name: "wt", path: "/w/wt", branch: "wt", isMain: false),
        ], projectId: "pp")]
        let built = HomeList.build(projects: projects, names: [:], locals: [], localOfTerminal: [:], sessions: [
            session("in-wt", "/p", projectId: "pp", tree: "/w/wt"), session("no-tree", "/w/wt", projectId: "pp"),
            session("other", "/w/wt", projectId: "pp", tree: "/q"),
        ])
        let trees = Dictionary(uniqueKeysWithValues: built.sections[0].trees.map { ($0.id, $0.sessions) })
        #expect(trees["/w/wt"] == ["in-wt"])
        #expect(Set(trees["/p"] ?? []) == ["no-tree", "other"])
    }

    @Test("an id no project has falls back to the folder, and so does a session without one")
    func unknownProjectId() {
        let built = build([
            session("unknown", otter, projectId: "nope", tree: "/nowhere"), session("none", otter), session("lost", "/tmp/x", projectId: "nope"),
        ])
        let trees = Dictionary(uniqueKeysWithValues: built.sections[0].trees.map { ($0.id, $0.sessions) })
        #expect(Set(trees[otter] ?? []) == ["unknown", "none"])
        #expect(built.elsewhere == ["lost"])
    }

    @Test("a session opened in a Local stays there whatever its project id")
    func localWinsOverId() {
        let projects = [Project(path: "/p", name: "p", worktrees: [], projectId: "pp")]
        let built = HomeList.build(projects: projects, names: [:], locals: [Local(id: "l", project: "/p", name: "L")], localOfTerminal: ["t-s": "l"],
                                   sessions: [session("s", "/p", projectId: "pp", tree: "/p")])
        #expect(built.sections[0].trees.first { $0.id == "l" }?.sessions == ["s"])
    }

    @Test("a tree's sessions are pinned first, then newest, and the tree and project roll up the most urgent")
    func order() {
        let built = build([
            session("old", otter, status: .done, created: 1), session("new", otter, status: .working, created: 3),
            session("pinned", otter, created: 0, pinned: true), session("needs", anywhere, status: .needsYou, created: 2),
        ])
        let otterRow = built.sections[0].trees.first { $0.id == otter }!
        #expect(otterRow.sessions == ["pinned", "new", "old"])
        #expect(otterRow.rollUp == .init(status: .done, count: 1))
        #expect(built.sections[0].rollUp == .init(status: .needsYou, count: 1))
        #expect(built.sections[1].rollUp == nil)
    }

    @Test("search keeps a matching project whole, else the matching trees whole, else the matching sessions")
    func search() {
        let sessions = [session("1", otter), session("2", anywhere), session("3", "/Users/mingo/dev/website")]
        #expect(build(sessions, query: "WEB").sections.map(\.name) == ["website"])
        #expect(build(sessions, query: "web").sections[0].trees.count == 2)
        let byTree = build(sessions, query: "phone").sections
        #expect(byTree.map(\.name) == ["anywhere"])
        #expect(byTree[0].trees.map(\.label) == ["Phone terminal"])
        let bySession = build(sessions, query: "session 2").sections
        #expect(bySession.count == 1)
        #expect(bySession[0].trees.map(\.sessions) == [["2"]])
        #expect(build(sessions, query: "nothing").sections.isEmpty)
    }

    @Test("search lists the sessions whose title matches, newest first, with their project and tree")
    func matches() {
        let sessions = [session("1", otter, created: 1), session("12", anywhere, created: 5), session("3", "/Users/mingo/dev/website", created: 3)]
        #expect(build(sessions, query: "session 1").matches == [
            .init(id: "12", project: "anywhere", tree: "Local"), .init(id: "1", project: "anywhere", tree: "Phone terminal"),
        ])
        #expect(build(sessions).matches.isEmpty)
        #expect(build(sessions, query: "anywhere").matches.isEmpty, "a project's name isn't a session's title")
    }

    @Test("a tree being deleted leaves the list, its sessions with it")
    func hiding() {
        let built = HomeList.build(projects: Sample.projects, names: Sample.worktreeNames, locals: Sample.locals, localOfTerminal: [:],
                                   sessions: [session("1", otter, status: .done)], hiding: [otter, "review"])
        #expect(built.sections[0].trees.map(\.label) == ["fix/login"])
        #expect(built.sections[0].rollUp == nil)
    }

    @Test("the list for 50 projects of 10 worktrees and 200 sessions builds in a few milliseconds")
    func scale() {
        let (projects, agents) = Sample.workspace(projects: 50, worktrees: 10, agents: 200)
        let sessions = agents.map { HomeList.Session(card: .init(id: $0.id, title: $0.title, status: .idle, createdAt: $0.createdAt, updatedAt: $0.updatedAt), cwd: $0.cwd, terminalId: $0.terminalId) }
        let built = HomeList.build(projects: projects, names: [:], locals: [], localOfTerminal: [:], sessions: sessions, terminalCwds: projects.map(\.path))
        #expect(built.sections.count == 50)
        #expect(built.sections.flatMap(\.trees).count == 500)
        #expect(built.sections.flatMap(\.trees).flatMap(\.sessions).count + built.elsewhere.count == 200)
        #expect(built.elsewhere.count == 40)
        for (agentCount, budget) in [(200, 20.0), (2_000, 60.0)] {
            let (p, a) = Sample.workspace(projects: 50, worktrees: 10, agents: agentCount)
            let s = a.map { HomeList.Session(card: .init(id: $0.id, title: $0.title, status: .working, createdAt: $0.createdAt, updatedAt: $0.updatedAt), cwd: $0.cwd, terminalId: $0.terminalId) }
            var best = Double.infinity
            for _ in 0..<20 {
                let start = DispatchTime.now().uptimeNanoseconds
                _ = HomeList.build(projects: p, names: [:], locals: [], localOfTerminal: [:], sessions: s, terminalCwds: p.map(\.path), query: "")
                best = min(best, Double(DispatchTime.now().uptimeNanoseconds - start) / 1e6)
            }
            print("[bench] home list, 50 projects × 10 worktrees × \(agentCount) sessions: \(String(format: "%.3f", best)) ms")
            #expect(best < budget)
        }
    }
}
