/// The notification categories and actions pocketd's alerts name, registered at launch.
public enum NotificationCategory {
    /// Allow, Deny.
    public static let permission = "PERM"
    /// Allow, Deny, Deny with note.
    public static let permissionNote = "PERM_NOTE"
    /// Open: its options need the app.
    public static let question = "QUESTION"
    /// Reply.
    public static let done = "DONE"
    /// Reply.
    public static let failed = "FAILED"
    public static let automation = "AUTOMATION"
}

public enum NotificationAction {
    public static let allow = "ALLOW"
    public static let deny = "DENY"
    public static let denyNote = "DENY_NOTE"
    public static let open = "OPEN"
    public static let reply = "REPLY"
}

/// One action as the app registers it; the app turns these into UNNotificationActions.
public struct ActionSpec: Hashable, Sendable {
    public enum Style: Hashable, Sendable {
        /// Runs in the background once the phone is unlocked.
        case unlocked
        /// As `unlocked`, shown in red.
        case destructive
        /// Opens the app.
        case foreground
    }

    public let id: String
    public let title: String
    public let style: Style
    /// A text field's button title and placeholder; nil for a plain button.
    public let input: TextInput?

    public struct TextInput: Hashable, Sendable {
        public let button: String
        public let placeholder: String
    }

    static let allow = ActionSpec(id: NotificationAction.allow, title: "Allow", style: .unlocked, input: nil)
    static let deny = ActionSpec(id: NotificationAction.deny, title: "Deny", style: .destructive, input: nil)
    static let denyNote = ActionSpec(id: NotificationAction.denyNote, title: "Deny with Note…", style: .destructive, input: TextInput(button: "Deny", placeholder: "Tell the agent why"))
    static let open = ActionSpec(id: NotificationAction.open, title: "Open", style: .foreground, input: nil)
    static let reply = ActionSpec(id: NotificationAction.reply, title: "Reply…", style: .unlocked, input: TextInput(button: "Send", placeholder: "Message"))
}

extension NotificationCategory {
    /// Each category's actions, in the order the banner shows them.
    public static let actions: [(id: String, actions: [ActionSpec])] = [
        (permission, [.allow, .deny]),
        (permissionNote, [.allow, .deny, .denyNote]),
        (question, [.open]),
        (done, [.reply, .open]),
        (failed, [.reply, .open]),
        (automation, [.open]),
    ]
}

/// What an action on a notification asks the Mac to do.
public enum NotificationReply: Hashable, Sendable {
    case resolve(requestId: String, decision: String, message: String?)
    case prompt(agentId: String, text: String)
    /// Open the app at the notification's link.
    case open
    /// Nothing to send: an empty reply, or an action on a notification that lacks what it needs.
    case none

    /// The reply to `action` on a notification with `userInfo`; `text` is what a text action's field held.
    public static func of(action: String, userInfo: [String: String], text: String?) -> NotificationReply {
        let typed = text?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        switch action {
        case NotificationAction.allow, NotificationAction.deny, NotificationAction.denyNote:
            guard let request = userInfo["request"] else { return .open }
            let decision = action == NotificationAction.allow ? "allow" : "deny"
            return .resolve(requestId: request, decision: decision, message: action == NotificationAction.denyNote && !typed.isEmpty ? typed : nil)
        case NotificationAction.reply:
            guard let agent = userInfo["agent"] else { return .open }
            return typed.isEmpty ? .none : .prompt(agentId: agent, text: typed)
        default:
            return .open
        }
    }
}
