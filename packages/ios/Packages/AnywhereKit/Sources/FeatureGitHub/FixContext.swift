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
    var target = ChatTarget.none
    /// The phone holds the drive scope.
    var canDrive = false
    /// Sends a prompt to a session; false when it can't take one now.
    var send: (@MainActor (_ agentId: String, _ prompt: String) -> Bool)?
    var open: ((Route) -> Void)?

    /// Problems can go to a session, or start one.
    var offered: Bool { canDrive && (target == .none ? open != nil : send != nil) }
}
