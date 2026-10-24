import Fixtures
import Foundation
import Stores
import Testing
import Wire
@testable import FeatureSessions

private let utc = TimeZone(secondsFromGMT: 0)!

@Suite("Session text")
struct SessionTextTests {
    @Test("a session's second line is its status and what it is doing, else its model")
    func detail() {
        let working = Sample.agents.first { $0.id == "a-working" }!
        #expect(SessionText.detail(working, .working) == "Working")
        let needs = Sample.agents.first { $0.id == "a-needs" }!
        #expect(SessionText.detail(needs, .needsYou) == "Needs you · Approve Bash: rm -rf build")
        let compacting = Sample.agents.first { $0.id == "a-compacting" }!
        #expect(SessionText.detail(compacting, .working) == "Working · Compacting…")
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

    @Test("a Mac's link reads as online, reconnecting, or what to do about its end")
    func link() {
        #expect(SessionText.link(.online).text == "Online")
        #expect(SessionText.link(.retrying(seconds: 2)).tone == .waiting)
        #expect(SessionText.link(.ended(.revoked)).text == "Pair again")
        #expect(SessionText.link(.ended(.stopped)).text == "Offline")
    }

    @Test("an account shows the fuller of its usage windows")
    func account() {
        #expect(SessionText.account(Sample.accounts[0]) == "Work · 42%")
        #expect(SessionText.account(Sample.accounts[2]) == "Not signed in")
    }
}

@Suite("Places")
struct PlacesTests {
    @Test("a session can start in the own checkout, a Local or a worktree, and in new ones when the Mac allows")
    func options() {
        let project = Sample.projects[0]
        let all = Places.options(project: project, locals: Sample.locals, names: Sample.worktreeNames, canManage: true, canSpawn: true)
        #expect(all.map(\.label) == ["Local", "Review", "Phone terminal", "fix/login", "New Local", "New worktree"])
        #expect(all[1].place == .local("review"))
        let legacy = Places.options(project: project, locals: [], names: [:], canManage: false, canSpawn: true)
        #expect(legacy.map(\.label) == ["Local", "calm-otter", "fix/login", "New worktree"])
        #expect(!Places.options(project: project, locals: [], names: [:], canManage: false, canSpawn: false).contains { $0.place == .newWorktree })
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
