import Client
import Observation
import Wire

/// File trees, previews and go-to-file results of one Mac's trees.
@MainActor
@Observable
public final class FilesStore: HostStore {
    @ObservationIgnored let connection: DaemonConnection?

    public init(connection: DaemonConnection? = nil) {
        self.connection = connection
    }

    public func apply(_ message: ServerMessage) -> Bool { false }
}
