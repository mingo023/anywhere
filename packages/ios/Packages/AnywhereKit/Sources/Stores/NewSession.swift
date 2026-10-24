import Domain
import Wire

/// What starting a session takes, in order: a Local to make, a worktree to make, then the agent.
public struct LaunchPlan: Hashable, Sendable {
    public var local: ClientMessage.LocalCreate?
    /// Made first; the spec's checkout becomes the worktree it made.
    public var worktree: ClientMessage.WorktreeCreate?
    public var spec: LaunchSpec
}

/// The new session sheet's choices, as the desktop's new session page keeps them (`pocket/src/modals/new_session.rs`).
/// A phone can't pick a base, branch, PR, copy or setup in `agent.create`, so a worktree from any of them is made with
/// `worktree.create` first and the agent started in it.
public struct NewSessionDraft: Hashable, Sendable {
    public enum Place: Hashable, Sendable {
        /// An existing worktree, the project's own checkout included.
        case tree(String)
        /// An existing Local, by id.
        case local(String)
        case newLocal
        case newWorktree
    }

    /// Where a new worktree's branch comes from.
    public enum Source: Hashable, Sendable {
        /// A new branch named after the worktree, from `base`, or pocketd's default base when nil.
        case new
        /// An existing branch, local or on origin.
        case branch(String)
        /// A pull request, as typed.
        case pr(String)
    }

    public var project: String
    public var place: Place
    public var source: Source = .new
    public var base: String?
    /// The worktree's name as typed; empty takes a free one that pocketd renames after the prompt.
    public var name = ""
    public var seed = 0
    public var provider: ProviderId = .claude
    public var model: String?
    public var effort: String?
    public var access: Access = .edits
    public var plan = false
    public var account: String?
    /// Run without its TUI, driven only from the chat.
    public var chat = true
    public var prompt = ""

    public init(project: String, place: Place) {
        self.project = project
        self.place = place
    }

    /// The name the new worktree gets.
    public func worktreeName(taken: Set<String>) -> String {
        let typed = name.trimmingCharacters(in: .whitespaces)
        return typed.isEmpty ? WorktreeName.freeName(seed: seed, taken: taken) : typed
    }

    /// Why the draft can't start yet, if it can't. `maxAccess` is the most this phone may give an agent.
    public func problem(taken: Set<String>, maxAccess: String) -> String? {
        if project.isEmpty { return "Pick a project" }
        if !Self.accesses(max: maxAccess).contains(access) { return "Your Mac allows up to \(maxAccess) from this phone" }
        guard place == .newWorktree else { return nil }
        switch source {
        case .new:
            return WorktreeName.problem(worktreeName(taken: taken), taken: taken)
        case .branch(let b):
            return b.isEmpty ? "Pick a branch" : nil
        case .pr(let typed):
            return WorktreeName.parsePR(typed) == nil ? "Type a pull request number or link" : nil
        }
    }

    /// pocketd names a new branch from its prompt; the name sent is a placeholder.
    public var autoNames: Bool {
        place == .newWorktree && source == .new && base == nil && name.trimmingCharacters(in: .whitespaces).isEmpty
            && !prompt.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    /// The plan that starts this draft. `localId` names a new Local; `localName` is what it's called.
    public func plan(taken: Set<String>, localId: String, localName: String) -> LaunchPlan {
        let trimmed = prompt.trimmingCharacters(in: .whitespacesAndNewlines)
        var spec = LaunchSpec(
            project: project, checkout: .worktree(project), provider: provider, model: model.flatMap { $0.isEmpty ? nil : $0 },
            effort: effort.flatMap { $0.isEmpty ? nil : $0 }, access: access, plan: plan, prompt: trimmed.isEmpty ? nil : trimmed,
            account: account.flatMap { $0.isEmpty ? nil : $0 }, chat: chat ? true : nil)
        var plan = LaunchPlan(spec: spec)
        switch place {
        case .tree(let path):
            spec.checkout = .worktree(path)
        case .local(let id):
            spec.checkout = .local(id)
        case .newLocal:
            plan.local = .init(localId: localId, project: project, name: localName)
            spec.checkout = .local(localId)
        case .newWorktree:
            if source == .new && base == nil {
                spec.checkout = .new(NewWorktree(name: worktreeName(taken: taken), autoName: autoNames ? true : nil))
            } else {
                plan.worktree = worktreeCreate(taken: taken)
            }
        }
        plan.spec = spec
        return plan
    }

    /// The `worktree.create` that makes this draft's worktree; its `requestId` is set when it is sent.
    public func worktreeCreate(taken: Set<String>, setup: Bool = true) -> ClientMessage.WorktreeCreate {
        var create = ClientMessage.WorktreeCreate(requestId: "", project: project, name: "", setup: setup ? nil : false)
        switch source {
        case .new:
            create.name = worktreeName(taken: taken)
            create.base = base
        case .branch(let b):
            create.branch = WorktreeName.withoutRemote(b)
        case .pr(let typed):
            create.pr = WorktreeName.parsePR(typed)
        }
        return create
    }

    /// Picking another agent drops the model, effort and account picked for the last one; access holds for any.
    public mutating func pick(provider: ProviderId) {
        guard provider != self.provider else { return }
        self.provider = provider
        (model, effort, account) = (nil, nil, nil)
    }

    /// Ask, Edits, Auto and Full, up to `max`.
    public static func accesses(max: String) -> [Access] {
        let all: [Access] = [.ask, .edits, .auto, .full]
        guard let top = all.firstIndex(of: Access(rawValue: max)) else { return [.ask] }
        return Array(all[...top])
    }

    /// The access a draft starts with: Edits, or the most allowed below it.
    public static func defaultAccess(max: String) -> Access {
        accesses(max: max).last { $0 != .auto && $0 != .full } ?? .ask
    }

    /// The models a picker offers besides the agent's own default.
    public static func models(_ provider: ProviderId) -> [String] {
        provider == .claude ? ["opus", "sonnet", "haiku"] : []
    }

    public static func accessLabel(_ access: Access) -> String {
        switch access {
        case .ask: "Ask"
        case .edits: "Edits"
        case .auto: "Auto"
        case .full: "Full"
        default: access.rawValue.capitalized
        }
    }

    public static func accessHint(_ access: Access) -> String {
        switch access {
        case .ask: "Ask before commands and file changes."
        case .edits: "Auto-approve edits, ask before other actions."
        case .auto: "A reviewer model approves or denies actions."
        case .full: "Everything runs without asking."
        default: ""
        }
    }
}
