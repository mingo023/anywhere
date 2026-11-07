import Domain
import Stores

/// What the PR screens need to send its problems to the worktree's agent (GH-2): the Mac, the session that takes them,
/// whether this phone may prompt, how to send, and how to open the chat or a new session.
struct FixContext {
    var mac = ""
    /// The agent whose chat opened the PR, passed on to Comments.
    var opener: String?
    /// The project holding the tree, for a session started from it.
    var project: String?
    /// As read when the screen opened, for what the buttons say.
    var target = ChatTarget.none
    /// Reads the target again when problems are sent, so the screens needn't watch every session; nil keeps `target`.
    var retarget: (@MainActor () -> ChatTarget)?
    /// The phone holds the drive scope.
    var canDrive = false
    /// Sends a prompt to a session; false when it can't take one now.
    var send: (@MainActor (_ agentId: String, _ prompt: String) -> Bool)?
    var open: ((Route) -> Void)?

    /// Problems can go to a session, or start one.
    var offered: Bool { canDrive && (target == .none ? open != nil : send != nil) }

    /// Threads can go one by one too, unless the agent already fixes something on the PR (`can_send`).
    func canSend(fixing: SentFixes?) -> Bool { offered && fixing == nil }
}
