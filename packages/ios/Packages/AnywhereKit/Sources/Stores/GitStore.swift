import Client
import Observation
import Wire

/// Status, diffs, log and commits of the trees one Mac watches.
@MainActor
@Observable
public final class GitStore: HostStore {
    @ObservationIgnored let connection: DaemonConnection?

    public init(connection: DaemonConnection? = nil) {
        self.connection = connection
    }

    public func apply(_ message: ServerMessage) -> Bool { false }
}
