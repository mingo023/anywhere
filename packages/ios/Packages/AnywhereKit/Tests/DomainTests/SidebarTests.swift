import Foundation
import Testing
@testable import Domain

private func card(_ title: String, _ status: Status, created: Int = 0, updated: Int = 0, pinned: Bool = false) -> Sidebar.Card {
    Sidebar.Card(id: title, title: title, status: status, createdAt: created, updatedAt: updated, pinned: pinned)
}

@Suite("Sidebar places")
struct SidebarPlacesTests {
    @Test("a project holds its folder and what is below it")
    func holds() {
        #expect(Sidebar.projectOf("/a/foo", projects: ["/a/foo"], trees: [:]) == "/a/foo")
        #expect(Sidebar.projectOf("/a/foo/src", projects: ["/a/foo"], trees: [:]) == "/a/foo")
    }

    @Test("a project does not hold a sibling sharing its prefix")
    func sibling() {
        #expect(Sidebar.projectOf("/a/foobar", projects: ["/a/foo"], trees: [:]) == nil)
        #expect(Sidebar.projectOf("/a", projects: ["/a/foo"], trees: [:]) == nil)
    }

    @Test("a project saved with a trailing slash holds only that exact path")
    func trailingSlash() {
        #expect(Sidebar.projectOf("/a/foo/", projects: ["/a/foo/"], trees: [:]) == "/a/foo/")
        #expect(Sidebar.projectOf("/a/foo", projects: ["/a/foo/"], trees: [:]) == nil)
        #expect(Sidebar.projectOf("/a/foo/src", projects: ["/a/foo/"], trees: [:]) == nil)
    }

    @Test("the innermost of nested projects owns a folder")
    func nested() {
        let projects = ["/a/foo/pkg", "/a/foo"]
        #expect(Sidebar.projectOf("/a/foo/pkg/src", projects: projects, trees: [:]) == "/a/foo/pkg")
        #expect(Sidebar.projectOf("/a/foo/src", projects: projects, trees: [:]) == "/a/foo")
    }

    @Test("a project owns its worktrees outside its folder")
    func outside() {
        let trees = ["/a/foo": ["/a/foo", "/w/foo-fix"]]
        #expect(Sidebar.projectOf("/w/foo-fix/src", projects: ["/a/foo", "/w"], trees: trees) == "/a/foo")
        #expect(Sidebar.projectOf("/w/other", projects: ["/a/foo", "/w"], trees: trees) == "/w")
    }

    @Test("a later project inside a repo ties with the repo and takes its folders")
    func tie() {
        let trees = ["/a/foo": ["/a/foo"], "/a/foo/pkg": ["/a/foo"]]
        #expect(Sidebar.projectOf("/a/foo/src", projects: ["/a/foo", "/a/foo/pkg"], trees: trees) == "/a/foo/pkg")
    }

    @Test("a folder belongs to the innermost worktree holding it")
    func innermost() {
        let trees = ["/a/foo", "/a/foo/.worktrees/fix"]
        #expect(Sidebar.worktreeOf("/a/foo/.worktrees/fix/src", trees: trees) == "/a/foo/.worktrees/fix")
        #expect(Sidebar.worktreeOf("/a/foo/src", trees: trees) == "/a/foo")
        #expect(Sidebar.worktreeOf("/a/foobar", trees: trees) == nil)
    }

    @Test("a folder's tree is its worktree, else its project's main tree")
    func treeOf() {
        let trees = ["/a/foo": ["/a/foo", "/w/fix"]]
        let projects = ["/a/foo", "/b"]
        #expect(Sidebar.treeOf("/w/fix/src", projects: projects, trees: trees, mains: ["/a/foo": "/a/foo"]) == "/w/fix")
        #expect(Sidebar.treeOf("/b/src", projects: projects, trees: trees, mains: [:]) == "/b")
        #expect(Sidebar.treeOf("/c", projects: projects, trees: trees, mains: [:]) == nil)
    }
}

