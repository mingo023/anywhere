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
    @Test("a session's second line is what failed, else what it is doing, else a status its glyph alone can't tell apart, else its model")
    func detail() {
        let needs = Sample.agents.first { $0.id == "a-needs" }!
        #expect(SessionText.detail(needs, .needsYou) == "Approve Bash: rm -rf build")
        let compacting = Sample.agents.first { $0.id == "a-compacting" }!
        #expect(SessionText.detail(compacting, .working) == "Compacting…")
        var failed = Sample.agents.first { $0.id == "a-failed" }!
        #expect(SessionText.detail(failed, .failed) == "Failed")
        failed.error = "API Error: 529 overloaded"
        #expect(SessionText.detail(failed, .failed) == "API Error: 529 overloaded")
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

    @Test("a roll-up reads as a count and the status those sessions have, in the domain's words; idle ones say nothing")
    func rollUp() {
        #expect(Workspaces.label(.working, count: 2) == "2 working")
        #expect(Workspaces.label(.needsYou, count: 1) == "1 needs you")
        #expect(Workspaces.label(.failed, count: 1) == "1 failed")
        #expect(Workspaces.label(.done, count: 2) == "2 done")
        #expect(Workspaces.label(.idle, count: 3) == nil)
    }

    @Test("one failed and two done read as 1 failed, with the whole breakdown for VoiceOver")
    func breakdown() {
        let statuses: [Status] = [.done, .failed, .done, .idle]
        let roll = Sidebar.rollUp(statuses)!
        #expect(Workspaces.label(roll.status, count: roll.count) == "1 failed")
        #expect(Workspaces.breakdown(statuses) == "1 failed, 2 done")
    }

    @Test("working lists the sessions working now, the last to move first")
    func working() {
        let cards = [
            Sidebar.Card(id: "run-old", title: "", status: .working, createdAt: 0, updatedAt: 10),
            Sidebar.Card(id: "wait", title: "", status: .needsYou, createdAt: 0, updatedAt: 5),
            Sidebar.Card(id: "run-new", title: "", status: .working, createdAt: 0, updatedAt: 20),
        ]
        #expect(Workspaces.working(cards) == ["run-new", "run-old"])
    }

    @Test("a project's line counts its worktrees and Locals")
    func projectLine() {
        #expect(Workspaces.line(worktrees: 2, locals: 1) == "2 worktrees · 1 Local")
        #expect(Workspaces.line(worktrees: 1, locals: 0) == "1 worktree")
        #expect(Workspaces.line(worktrees: 0, locals: 2) == "2 Locals")
        #expect(Workspaces.line(worktrees: 0, locals: 0) == "No worktrees")
    }

    @Test("a project's corner dot shows the most urgent session that wants a look, and none for work alone")
    func alert() {
        #expect(Workspaces.alert([.done, .failed, .needsYou]) == .needsYou)
        #expect(Workspaces.alert([.done, .failed]) == .failed)
        #expect(Workspaces.alert([.working, .done]) == .done)
        #expect(Workspaces.alert([.working, .idle]) == nil)
    }

    @Test("the home header says how the Mac is reached and how many sessions it holds")
    func headerStatus() {
        #expect(HomeHeader.status(.online, sessions: 1) == "Connected over Tailscale · 1 session")
        #expect(HomeHeader.status(.reconnecting, sessions: 40) == "\(Reachability.reconnecting.label) · 40 sessions")
    }

    @Test("a worktree's header names its branch against its base, and the bare branch before git answers")
    func branchLine() {
        let head = GitHead(Sample.gitStatus(files: 1))
        #expect(TreeHeader.branchLine(branch: nil, head: head) == nil)
        #expect(TreeHeader.branchLine(branch: "fix", head: nil) == "fix")
        #expect(TreeHeader.branchLine(branch: "fix", head: head) == "fix from main · 4 ahead")
    }
}

@Suite("Folds")
struct FoldsTests {
    @Test("a project opens by itself while a session in it is working or wants a look, and the user's choice sticks either way")
    func sticks() {
        var folds = Folds()
        #expect(folds.isOpen("p", lively: true))
        #expect(!folds.isOpen("p", lively: false))
        folds.toggle("p", lively: true)
        #expect(!folds.isOpen("p", lively: true), "closed while busy stays closed")
        folds.toggle("q", lively: false)
        #expect(folds.isOpen("q", lively: false), "opened while idle stays open when its last session finishes")
        #expect(Folds(rawValue: folds.rawValue) == folds)
    }
}

