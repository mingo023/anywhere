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
        let plan = draft(.tree("/w")) { $0.prompt = "  fix it \n"; $0.model = ""; $0.account = "acc-work" }.plan(taken: [], localId: "l", localName: "Local 1", neverAsks: false)
        #expect(plan.local == nil && plan.worktree == nil)
        #expect(plan.spec == LaunchSpec(project: project, checkout: .worktree("/w"), provider: .claude, access: .edits, plan: false,
                                        prompt: "fix it", account: "acc-work", chat: true))
        #expect(draft(.local("review")).plan(taken: [], localId: "l", localName: "x", neverAsks: false).spec.checkout == .local("review"))
    }

    @Test("a new Local is made first under the id the agent then starts in")
    func newLocal() {
        let plan = draft(.newLocal).plan(taken: [], localId: "local-1f", localName: "Local 2", neverAsks: false)
        #expect(plan.local == .init(localId: "local-1f", project: project, name: "Local 2"))
        #expect(plan.spec.checkout == .local("local-1f"))
    }

    @Test("a new worktree off the default base goes in agent.create, named from the prompt when no name is typed")
    func newWorktree() {
        let named = draft(.newWorktree) { $0.prompt = "add dark mode" }.plan(taken: [], localId: "", localName: "", neverAsks: false)
        guard case .new(let new) = named.spec.checkout else { Issue.record("not a new worktree"); return }
        #expect(new.autoName == true && new.name?.isEmpty == false)
        #expect(named.worktree == nil)
        let typed = draft(.newWorktree) { $0.prompt = "x"; $0.name = "dark-mode" }.plan(taken: [], localId: "", localName: "", neverAsks: false)
        #expect(typed.spec.checkout == .new(NewWorktree(name: "dark-mode")))
    }

    @Test("a worktree from a base, a branch or a PR is made with worktree.create first, since a phone can't ask agent.create for one")
    func twoSteps() {
        let base = draft(.newWorktree) { $0.base = "origin/dev"; $0.name = "t" }.plan(taken: [], localId: "", localName: "", neverAsks: false)
        #expect(base.worktree == .init(requestId: "", project: project, name: "t", base: "origin/dev"))
        let branch = draft(.newWorktree) { $0.source = .branch("origin/fix/login") }.plan(taken: [], localId: "", localName: "", neverAsks: false)
        #expect(branch.worktree == .init(requestId: "", project: project, name: "", branch: "fix/login"))
        let pr = draft(.newWorktree) { $0.source = .pr("https://github.com/o/r/pull/118") }.plan(taken: [], localId: "", localName: "", neverAsks: false)
        #expect(pr.worktree?.pr == 118)
        let alone = draft(.newWorktree) { $0.name = "t" }.worktreeCreate(taken: [], setup: false)
        #expect(alone == .init(requestId: "", project: project, name: "t", setup: false), "a worktree made on its own may skip setup")
    }

    @Test("a draft says what stops it: a taken name, no branch, an unreadable PR")
    func problems() {
        #expect(draft(.newWorktree) { $0.name = "main" }.problem(taken: ["main"]) == WorktreeName.takenProblem)
        #expect(draft(.newWorktree) { $0.source = .branch("") }.problem(taken: []) == "Pick a branch")
        #expect(draft(.newWorktree) { $0.source = .pr("soon") }.problem(taken: []) != nil)
        #expect(draft(.tree("/w")) { $0.access = .full }.problem(taken: []) == nil)
        #expect(draft(.tree("/w")).problem(taken: []) == nil)
    }

    @Test("a draft starts on the Mac's own default, else Edits")
    func access() {
        #expect(NewSessionDraft.defaultAccess() == .edits)
        #expect(NewSessionDraft.defaultAccess(mac: "auto") == .auto)
        #expect(NewSessionDraft.defaultAccess(mac: "settings") == .settings)
        #expect(NewSessionDraft.defaultAccess(mac: "nonsense") == .edits)
    }

    @Test("a draft never starts on Full by itself, even when the Mac's default is Full")
    func neverStartsOnFull() {
        #expect(NewSessionDraft.defaultAccess(mac: "full") == .edits)
    }

    @Test("picking another agent drops the model, effort and account picked for the last")
    func provider() {
        var d = draft(.tree("/w")) { $0.model = "opus"; $0.effort = "max"; $0.account = "a"; $0.access = .ask }
        d.pick(provider: .codex)
        #expect(d.provider == .codex && d.model == nil && d.effort == nil && d.account == nil && d.access == .ask)
        d.effort = "high"
        d.pick(provider: "pi")
        #expect(d.provider == "pi" && d.effort == nil && d.plan(taken: [], localId: "l", localName: "x", neverAsks: false).spec.provider == "pi")
    }

    @Test("an agent that never asks before a tool starts with Full whatever the pill held, and Start asks first as picking Full does")
    func neverAsks() {
        for held in [Access.settings, .ask, .edits, .auto] {
            let d = draft(.tree("/w")) { $0.provider = "pi"; $0.access = held }
            #expect(d.access(neverAsks: true) == .full && d.confirmsStart(neverAsks: true), "\(held.rawValue)")
            #expect(d.plan(taken: [], localId: "l", localName: "x", neverAsks: true).spec.access == .full, "\(held.rawValue)")
        }
        let asks = draft(.tree("/w")) { $0.access = .ask }
        #expect(asks.access(neverAsks: false) == .ask && !asks.confirmsStart(neverAsks: false))
        #expect(asks.plan(taken: [], localId: "l", localName: "x", neverAsks: false).spec.access == .ask)
    }

    @Test("Full picked on purpose was already confirmed, so Start doesn't ask again")
    func fullPickedOnPurposeStartsAtOnce() {
        let d = draft(.tree("/w")) { $0.provider = "omp"; $0.access = .full }
        #expect(!d.confirmsStart(neverAsks: true) && !d.confirmsStart(neverAsks: false))
    }

    @Test("only Claude's models are offered by name: another agent's catalog is on the Mac")
    func models() {
        #expect(NewSessionDraft.models(.claude) == ["opus", "sonnet", "haiku"])
        for provider: ProviderId in ["codex", "cursor", "grok", "opencode", "pi", "omp", "fx", "hermes", "antigravity", "gemini"] {
            #expect(NewSessionDraft.models(provider).isEmpty, "\(provider.rawValue)")
        }
    }

    @Test("another project starts a fresh worktree: the last one's base, branch, name and workspace don't carry over")
    func anotherProjectStartsFresh() {
        var draft = NewSessionDraft(project: "/a", place: .tree("/a/w"))
        (draft.source, draft.base, draft.name) = (.branch("fix"), "dev", "mine")
        draft.pick(project: "/b")
        #expect(draft.project == "/b" && draft.place == .newWorktree && draft.source == .new && draft.base == nil && draft.name.isEmpty)
        draft.place = .newLocal
        draft.pick(project: "/a")
        #expect(draft.place == .newLocal)
    }
}