@Suite("Sidebar sessions")
struct SidebarSessionsTests {
    @Test("sessions sort newest first, by status, by activity or by name")
    func sorts() {
        let cards = [card("b", .idle, created: 1, updated: 9), card("C", .needsYou, created: 2, updated: 5), card("a", .working, created: 3, updated: 1)]
        let order = { (by: Sidebar.SessionSort) in Sidebar.sorted(cards, by: by).map(\.title) }
        #expect(order(.newest) == ["a", "C", "b"])
        #expect(order(.status) == ["C", "a", "b"])
        #expect(order(.activity) == ["b", "C", "a"])
        #expect(order(.name) == ["a", "b", "C"])
    }

    @Test("pinned sessions lead their tree, each part in the sort's order")
    func pinnedFirst() {
        let cards = [card("old pin", .idle, created: 1, pinned: true), card("new", .idle, created: 5), card("new pin", .idle, created: 4, pinned: true)]
        #expect(Sidebar.sorted(cards).map(\.title) == ["new pin", "old pin", "new"])
    }

    @Test("sections put Failed with Done and split Idle by day")
    func sections() {
        let got = [(Status.needsYou, false), (.failed, false), (.done, true), (.working, true), (.idle, true), (.idle, false)]
            .map { Sidebar.section($0.0, today: $0.1) }
        #expect(got == [0, 1, 1, 2, 3, 4])
    }

    @Test("rolls up the most urgent status and counts only the sessions that have it")
    func rollUp() {
        #expect(Sidebar.rollUp([.working, .done, .failed, .done, .idle]).map { [$0.status: $0.count] } == [.failed: 1])
        #expect(Sidebar.rollUp([.working, .needsYou, .done, .needsYou]).map { [$0.status: $0.count] } == [.needsYou: 2])
        #expect(Sidebar.rollUp([.idle, .working]).map { [$0.status: $0.count] } == [.working: 1])
        #expect(Sidebar.rollUp([.idle]) == nil)
        #expect(Sidebar.rollUp([]) == nil)
    }

    @Test("a breakdown counts each status but Idle, most urgent first")
    func breakdown() {
        #expect(Sidebar.breakdown([.done, .working, .failed, .done, .idle]).map { "\($0.count) \($0.status)" } == ["1 failed", "2 done", "1 working"])
        #expect(Sidebar.breakdown([.idle]).isEmpty)
    }

    @Test("search keeps the sessions whose title matches in any case, and a status change keeps their order")
    func search() {
        let cards = [card("Fix CI", .idle), card("Docs", .working), card("fix login", .done)]
        #expect(Sidebar.matching(cards, "FIX").map(\.title) == ["Fix CI", "fix login"])
        #expect(Sidebar.matching(cards, " ").map(\.title) == ["Fix CI", "Docs", "fix login"])
    }

    @Test("a matching project lists all its trees and otherwise only the matching ones")
    func shown() {
        let trees = [(key: "/p", label: "Local"), (key: "/wt/a", label: "fix-login"), (key: "/wt/b", label: "docs")]
        #expect(Sidebar.shown("pock", name: "coding-pocket", trees: trees) == ["/p", "/wt/a", "/wt/b"])
        #expect(Sidebar.shown("login", name: "coding-pocket", trees: trees) == ["/wt/a"])
        #expect(Sidebar.shown("nothing", name: "coding-pocket", trees: trees) == nil)
        #expect(Sidebar.shown("pock", name: "coding-pocket", trees: []) == [])
        #expect(Sidebar.shown("x", name: "coding-pocket", trees: []) == nil)
    }
}

@Suite("Sidebar names")
struct SidebarNamesTests {
    @Test("a worktree shows its name over its branch, and a detached one its folder")
    func treeLabel() {
        #expect(Sidebar.treeLabel(path: "/wt/calm-otter", branch: "fix-login", given: "Fix the login form") == ("Fix the login form", "fix-login"))
        #expect(Sidebar.treeLabel(path: "/wt/calm-otter", branch: "fix-login", given: nil) == ("fix-login", "fix-login"))
        #expect(Sidebar.treeLabel(path: "/wt/calm-otter", branch: "", given: nil) == ("calm-otter", "calm-otter"))
        #expect(Sidebar.treeLabel(path: "/wt/calm-otter", branch: "detached", given: "Fix the login form") == ("Fix the login form", "calm-otter"))
    }

