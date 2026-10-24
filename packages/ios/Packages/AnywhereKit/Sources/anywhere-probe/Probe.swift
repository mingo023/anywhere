import Client
import Foundation
import Stores
import Wire

/// Drives one real pocketd through `Client` and `Stores` and prints a PASS, FAIL or SKIP line per check.
/// Every event is both recorded, for checks that wait on pushes and binary frames, and applied to the `HostModel`,
/// so the stores see exactly what the app's would.
@MainActor
final class Probe {
    struct Failure: Error, CustomStringConvertible {
        let description: String
        init(_ description: String) { self.description = description }
    }

    let host: HostModel
    let connection: DaemonConnection
    private(set) var events: [ConnectionEvent] = []
    private(set) var passed = 0
    private(set) var failed = 0
    private(set) var skipped = 0

    init(credentials: Credentials) {
        connection = DaemonConnection(config: .init(host: credentials.host, token: credentials.deviceToken, clientId: credentials.clientId))
        host = HostModel(credentials: credentials, connection: connection)
        let connection = connection
        Task { @MainActor [weak self] in
            for await event in connection.events {
                self?.events.append(event)
                self?.host.apply([event])
            }
        }
        Task { await connection.start() }
    }

    var hello: HelloOk? { host.hello }

    // MARK: Checks

    /// Runs one check. A check needing a cap the Mac doesn't offer is skipped, so after merge each slice's check runs
    /// as soon as its handlers land.
    func check(_ name: String, cap: String? = nil, _ body: () async throws -> String) async {
        if let cap, hello?.has(cap) != true {
            skipped += 1
            print("SKIP \(name): this pocketd doesn't offer \(cap)")
            return
        }
        let start = Date()
        do {
            let detail = try await body()
            passed += 1
            print("PASS \(name) — \(detail) (\(Int(Date().timeIntervalSince(start) * 1000)) ms)")
        } catch {
            failed += 1
            print("FAIL \(name) — \(error)")
        }
    }

    func skip(_ name: String, _ reason: String) {
        skipped += 1
        print("SKIP \(name): \(reason)")
    }

    // MARK: Helpers

    /// Sends a request and takes the reply `extract` accepts, failing on any other.
    func request<T>(_ message: ClientMessage, timeout: Duration = .seconds(10), _ extract: (ServerMessage) -> T?) async throws -> T {
        let reply = try await connection.request(message, timeout: timeout)
        guard let value = extract(reply) else { throw Failure("unexpected reply \(reply.type)") }
        return value
    }

    /// Waits until `done` holds, polling the main actor.
    func until(_ what: String, seconds: Double = 10, _ done: () -> Bool) async throws {
        let deadline = Date().addingTimeInterval(seconds)
        while !done() {
            guard Date() < deadline else { throw Failure("timed out waiting for \(what)") }
            try await Task.sleep(for: .milliseconds(20))
        }
    }

    /// Waits for an event after `mark` (an index into `events`) that `match` accepts.
    func event<T>(after mark: Int, _ what: String, seconds: Double = 10, _ match: (ConnectionEvent) -> T?) async throws -> T {
        var found: T?
        try await until(what, seconds: seconds) {
            found = events[min(mark, events.count)...].lazy.compactMap(match).first
            return found != nil
        }
        return found!
    }

    func push<T>(after mark: Int, _ what: String, seconds: Double = 10, _ match: (ServerMessage) -> T?) async throws -> T {
        try await event(after: mark, what, seconds: seconds) { if case .message(let m) = $0 { match(m) } else { nil } }
    }

    func stop() async { await connection.stop() }
}