@Suite("Up next text")
struct UpNextTextTests {
    @Test("a Needs you row shows the ask it waits on, a Failed row what failed, else what the session is doing")
    func look() {
        let needs = Sample.agents.first { $0.id == "a-needs" }!
        #expect(SessionText.look(needs, .needsYou, requests: [Sample.permission]) == "Wants to run rm -rf build")
        #expect(SessionText.look(needs, .needsYou, requests: [Sample.question]) == "Which colour?")
        #expect(SessionText.look(needs, .needsYou, requests: []) == "Approve Bash: rm -rf build")
        var failed = Sample.agents.first { $0.id == "a-failed" }!
        #expect(SessionText.look(failed, .failed, requests: []) == "Failed")
        failed.activity = "Running tests"
        #expect(SessionText.look(failed, .failed, requests: []) == "Running tests")
        failed.error = "API Error: 529 overloaded"
        #expect(SessionText.look(failed, .failed, requests: []) == "API Error: 529 overloaded")
        let working = Sample.agents.first { $0.id == "a-working" }!
        #expect(SessionText.look(working, .working, requests: []) == "Opus 5.5")
    }

    @Test("a row leads with what the session asks or what failed, and otherwise its title")
    func headline() {
        let needs = Sample.agents.first { $0.id == "a-needs" }!
        #expect(SessionText.headline(needs, .needsYou, requests: [Sample.permission]) == "Wants to run rm -rf build")
        #expect(SessionText.headline(needs, .needsYou, requests: []) == SessionText.title(needs))
        var failed = Sample.agents.first { $0.id == "a-failed" }!
        #expect(SessionText.headline(failed, .failed, requests: []) == SessionText.title(failed))
        failed.error = "API Error: 529 overloaded"
        #expect(SessionText.headline(failed, .failed, requests: []) == "API Error: 529 overloaded")
        #expect(SessionText.headline(failed, .working, requests: []) == SessionText.title(failed))
    }

    @Test("a finished session says how it ended, and one still going doesn't")
    func outcome() {
        #expect(SessionText.outcome(.failed) == "Failed")
        #expect(SessionText.outcome(.done) == "Finished")
        #expect(SessionText.outcome(.working) == nil)
        #expect(SessionText.outcome(.needsYou) == nil)
    }

    @Test("only a plain yes or no, short enough to read whole, is answered on its row")
    func answersInPlace() {
        #expect(SessionText.answersInPlace(Sample.permission))
        #expect(!SessionText.answersInPlace(Sample.question))
        let long = PermissionRequest(requestId: "r", agentId: "a", toolName: "Bash", detail: .shell(command: String(repeating: "x", count: 300), description: "long"))
        #expect(!SessionText.answersInPlace(long))
    }

    @Test("a worktree's pull request tile says its state and how many checks passed")
    func pullRequestState() {
        let open = PrItem(tree: "/t", number: 7, title: "", state: "OPEN", draft: false, gate: .ready, checks: .init(passed: 3, failed: 1, pending: 0))
        #expect(SessionText.pullRequestState(open) == "Open · 3 of 4 checks")
        let draft = PrItem(tree: "/t", number: 7, title: "", state: "OPEN", draft: true, gate: .ready, checks: .init(passed: 0, failed: 0, pending: 0))
        #expect(SessionText.pullRequestState(draft) == "Draft")
        let merged = PrItem(tree: "/t", number: 7, title: "", state: "MERGED", draft: false, gate: .ready, checks: .init(passed: 1, failed: 0, pending: 0))
        #expect(SessionText.pullRequestState(merged) == "Merged · 1 of 1 checks")
    }

    @Test("the steps still to come are those after the last one reported")
    func stepsLeft() {
        #expect(SessionText.stepsLeft(after: "copy") == ["setup", "agent"])
        #expect(SessionText.stepsLeft(after: "agent") == [])
        #expect(SessionText.stepsLeft(after: nil) == [])
    }

    @Test("a pull request reads as its number, Draft, and its checks, and its badge takes the colour of what it waits on")
    func pullRequest() {
        let item = PrItem(tree: "/t", number: 42, title: "", state: "OPEN", draft: false, gate: .checksFailed, checks: .init(passed: 3, failed: 1, pending: 0))
        #expect(SessionText.pullRequest(item) == "#42 · 1 failed · 3 passed")
        #expect(PrBadge.tone(item) == .failed)
        var draft = item
        draft.draft = true
        #expect(SessionText.pullRequest(draft) == "#42 · Draft · 1 failed · 3 passed")
        #expect(PrBadge.tone(draft) == .neutral)
        var running = item
        (running.checks, running.gate) = (.init(passed: 1, failed: 0, pending: 2), .checksRunning)
        #expect(PrBadge.tone(running) == .working)
    }
}

