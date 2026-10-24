import Client
import Observation
import Wire

/// One Mac's terminals and the attached screens fed by binary frames.
@MainActor
@Observable
public final class TerminalStore: HostStore {
    @ObservationIgnored let connection: DaemonConnection?

    public init(connection: DaemonConnection? = nil) {
        self.connection = connection
    }

    public func apply(_ message: ServerMessage) -> Bool { false }

    /// Takes in a terminal's bytes; true when an attached screen took them.
    public func apply(_ frame: TerminalFrame) -> Bool { false }
}
