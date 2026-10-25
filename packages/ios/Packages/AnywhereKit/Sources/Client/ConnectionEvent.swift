import Wire

/// What a `DaemonConnection` reports, in the order it happened.
public enum ConnectionEvent: Hashable, Sendable {
    /// The Mac accepted the hello; its snapshot follows as messages.
    case connected(HelloOk)
    /// A push, an interim reply, or a reply nobody is awaiting.
    case message(ServerMessage)
    case terminal(TerminalFrame)
    case closed(CloseOutcome)
}

public enum DaemonError: Error, Hashable, Sendable {
    /// Not connected to the Mac right now.
    case offline
    case timedOut
    /// The connection dropped before the reply came.
    case closed
    case cancelled
    /// The Mac refused the request.
    case server(ServerError)
    case encoding(String)
}
