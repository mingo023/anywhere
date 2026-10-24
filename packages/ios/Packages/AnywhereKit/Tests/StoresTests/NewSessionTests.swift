import Domain
import Testing
import Wire
@testable import Stores

private let project = "/Users/mingo/dev/anywhere"

private func draft(_ place: NewSessionDraft.Place, _ change: (inout NewSessionDraft) -> Void = { _ in }) -> NewSessionDraft {
    var d = NewSessionDraft(project: project, place: place)
    change(&d)
    return d
}

@Suite("New session")
struct NewSessionTests {
    @Test("an existing worktree or Local starts the agent there, with only the choices made")
    func existing() {
        let plan = draft(.tree("/w")) { $0.prompt = "  fix it \n"; $0.model = ""; $0.account = "acc-work" }.plan(taken: [], localId: "l", localName: "Local 1")
        #expect(plan.local == nil && plan.worktree == nil)
        #expect(plan.spec == LaunchSpec(project: project, checkout: .worktree("/w"), provider: .claude, access: .edits, plan: false,
                                        prompt: "fix it", account: "acc-work", chat: true))
        #expect(draft(.local("review")).plan(taken: [], localId: "l", localName: "x").spec.checkout == .local("review"))
    }

    @Test("a new Local is made first under the id the agent then starts in")
    func newLocal() {
        let plan = draft(.newLocal).plan(taken: [], localId: "local-1f", localName: "Local 2")
        #expect(plan.local == .init(localId: "local-1f", project: project, name: "Local 2"))
        #expect(plan.spec.checkout == .local("local-1f"))
    }

    @Test("a new worktree off the default base goes in agent.create, named from the prompt when no name is typed")
    func newWorktree() {
        let named = draft(.newWorktree) { $0.prompt = "add dark mode" }.plan(taken: [], localId: "", localName: "")
        guard case .new(let new) = named.spec.checkout else { Issue.record("not a new worktree"); return }
        #expect(new.autoName == true && new.name?.isEmpty == false)
        #expect(named.worktree == nil)
        let typed = draft(.newWorktree) { $0.prompt = "x"; $0.name = "dark-mode" }.plan(taken: [], localId: "", localName: "")
        #expect(typed.spec.checkout == .new(NewWorktree(name: "dark-mode")))
    }

    @Test("a worktree from a base, a branch or a PR is made with worktree.create first, since a phone can't ask agent.create for one")
    func twoSteps() {
        let base = draft(.newWorktree) { $0.base = "origin/dev"; $0.name = "t" }.plan(taken: [], localId: "", localName: "")
        #expect(base.worktree == .init(requestId: "", project: project, name: "t", base: "origin/dev"))
        let branch = draft(.newWorktree) { $0.source = .branch("origin/fix/login") }.plan(taken: [], localId: "", localName: "")
        #expect(branch.worktree == .init(requestId: "", project: project, name: "", branch: "fix/login"))
        let pr = draft(.newWorktree) { $0.source = .pr("https://github.com/o/r/pull/118") }.plan(taken: [], localId: "", localName: "")
        #expect(pr.worktree?.pr == 118)
        let alone = draft(.newWorktree) { $0.name = "t" }.worktreeCreate(taken: [], setup: false)
        #expect(alone == .init(requestId: "", project: project, name: "t", setup: false), "a worktree made on its own may skip setup")
    }

    @Test("a draft says what stops it: a taken name, no branch, an unreadable PR, more access than the Mac allows")
    func problems() {
        #expect(draft(.newWorktree) { $0.name = "main" }.problem(taken: ["main"], maxAccess: "edits") == WorktreeName.takenProblem)
        #expect(draft(.newWorktree) { $0.source = .branch("") }.problem(taken: [], maxAccess: "edits") == "Pick a branch")
        #expect(draft(.newWorktree) { $0.source = .pr("soon") }.problem(taken: [], maxAccess: "edits") != nil)
        #expect(draft(.tree("/w")) { $0.access = .auto }.problem(taken: [], maxAccess: "edits") != nil)
        #expect(draft(.tree("/w")).problem(taken: [], maxAccess: "edits") == nil)
        #expect(draft(.newWorktree).problem(taken: [], maxAccess: "ask") != nil, "Edits is more than Ask")
    }

    @Test("access choices stop at the phone's ceiling, and a draft starts at Edits or below")
    func access() {
        #expect(NewSessionDraft.accesses(max: "edits") == [.ask, .edits])
        #expect(NewSessionDraft.accesses(max: "full") == [.ask, .edits, .auto, .full])
        #expect(NewSessionDraft.accesses(max: "nonsense") == [.ask])
        #expect(NewSessionDraft.defaultAccess(max: "full") == .edits)
        #expect(NewSessionDraft.defaultAccess(max: "ask") == .ask)
    }

    @Test("picking another agent drops the model, effort and account picked for the last")
    func provider() {
        var d = draft(.tree("/w")) { $0.model = "opus"; $0.effort = "max"; $0.account = "a"; $0.access = .ask }
        d.pick(provider: .codex)
        #expect(d.provider == .codex && d.model == nil && d.effort == nil && d.account == nil && d.access == .ask)
    }

    @Test("trying again after a failed start begins in the worktree or Local it already made")
    func resume() {
        var d = draft(.newWorktree) { $0.source = .branch("fix/login") }
        d.resume(after: Creation(kind: .session, madeTree: "/w/fix-login"))
        #expect(d.place == .tree("/w/fix-login"))
        var l = draft(.newLocal)
        l.resume(after: Creation(kind: .session, madeLocal: "local-1f"))
        #expect(l.place == .local("local-1f"))
        var nothing = draft(.newLocal)
        nothing.resume(after: Creation(kind: .session))
        #expect(nothing.place == .newLocal)
    }
}
