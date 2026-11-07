import Domain
import Wire

/// The composer under the transcript: what it offers and says, from the agent's state.
public enum Composer {
    public static let placeholder = "Ask, build, / for commands, @ for files"
    static let busyPlaceholder = "Send a follow-up into this turn…"
    static let stoppingPlaceholder = "Queue a follow-up…"
    public static let lineHeight: Double = 22
    /// The rows that fit 260 pt; past them the input scrolls.
    public static let maxRows = Int(260 / lineHeight)
    /// How long after a send the button ignores Stop: the send turns it into Stop under the finger.
    public static let stopGuardMs = 1000

    /// What stands under the transcript.
    public enum Mode: Hashable, Sendable {
        case prompt, viewOnly
        /// The session ended, or the Mac no longer lists it.
        case closed
        /// The agent waits on a permission; pocketd refuses prompts until it's answered.
        case permission(answerable: Bool)
        case question(answerable: Bool)
    }

    /// Prompts need the drive scope and answers the approve scope; a phone may hold either without the other.
    public static func mode(observeOnly: Bool, canApprove: Bool, ask: Ask?, closed: Bool = false) -> Mode {
        if closed { return .closed }
        return switch ask {
        case let a? where !a.questions.isEmpty: .question(answerable: canApprove)
        case .some: .permission(answerable: canApprove)
        case nil: observeOnly ? .viewOnly : .prompt
        }
    }

    /// The footer's hint at what the button does now, or with a hardware `keyboard`, what Return, Esc and ⌘Return do.
    public static func hint(_ mode: Mode, working: Bool, stopping: Bool, empty: Bool, canQueue: Bool, keyboard: Bool, mac: String) -> String? {
        switch mode {
        case .viewOnly, .closed: nil
        case .permission(answerable: false), .question(answerable: false): "Answer it on \(mac)"
        case .permission, .question: "Answer the request above to continue"
        case .prompt where !working: nil
        case .prompt where stopping: "Stopping…"
        case .prompt where empty || !canQueue: keyboard ? "Working · esc to stop" : "Working · tap ■ to stop"
        case .prompt: keyboard ? "⏎ send into this turn · ⌘⏎ queue for after" : "↑ sends into this turn · hold to queue"
        }
    }

    /// What stands in place of the box on a phone that may only watch.
    public static func viewOnly(mac: String) -> String {
        "This iPhone can only watch on \(mac). Pair it again to send prompts."
    }

    /// The input's rows in a chat `height` pt tall: those that fit 28% of it, so about five with the keyboard up.
    public static func rows(height: Double) -> Int {
        min(max(Int(height * 0.28 / lineHeight), 3), maxRows)
    }

    /// Whether a drag `dragged` pt down the transcript hides the keyboard. Scrolling only hides it once the finger reaches
    /// the keyboard, so a swipe that ends above it would leave it up.
    public static func hidesKeyboard(dragged: Double) -> Bool {
        dragged >= 24
    }

    /// Whether a Stop pressed at `now` (ms) comes too soon after the send at `sentAt` to be meant.
    public static func guardsStop(sentAt: Int?, now: Int) -> Bool {
        sentAt.map { now - $0 < stopGuardMs } ?? false
    }

    /// The input's placeholder: a turn being stopped takes a follow-up only into the queue.
    public static func placeholder(working: Bool, stopping: Bool, canQueue: Bool) -> String {
        switch (working && canQueue, stopping) {
        case (false, _): placeholder
        case (true, false): busyPlaceholder
        case (true, true): stoppingPlaceholder
        }
    }

    /// Whether Stop stands beside Steer: while the agent can be stopped. Once stopping, a draft queues and the stop is on its way.
    public static func stopBesideSteer(_ action: Action, interrupt: Bool) -> Bool {
        action == .steer && interrupt
    }

    /// Why the chat won't send `command`: only the agent's TUI runs it, and one the chat drives has none.
    public static func terminalOnlySaid(_ command: String, provider: String, chatOnly: Bool) -> String {
        chatOnly ? "\(command) isn't available in the chat." : "\(command) runs only in \(who(provider))'s terminal. Run it there."
    }

    /// On a turn, which an agent asking the user still is.
    public static func working(_ agent: AgentSummary) -> Bool {
        switch Status(status: agent.status.rawValue, failed: agent.failed ?? false, attached: agent.attached) {
        case .working, .needsYou: true
        default: false
        }
    }

    /// `busy` for the agent's chat; one not yet opened goes by status.
    public static func busy(_ agent: AgentSummary, _ session: Session?) -> Bool {
        guard let session else { return working(agent) }
        return ChatModel.busy(session.transcript, session.outgoing, working: working(agent))
    }
}
