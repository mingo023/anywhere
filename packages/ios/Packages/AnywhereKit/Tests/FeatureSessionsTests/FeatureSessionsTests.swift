import Domain
import Fixtures
import Foundation
import Stores
import Testing
import Wire
@testable import FeatureSessions

private let utc = TimeZone(secondsFromGMT: 0)!

@Suite("Session text")
struct SessionTextTests {
    @Test("a session's second line is what it is doing, else a status its glyph alone can't tell apart, else its model")
    func detail() {
        let needs = Sample.agents.first { $0.id == "a-needs" }!
        #expect(SessionText.detail(needs, .needsYou) == "Approve Bash: rm -rf build")
        let compacting = Sample.agents.first { $0.id == "a-compacting" }!
        #expect(SessionText.detail(compacting, .working) == "Compacting…")
        let failed = Sample.agents.first { $0.id == "a-failed" }!
        #expect(SessionText.detail(failed, .failed) == "Failed")
        let working = Sample.agents.first { $0.id == "a-working" }!
        #expect(SessionText.detail(working, .working) == "Opus 5.5")
        let idle = Sample.agents.first { $0.id == "a-chat" }!
        #expect(SessionText.detail(idle, .idle) == "Sonnet 5")
    }

    @Test("pocketd's steps read as what is happening, and an unknown one as itself")
    func steps() {
        #expect(SessionText.step("worktree") == "Creating git worktree")
        #expect(SessionText.step("setup") == "Setting up")
        #expect(SessionText.step("agent") == "Starting agent")
        #expect(SessionText.step("warmup") == "Warmup")
    }

    @Test("an account shows the fuller of its usage windows")
    func account() {
        #expect(SessionText.account(Sample.accounts[0]) == "Work · 42%")
        #expect(SessionText.account(Sample.accounts[2]) == "Not signed in")
    }
}

@Suite("Places")
struct PlacesTests {
    @Test("a session starts in a new Worktree or Local as on the desktop, each as the Mac allows")
    func options() {
        #expect(Places.options(canManage: true, canSpawn: true).map(\.label) == ["Worktree", "Local"])
        #expect(Places.options(canManage: false, canSpawn: true).map(\.place) == [.newWorktree])
        #expect(Places.options(canManage: false, canSpawn: false).isEmpty)
    }

    @Test("a session asked for from a workspace's row names that workspace, only in its own project")
    func existing() {
        let project = Sample.projects[0]
        let calm = "/Users/mingo/.worktrees/anywhere/calm-otter"
        #expect(Places.existing(.tree(calm), project: project, locals: [], names: Sample.worktreeNames)?.label == "Phone terminal")
        #expect(Places.existing(.tree(project.path), project: project, locals: [], names: [:])?.label == "Local")
        #expect(Places.existing(.local("review"), project: project, locals: Sample.locals, names: [:])?.label == "Review")
        #expect(Places.existing(.tree(calm), project: Sample.projects[1], locals: [], names: [:]) == nil)
        #expect(Places.existing(.newWorktree, project: project, locals: [], names: [:]) == nil)
    }

    @Test("a new worktree starts from main as pocketd picks it, or from the checkout's branch before branches are listed")
    func defaultBase() {
        let project = Sample.projects[0]
        #expect(Places.defaultBase(project: project, branches: Sample.branches) == "main")
        #expect(Places.defaultBase(project: project, branches: nil) == project.worktrees.first(where: \.isMain)?.branch)
        #expect(Places.defaultBase(project: Project(path: "/p", name: "p", worktrees: []), branches: nil) == nil)
    }

    @Test("a new worktree can't take a branch's name, local or on origin, or a worktree's folder")
    func taken() {
        let taken = Places.taken(project: Sample.projects[0], branches: Sample.branches)
        #expect(taken.isSuperset(of: ["main", "fix/login", "feature/push", "calm-otter", "fix-login"]))
    }
}

