import Client
import Observation
import Wire

/// This phone's push registration and preferences on one Mac.
@MainActor
@Observable
public final class PushStore: HostStore {
    @ObservationIgnored let connection: DaemonConnection?

    public init(connection: DaemonConnection? = nil) {
        self.connection = connection
    }

    public func apply(_ message: ServerMessage) -> Bool { false }
}
