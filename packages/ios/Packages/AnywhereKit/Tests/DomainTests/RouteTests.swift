import Foundation
import Testing
@testable import Domain

@Suite("Routes")
struct RouteTests {
    @Test("a file route opens at its line, and at the top without one")
    func fileLine() {
        #expect(Route.file(mac: "m1", tree: "/r", path: "/r/a.rs") == .file(mac: "m1", tree: "/r", path: "/r/a.rs", line: nil))
        #expect(Route.file(mac: "m1", tree: "/r", path: "/r/a.rs", line: 842).mac == "m1")
    }

    @Test("a new session route carries its place and prompt, and neither by default")
    func newSession() {
        #expect(Route.newSession(mac: "m1", project: "/r") == .newSession(mac: "m1", project: "/r", place: nil, prompt: nil))
        let route = Route.newSession(mac: "m1", project: "/r", place: "/r/.worktrees/calm-otter", prompt: "Fix the build")
        guard case .newSession(_, _, let place, let prompt) = route else { Issue.record("not a new session"); return }
        #expect(place == "/r/.worktrees/calm-otter")
        #expect(prompt == "Fix the build")
    }

    @Test("a route names the tree it is about, and none for a Mac-wide screen or a chat")
    func tree() {
        #expect(Route.worktree(mac: "m1", path: "/r/wt").tree == "/r/wt")
        #expect(Route.local(mac: "m1", id: "l1").tree == "l1")
        #expect(Route.pullRequest(mac: "m1", tree: "/r/wt").tree == "/r/wt")
        #expect(Route.file(mac: "m1", tree: "/r", path: "a.rs").tree == "/r")
        #expect(Route.chat(mac: "m1", agentId: "a1").tree == nil)
        #expect(Route.pullRequests(mac: "m1", project: "/r").tree == nil)
    }

    @Test("stacks saved before routes had a line, place, prompt or chat still restore", arguments: [
        (#"{"file":{"mac":"m1","tree":"/r","path":"/r/a.rs"}}"#, Route.file(mac: "m1", tree: "/r", path: "/r/a.rs")),
        (#"{"newSession":{"mac":"m1","project":"/r"}}"#, Route.newSession(mac: "m1", project: "/r")),
        (#"{"files":{"mac":"m1","tree":"/r","dir":""}}"#, Route.files(mac: "m1", tree: "/r", dir: "")),
        (#"{"changes":{"mac":"m1","tree":"/r"}}"#, Route.changes(mac: "m1", tree: "/r")),
        (#"{"diff":{"mac":"m1","tree":"/r","path":"a.rs","staged":false}}"#, Route.diff(mac: "m1", tree: "/r", path: "a.rs", staged: false)),
        (#"{"pullRequest":{"mac":"m1","tree":"/r"}}"#, Route.pullRequest(mac: "m1", tree: "/r")),
        (#"{"reviewThreads":{"mac":"m1","tree":"/r"}}"#, Route.reviewThreads(mac: "m1", tree: "/r")),
    ])
    func restores(_ json: String, _ route: Route) throws {
        #expect(try JSONDecoder().decode(Route.self, from: Data(json.utf8)) == route)
    }

    @Test("a route survives saving and restoring with every value set")
    func roundTrips() throws {
        let routes: [Route] = [
            .file(mac: "m1", tree: "/r", path: "/r/a.rs", line: 7, chat: "a1"),
            .files(mac: "m1", tree: "/r", dir: "src", chat: "a1"),
            .changes(mac: "m1", tree: "/r", chat: "a1"),
            .diff(mac: "m1", tree: "/r", path: "a.rs", staged: true, chat: "a1"),
            .newSession(mac: "m1", project: "/r", place: "local-1", prompt: "Explain lines 40–58"),
            .pullRequest(mac: "m1", tree: "/r", chat: "a1"),
            .reviewThreads(mac: "m1", tree: "/r", chat: "a1"),
        ]
        #expect(try JSONDecoder().decode([Route].self, from: JSONEncoder().encode(routes)) == routes)
    }
}