    @Test("a new Local takes the first free number")
    func nextLocal() {
        #expect(Sidebar.nextLocalName([]) == "Local 1")
        #expect(Sidebar.nextLocalName(["Local", "Local 1", "Local 2", "Local 4"]) == "Local 3")
    }

    @Test("a Local id is anything but a path, of the characters pocketd takes")
    func localId() {
        let id = Sidebar.newLocalId(now: Date(timeIntervalSince1970: 1_790_000_000))
        #expect(!id.hasPrefix("/"))
        #expect(id.allSatisfy { $0.isLetter || $0.isNumber || $0 == "-" })
    }

    @Test("a model id reads as its family and version")
    func modelName() {
        #expect(Sidebar.modelName("claude-opus-5-5") == "Opus 5.5")
        #expect(Sidebar.modelName("claude-haiku-4-5-20251001") == "Haiku 4.5")
        #expect(Sidebar.modelName("haiku") == "Haiku")
        #expect(Sidebar.modelName("gpt-5.5") == "GPT 5.5")
    }

    @Test("a model reads without who serves it, its date or its dashes, as on the desktop")
    func servedModelName() {
        let ids = [
            "openai-codex/gpt-5.4", "opencode/big-pickle", "anthropic/claude-sonnet-4-5", "openai-codex:gpt-6-sol", "zai/glm-5.2-fast",
            "composer-2.5", "gemini-3.8-flash-high", "claude-3-5-sonnet-20241022", "deepseek/deepseek-v4-pro", "haiku", "",
        ]
        #expect(ids.map(Sidebar.modelName) == [
            "GPT 5.4", "Big Pickle", "Sonnet 4.5", "GPT 6 Sol", "GLM 5.2 Fast", "Composer 2.5", "Gemini 3.8 Flash High", "3.5 Sonnet",
            "Deepseek V4 Pro", "Haiku", "",
        ])
    }

    @Test("Claude's 1M-context mark stays as written after the name")
    func contextWindow() {
        let ids = ["claude-opus-4-7[1m]", "sonnet[1m]", "claude-haiku-4-5-20251001[1m]", "[1m]"]
        #expect(ids.map(Sidebar.modelName) == ["Opus 4.7[1m]", "Sonnet[1m]", "Haiku 4.5[1m]", "[1m]"])
    }

    @Test("a Bedrock id reads without where and who serve it")
    func bedrock() {
        let ids = [
            "amazon-bedrock/us.anthropic.claude-opus-5-5", "us.anthropic.claude-sonnet-4-5-20250929-v1:0",
            "amazon-bedrock/anthropic.claude-haiku-4-5-20251001-v1:0", "amazon-bedrock/amazon.nova-lite-v1:0",
            "amazon-bedrock/us-gov.openai.gpt-oss-120b-1:0", "amazon-bedrock/global.openai.gpt-6.1-sol", "amazon-bedrock/moonshotai.kimi-k2.5",
            "amazon-bedrock/qwen.qwen3-32b-v1:0", "amazon-bedrock/minimax.minimax-m2.5",
        ]
        #expect(ids.map(Sidebar.modelName) == [
            "Opus 5.5", "Sonnet 4.5 V1:0", "Haiku 4.5 V1:0", "Nova Lite V1:0", "GPT Oss 120B 1:0", "GPT 6.1 Sol", "Kimi K2.5", "Qwen3 32B V1:0",
            "Minimax M2.5",
        ])
    }

    @Test("a version's dot is no vendor prefix")
    func versionDot() {
        let ids = ["gpt-5.4", "qwen3.5-coder", "MiniMax-M2.5", "amazon-bedrock/deepseek.v3.2", "opencode/mimo-v2.6-flash-free", "a.b"]
        #expect(ids.map(Sidebar.modelName) == ["GPT 5.4", "Qwen3.5 Coder", "MiniMax M2.5", "Deepseek.v3.2", "Mimo V2.6 Flash Free", "A.b"])
    }
}
