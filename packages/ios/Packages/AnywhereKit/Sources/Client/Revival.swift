/// What to do with a link when the network changes or the app comes back to the front.
public enum Revival: Sendable, Equatable {
    case none
    /// Ping a link that looks up.
    case verify
    /// Dial now if the link is waiting out a backoff.
    case reconnect
    /// Drop the socket and dial a fresh one at once: it may be dead, and waiting on it would stall every request.
    case redial

    /// How long in the background before an online socket is presumed dead: pocketd closes one whose ping goes
    /// unanswered for 20s plus 10s, and a suspended app answers none.
    public static let awayLimit = Duration.seconds(30)

    public static func onPath(online: Bool, satisfied: Bool, interfacesChanged: Bool) -> Revival {
        guard satisfied else { return .none }
        guard online else { return .reconnect }
        return interfacesChanged ? .redial : .verify
    }

    public static func onForeground(online: Bool, away: Duration) -> Revival {
        guard online else { return .reconnect }
        return away > awayLimit ? .redial : .verify
    }
}