@Suite("Row actions")
struct RowActionsTests {
    @Test("the confirm before a forced delete counts the commits the branch alone holds")
    func lost() {
        #expect(RowActions.lostTitle(1, branch: "fix-login") == "Delete fix-login and its 1 unpushed commit?")
        #expect(RowActions.lostTitle(3, branch: "fix-login") == "Delete fix-login and its 3 unpushed commits?")
        #expect(RowActions.lostMessage(1) == "That commit is on no other branch, remote or tag, so it will be lost.")
        #expect(RowActions.lostMessage(3) == "Those 3 commits are on no other branch, remote or tag, so they will be lost.")
    }
}

@Suite("Automation text")
struct AutomationTextTests {
    @Test("a schedule reads as days at a time, or as an interval")
    func when() {
        let days = { (d: [Int]) in AutomationText.when(.days(d, time: "09:00")) }
        #expect(days([0, 1, 2, 3, 4, 5, 6]) == "Every day at 09:00")
        #expect(days([5, 4, 3, 2, 1]) == "Weekdays at 09:00")
        #expect(days([6, 0]) == "Weekends at 09:00")
        #expect(days([5]) == "Fridays at 09:00")
        #expect(days([0, 5, 1]) == "Mon, Fri, Sun at 09:00")
        #expect([30, 60, 120, 1440, 2880, 90].map { AutomationText.when(.interval(everyMin: $0)) }
            == ["Every 30 minutes", "Every hour", "Every 2 hours", "Every day", "Every 2 days", "Every 90 minutes"])
    }

    @Test("the next run shows only while an automation is on")
    func trigger() {
        let on = Sample.automations.automations[0]
        #expect(AutomationText.trigger(on, now: Sample.now, timeZone: utc) == "Weekdays at 09:00 · next Sun 09:00")
        let off = Sample.automations.automations[1]
        #expect(AutomationText.trigger(off, now: Sample.now, timeZone: utc) == "Every 12 hours")
    }

    @Test("the last run says what needs a look, or when a good one ran")
    func last() {
        let runs = Sample.automations.runs
        #expect(AutomationText.last(runs[0], now: Sample.now, timeZone: utc) == "Working")
        #expect(AutomationText.last(runs[1], now: Sample.now, timeZone: utc) == "Fri 09:00")
    }
}

@Suite("Workspaces")
struct WorkspacesTests {
    @Test("a project's avatar takes the first letters of its first two words, or of its one word")
    func initials() {
        #expect(Workspaces.initials("glow-me") == "GM")
        #expect(Workspaces.initials("regulations_api_ingestion") == "RA")
        #expect(Workspaces.initials("employer backend") == "EB")
        #expect(Workspaces.initials("anywhere") == "AN")
        #expect(Workspaces.initials("") == "?")
    }

    @Test("a roll-up reads as a count and what those sessions are doing; idle ones say nothing")
    func rollUp() {
        #expect(Workspaces.label(.working, count: 1) == "1 running")
        #expect(Workspaces.label(.needsYou, count: 2) == "2 waiting")
        #expect(Workspaces.label(.failed, count: 1) == "1 failed")
        #expect(Workspaces.label(.idle, count: 3) == nil)
    }

    @Test("active now is what waits on you, then what runs, each newest first; the rest isn't active")
    func active() {
        let cards = [
            Sidebar.Card(id: "run-old", title: "", status: .working, createdAt: 0, updatedAt: 10),
            Sidebar.Card(id: "wait", title: "", status: .needsYou, createdAt: 0, updatedAt: 5),
            Sidebar.Card(id: "run-new", title: "", status: .working, createdAt: 0, updatedAt: 20),
            Sidebar.Card(id: "done", title: "", status: .done, createdAt: 0, updatedAt: 30),
        ]
        #expect(Workspaces.active(cards) == ["wait", "run-new", "run-old"])
    }

    @Test("a project's line counts its workspaces and says when it last moved")
    func projectLine() {
        #expect(Workspaces.line(workspaces: 2, latest: 1_000, now: 1_000) == "2 workspaces · now")
        #expect(Workspaces.line(workspaces: 1, latest: 0, now: 1_000) == "1 workspace")
    }
}
