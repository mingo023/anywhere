import Client
import Foundation

/// Every event a connection reports, for tests to wait on.
actor Recorder {
    private(set) var events: [ConnectionEvent] = []

    init(_ connection: DaemonConnection) {
        let stream = connection.events
        Task { await self.listen(stream) }
    }

    private func listen(_ stream: AsyncStream<ConnectionEvent>) async {
        for await event in stream { events.append(event) }
    }

    /// Waits until `done` holds for the events so far, or `seconds` pass.
    @discardableResult
    func wait(seconds: Double = 5, until done: @Sendable ([ConnectionEvent]) -> Bool) async -> Bool {
        let deadline = Date().addingTimeInterval(seconds)
        while Date() < deadline {
            if done(events) { return true }
            try? await Task.sleep(for: .milliseconds(10))
        }
        return done(events)
    }
}

extension ConnectionEvent {
    var isConnected: Bool {
        if case .connected = self { true } else { false }
    }

    var closed: CloseOutcome? {
        if case .closed(let outcome) = self { outcome } else { nil }
    }
}
