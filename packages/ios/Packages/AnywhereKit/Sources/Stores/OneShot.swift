import Client
import Foundation
import Wire

/// One request to a Mac on a connection of its own, for a notification action or a Live Activity button that runs
/// while the app's own connection is down or suspended.
enum OneShot {
    static func request(_ message: ClientMessage, credentials: Credentials, within limit: Duration = .seconds(15)) async throws -> ServerMessage {
        let connection = DaemonConnection(config: .init(host: credentials.host, token: credentials.deviceToken, clientId: credentials.clientId))
        defer { Task { await connection.stop() } }
        await connection.start()
        try await withThrowingTaskGroup(of: Void.self) { group in
            group.addTask {
                for await event in connection.events {
                    switch event {
                    case .connected: return
                    case .closed(.retrying): continue
                    case .closed: throw DaemonError.closed
                    default: continue
                    }
                }
                throw DaemonError.closed
            }
            group.addTask {
                try await Task.sleep(for: limit)
                throw DaemonError.timedOut
            }
            try await group.next()
            group.cancelAll()
        }
        return try await connection.request(message)
    }
}

extension AppModel {
    /// Sends `message` to the Mac with this serverId: on its connection when online, else on one of its own.
    public func deliver(_ message: ClientMessage, mac serverId: String) async throws {
        guard let host = host(mac: serverId) else { throw DaemonError.offline }
        if host.link == .online {
            do {
                _ = try await host.connection.request(message, timeout: .seconds(5))
                return
            } catch DaemonError.offline, DaemonError.timedOut, DaemonError.closed {}
        }
        _ = try await OneShot.request(message, credentials: host.credentials)
    }
}
