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
