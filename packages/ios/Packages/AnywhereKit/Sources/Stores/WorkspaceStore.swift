import Client
import Observation
import Wire

/// Projects, worktrees, Locals, worktree names, automations, branches and accounts of one Mac.
@MainActor
@Observable
public final class WorkspaceStore: HostStore {
    @ObservationIgnored let connection: DaemonConnection?

    public init(connection: DaemonConnection? = nil) {
        self.connection = connection
    }

    public func apply(_ message: ServerMessage) -> Bool { false }
}