@Suite("Starting a session")
struct StartingTests {
    @Test("a start's row says its step, waits for the Mac when the link dropped, and names the agent it starts")
    func steps() {
        #expect(SessionText.creating(step: nil, waiting: false, provider: "claude") == "Asking your Mac…")
        #expect(SessionText.creating(step: "worktree", waiting: false, provider: "claude") == "Creating git worktree…")
        #expect(SessionText.creating(step: "agent", waiting: false, provider: "claude") == "Starting Claude…")
        #expect(SessionText.creating(step: "agent", waiting: true, provider: "claude") == "Waiting for your Mac…")
    }

    @Test("Start says why it waits on the Mac: offline first, then agents still loading")
    func waiting() {
        #expect(NewSessionView.waiting(online: false, providers: false, failed: false, mac: "Studio") == "Studio isn't connected. Start turns on when it's back.")
        #expect(NewSessionView.waiting(online: true, providers: false, failed: false, mac: "Studio") == "Loading agents from Studio…")
        #expect(NewSessionView.waiting(online: true, providers: false, failed: true, mac: "Studio") == nil)
        #expect(NewSessionView.waiting(online: true, providers: true, failed: false, mac: "Studio") == nil)
    }

    @Test("a start is titled by its prompt's first line and listed in the worktree or Local it runs in")
    func row() {
        #expect(NewSessionView.title("  Fix the login\nand the tests") == "Fix the login")
        #expect(NewSessionView.title("") == "")
        #expect(NewSessionView.key(.worktree("/w"), makesWorktree: false) == "/w")
        #expect(NewSessionView.key(.local("local-1"), makesWorktree: false) == "local-1")
        #expect(NewSessionView.key(.worktree("/p"), makesWorktree: true) == nil)
        #expect(NewSessionView.key(.new(NewWorktree(name: "x")), makesWorktree: false) == nil)
    }

    @Test("a route's place is a Local when one has that id, else a worktree")
    func place() {
        #expect(Places.place(key: "review", locals: Sample.locals) == .local("review"))
        #expect(Places.place(key: "/w/calm-otter", locals: Sample.locals) == .tree("/w/calm-otter"))
    }
}

@Suite("Tree links")
struct TreeLinksTests {
    @Test("Changes counts the changed files, and says No changes at none")
    func changes() {
        #expect(TreeLinks.changes(nil) == nil)
        #expect(TreeLinks.changes(0) == "No changes")
        #expect(TreeLinks.changes(1) == "1 file")
        #expect(TreeLinks.changes(1_200) == "1200 files")
    }

    @Test("the pull request tile names the PR, offers to create one once gh is ready, and hides without a GitHub remote or on a Mac that can't read GitHub")
    func pullRequest() {
        let item = PrItem(tree: "/t", number: 7, title: "", state: "OPEN", draft: false, gate: .ready, checks: .init(passed: 2, failed: 0, pending: 0))
        #expect(TreeLinks.pullRequest(item, gh: .ready, problem: nil, readsGitHub: true) == "Pull request")
        #expect(TreeLinks.pullRequest(nil, gh: .ready, problem: nil, readsGitHub: true) == "Create pull request")
        #expect(TreeLinks.pullRequest(nil, gh: .signedOut, problem: nil, readsGitHub: true) == "Pull request")
        #expect(TreeLinks.pullRequest(item, gh: .ready, problem: .noRemote, readsGitHub: true) == nil)
        #expect(TreeLinks.pullRequest(nil, gh: nil, problem: nil, readsGitHub: false) == nil)
    }
}

@Suite("A place's chats")
@MainActor
struct PlaceChatsTests {
    let host = HostModel.offline(hello: Sample.hello, messages: Sample.snapshot)

    @Test("a worktree's chats are the sessions it lists")
    func worktree() {
        let chats = host.chats(in: .worktree(mac: host.mac, path: "/Users/mingo/.worktrees/anywhere/fix-login"))
        #expect(chats.count == 2)
    }

    @Test("a project's chats are those of its main checkout")
    func project() throws {
        let path = Sample.projects[0].path
        let own = try #require(host.homeList().sections.first { $0.id == path }?.trees.first { $0.kind == .own })
        #expect(host.chats(in: .project(mac: host.mac, path: path)) == own.sessions.map { .chat(mac: host.mac, agentId: $0) })
    }

    @Test("a place the Mac doesn't list has no chats")
    func unknown() {
        #expect(host.chats(in: .worktree(mac: host.mac, path: "/nowhere")).isEmpty)
    }
}
