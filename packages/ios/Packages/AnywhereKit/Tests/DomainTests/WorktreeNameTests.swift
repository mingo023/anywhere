import Testing
@testable import Domain

@Suite("New worktree names")
struct WorktreeNameTests {
    @Test("a name must be free and valid for git")
    func problem() {
        let taken: Set = ["main", "Foo", "fix/login"]
        #expect(WorktreeName.problem("fix-login_2.0", taken: taken) == nil)
        for used in ["main", "foo", "fix"] {
            #expect(WorktreeName.problem(used, taken: taken) == WorktreeName.takenProblem, "\(used)")
        }
        for bad in ["", "fix login", "fix/login", "-x", ".x", "x.", "a..b", "x.lock", "tên", "HEAD"] {
            #expect(WorktreeName.problem(bad, taken: taken) == WorktreeName.invalidProblem, "\(bad)")
        }
    }

    @Test("a closed worktree's pushed branch keeps its name taken")
    func pushedTaken() {
        let taken = WorktreeName.taken(local: ["main"], remote: ["origin/calm-lynx"], folders: ["brave-otter"])
        #expect(WorktreeName.problem("calm-lynx", taken: taken) == WorktreeName.takenProblem)
        #expect(WorktreeName.isTaken("brave-otter", taken))
        let first = WorktreeName.freeName(seed: 0, taken: [])
        #expect(WorktreeName.freeName(seed: 0, taken: WorktreeName.taken(local: [], remote: [first])) != first)
    }

    @Test("an empty name falls back to a free one, and the seed wraps round")
    func freeName() {
        let fresh = { (seed: Int) in WorktreeName.freeName(seed: seed, taken: []) }
        let taken: Set = [fresh(0).uppercased(), "\(fresh(1))/x", "release/1.0"]
        #expect(WorktreeName.freeName(seed: 0, taken: taken) == fresh(2))
        #expect(fresh(WorktreeName.adjectives.count * WorktreeName.nouns.count) == fresh(0))
        #expect(WorktreeName.unique("worktree", ["worktree", "worktree-2"]) == "worktree-3")
    }

    @Test("a PR is a number, a hash number or a pull URL")
    func parsePR() {
        for ok in ["123", " #123 ", "https://github.com/acme/app/pull/123", "https://github.com/acme/app/pull/123/files",
                   "https://github.com/acme/app/pull/123?w=1", "https://github.com/acme/app/pull/123#issuecomment-1"] {
            #expect(WorktreeName.parsePR(ok) == 123, "\(ok)")
        }
        for bad in ["", "#", "+5", "0", "abc", "https://github.com/acme/app/issues/123"] {
            #expect(WorktreeName.parsePR(bad) == nil, "\(bad)")
        }
    }

    @Test("the branch menu lists recent branches until a search reaches all and the remote's")
    func listed() {
        let local = (0..<25).map { "b\($0)" } + ["fix/login"]
        #expect(WorktreeName.listed(local: local, remote: ["fix/logout"], search: " ").count == 20)
        let found = WorktreeName.listed(local: local, remote: ["fix/logout"], search: "FIX/LOG")
        #expect(found.map(\.name) == ["fix/login", "fix/logout"])
        #expect(found.map(\.local) == [true, false])
    }

    @Test("the default base is the preferred one, else main, master, the current branch, else the newest")
    func defaultBase() {
        #expect(WorktreeName.defaultBase(["feat", "fix", "main", "old"], current: "fix") == "main")
        #expect(WorktreeName.defaultBase(["feat", "fix", "master"], current: "fix") == "master")
        #expect(WorktreeName.defaultBase(["feat", "fix"], current: "fix") == "fix")
        #expect(WorktreeName.defaultBase(["feat", "fix"], preferred: "feat", current: "fix") == "feat")
        #expect(WorktreeName.defaultBase(["feat", "fix"]) == "feat")
        #expect(WorktreeName.defaultBase([]) == nil)
    }

    @Test("an existing branch opens without its remote and lands in a dashed folder")
    func folders() {
        #expect(WorktreeName.withoutRemote("origin/fix/login") == "fix/login")
        #expect(WorktreeName.folder(name: "", branch: "fix/login") == "fix-login")
        #expect(WorktreeName.folder(name: "#7", pr: 7) == "pr-7")
        #expect(WorktreeName.folder(name: "calm-otter") == "calm-otter")
    }
}
