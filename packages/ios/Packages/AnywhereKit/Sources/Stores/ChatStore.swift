import Client
import Observation
import Wire

/// One open agent's chat: its timeline folded into rows, the outgoing queue and the composer's state.
@MainActor
@Observable
public final class ChatStore: HostStore {
    public let agentId: String
    @ObservationIgnored let connection: DaemonConnection?

    public init(agentId: String, connection: DaemonConnection? = nil) {
        self.agentId = agentId
        self.connection = connection
    }

    public func apply(_ message: ServerMessage) -> Bool { false }
}
