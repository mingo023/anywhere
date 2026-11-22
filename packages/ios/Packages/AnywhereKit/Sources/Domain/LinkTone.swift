/// How a Mac's link reads at a glance, for the dot beside its name.
public enum LinkTone: Hashable, Sendable {
    case up
    case trying
    /// Can't be used until something changes: out of reach, revoked, or a side to update.
    case lost
    case off
}
