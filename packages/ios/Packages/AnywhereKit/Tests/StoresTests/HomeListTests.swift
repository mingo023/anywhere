import Domain
import Fixtures
import Foundation
import Testing
import Wire
@testable import Stores

private func session(_ id: String, _ cwd: String, status: Status = .idle, created: Int = 0, pinned: Bool = false, terminal: String? = nil)
    -> HomeList.Session
{
    HomeList.Session(card: .init(id: id, title: "Session \(id)", status: status, createdAt: created, updatedAt: created, pinned: pinned),
                     cwd: cwd, terminalId: terminal ?? "t-\(id)")
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

    @Test("a folder inside a project but outside its worktrees goes to the project's own checkout")
    func projectFolder() {
        let projects = [Project(path: "/p", name: "p", worktrees: [Worktree(name: "main", path: "/p/main", branch: "main", isMain: true)])]
        let built = HomeList.build(projects: projects, names: [:], locals: [], localOfTerminal: [:], sessions: [session("s", "/p/other")])
        #expect(built.sections[0].trees[0].sessions == ["s"])
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
