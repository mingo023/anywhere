import Foundation
import Testing
import Wire
@testable import Client

private func connect(_ server: TestServer, backoff: Backoff = Backoff(first: 0.05, cap: 0.2)) async -> (DaemonConnection, Recorder) {
    let connection = DaemonConnection(config: .init(host: server.host, token: "tok", clientId: "ios-test", backoff: backoff))
    let recorder = Recorder(connection)
    await connection.start()
    return (connection, recorder)
}

/// Answers hello with hello.ok, and every other request with an ack.
private func helloThenAck(_ peer: TestServer.Peer, _ frame: ClientFrame) {
    if case .hello = frame.message {
        peer.send(ServerFrame(id: frame.id, .helloOk(helloOk)))
    } else {
        peer.send(ServerFrame(id: frame.id, .ack))
    }
}

@Suite("Daemon connection", .serialized)
struct DaemonConnectionTests {
    @Test("the phone says hello with its token, client id, caps and protocol range, and hears hello.ok")
    func hello() async throws {
        let server = try await TestServer(script: helloThenAck)
        defer { server.stop() }
        let (connection, recorder) = await connect(server)
        defer { Task { await connection.stop() } }

        #expect(await recorder.wait { $0.contains(where: \.isConnected) })
        #expect(await recorder.events.first == .connected(helloOk))
        #expect(await connection.isOnline)
        let hello = try #require(server.received.first)
        guard case .hello(let h) = hello.message else { Issue.record("first frame isn't hello"); return }
        #expect(h.token == "tok")
        #expect(h.clientId == "ios-test")
        #expect(h.caps == Caps.phone)
        #expect(h.protocol == .phone)
    }

    @Test("each reply resolves the request with its id, even out of order")
    func correlation() async throws {
        let held = HeldReplies()
        let server = try await TestServer { peer, frame in
            switch frame.message {
            case .hello: peer.send(ServerFrame(id: frame.id, .helloOk(helloOk)))
            case .agentCommands(let ref):
                held.add(peer, ServerFrame(id: frame.id, .agentCommands(.init(agentId: ref.agentId, commands: [.init(name: ref.agentId)]))))
                if held.count == 2 { held.flushReversed() }
            default: peer.send(ServerFrame(id: frame.id, .ack))
            }
        }
        defer { server.stop() }
        let (connection, recorder) = await connect(server)
        defer { Task { await connection.stop() } }
        #expect(await recorder.wait { $0.contains(where: \.isConnected) })

        async let a = connection.request(.agentCommands(.init(agentId: "a")))
        async let b = connection.request(.agentCommands(.init(agentId: "b")))
        let (ra, rb) = try await (a, b)
        #expect(ra == .agentCommands(.init(agentId: "a", commands: [.init(name: "a")])))
        #expect(rb == .agentCommands(.init(agentId: "b", commands: [.init(name: "b")])))
        #expect(try await connection.request(.agentPrompt(.init(agentId: "a", text: "hi"))) == .ack)
        #expect(await recorder.events.count == 1, "replies to requests aren't reported as events")
    }

    @Test("an error reply throws the server's error")
    func errorReply() async throws {
        let server = try await TestServer { peer, frame in
            if case .hello = frame.message {
                peer.send(ServerFrame(id: frame.id, .helloOk(helloOk)))
            } else {
                peer.send(ServerFrame(id: frame.id, .error(.init(message: "Unknown agent: zz", code: "unknown_agent"))))
            }
        }
        defer { server.stop() }
        let (connection, recorder) = await connect(server)
        defer { Task { await connection.stop() } }
        #expect(await recorder.wait { $0.contains(where: \.isConnected) })
        await #expect(throws: DaemonError.server(.init(message: "Unknown agent: zz", code: "unknown_agent"))) {
            try await connection.request(.agentInterrupt(.init(agentId: "zz")))
        }
    }

    @Test("launch progress arrives as events while the request waits for agent.created")
    func interimReplies() async throws {
        let server = try await TestServer { peer, frame in
            switch frame.message {
            case .hello: peer.send(ServerFrame(id: frame.id, .helloOk(helloOk)))
            case .agentCreate(let create):
                peer.send(ServerFrame(id: frame.id, .agentProgress(.init(requestId: create.requestId, step: "fetch"))))
                peer.send(ServerFrame(id: frame.id, .agentCreated(.init(requestId: create.requestId, agentId: "a9", terminalId: "t9"))))
            default: peer.send(ServerFrame(id: frame.id, .ack))
            }
        }
        defer { server.stop() }
        let (connection, recorder) = await connect(server)
        defer { Task { await connection.stop() } }
        #expect(await recorder.wait { $0.contains(where: \.isConnected) })
        let spec = LaunchSpec(project: "/w", checkout: .worktree("/w"), provider: .claude, access: .ask, plan: false)
        let reply = try await connection.request(.agentCreate(.init(requestId: "r1", spec: spec)))
        #expect(reply == .agentCreated(.init(requestId: "r1", agentId: "a9", terminalId: "t9")))
        #expect(await recorder.wait { $0.contains(.message(.agentProgress(.init(requestId: "r1", step: "fetch")))) })
    }

    @Test("pushes and binary terminal frames arrive as events in the order sent")
    func pushesAndFrames() async throws {
        let frame = TerminalFrame(kind: .snapshot, terminalId: "t1", payload: Data("\u{1b}[2J$ ".utf8))
        let server = try await TestServer { peer, f in
            guard case .hello = f.message else { return }
            peer.send(ServerFrame(id: f.id, .helloOk(helloOk)))
            peer.send(ServerFrame(.agentList(.init(agents: []))))
            peer.sendBinary(frame.encoded)
            peer.sendRaw(#"{"type":"agent.sparkle"}"#)
            peer.sendBinary(Data([7, 1, 0x61]))
            peer.send(ServerFrame(.namingFailed(.init(agentId: "a1"))))
        }
        defer { server.stop() }
        let (connection, recorder) = await connect(server)
        defer { Task { await connection.stop() } }
        #expect(await recorder.wait { $0.count >= 5 })
        #expect(await recorder.events == [
            .connected(helloOk), .message(.agentList(.init(agents: []))), .terminal(frame), .message(.unknown("agent.sparkle")),
            .message(.namingFailed(.init(agentId: "a1"))),
        ])
    }

    @Test("a 4401 close means the Mac revoked this phone, and the connection stops redialing")
    func revoked() async throws {
        let server = try await TestServer { peer, frame in
            guard case .hello = frame.message else { return }
            peer.send(ServerFrame(id: frame.id, .error(.init(message: "Not paired", code: .notPaired))))
            peer.close(code: 4401, reason: "not_paired")
        }
        defer { server.stop() }
        let (connection, recorder) = await connect(server)
        #expect(await recorder.wait { $0.contains { $0.closed != nil } })
        #expect(await recorder.events.compactMap(\.closed) == [.revoked])
        try await Task.sleep(for: .milliseconds(300))
        #expect(server.accepted == 1)
        #expect(await !connection.isOnline)
    }

    @Test("a bare 4401 close after hello.ok also means revoked")
    func revokedLater() async throws {
        let server = try await TestServer { peer, frame in
            guard case .hello = frame.message else { return }
            peer.send(ServerFrame(id: frame.id, .helloOk(helloOk)))
            peer.close(code: 4401, reason: "revoked")
        }
        defer { server.stop() }
        let (connection, recorder) = await connect(server)
        #expect(await recorder.wait { $0.contains { $0.closed != nil } })
        let seen = await recorder.events
        #expect(seen.compactMap(\.closed) == [.revoked], "\(seen)")
        #expect(await !connection.isOnline)
    }

    @Test("a 4426 close names which side to update")
    func versionMismatch() async throws {
        for (reason, outcome) in [("server_too_old", CloseOutcome.updateMac), ("client_too_old", .updatePhone)] {
            let server = try await TestServer { peer, frame in
                peer.send(ServerFrame(id: frame.id, .error(.init(message: "Update", code: ErrorCode(rawValue: reason)))))
                peer.close(code: 4426, reason: reason)
            }
            let (connection, recorder) = await connect(server)
            #expect(await recorder.wait { $0.contains { $0.closed != nil } })
            #expect(await recorder.events.compactMap(\.closed) == [outcome])
            #expect(await !connection.isOnline)
            server.stop()
        }
    }

    @Test("an old Mac that can't read the hello is a Mac to update")
    func oldMac() async throws {
        let server = try await TestServer { peer, frame in
            peer.send(ServerFrame(id: frame.id, .error(.init(message: "Malformed message"))))
        }
        defer { server.stop() }
        let (connection, recorder) = await connect(server)
        #expect(await recorder.wait { $0.contains { $0.closed != nil } })
        #expect(await recorder.events.compactMap(\.closed) == [.updateMac])
        #expect(await !connection.isOnline)
    }

    @Test("a dropped socket fails its pending requests, then the connection redials and says hello again")
    func redials() async throws {
        let server = try await TestServer { peer, frame in
            switch frame.message {
            case .hello: peer.send(ServerFrame(id: frame.id, .helloOk(helloOk)))
            case .agentInterrupt: peer.cut()
            default: peer.send(ServerFrame(id: frame.id, .ack))
            }
        }
        defer { server.stop() }
        let (connection, recorder) = await connect(server)
        defer { Task { await connection.stop() } }
        #expect(await recorder.wait { $0.contains(where: \.isConnected) })
        await #expect(throws: DaemonError.closed) { try await connection.request(.agentInterrupt(.init(agentId: "a"))) }
        #expect(await recorder.wait { $0.filter(\.isConnected).count == 2 })
        let closes = await recorder.events.compactMap(\.closed)
        guard case .retrying(let seconds) = closes.first else { Issue.record("no retry: \(closes)"); return }
        #expect(seconds >= 0.025 && seconds <= 0.05)
        #expect(server.accepted == 2)
        #expect(try await connection.request(.agentList) == .ack)
    }

    @Test("a request with no reply times out, and its late reply is dropped")
    func timeout() async throws {
        let server = try await TestServer { peer, frame in
            switch frame.message {
            case .hello: peer.send(ServerFrame(id: frame.id, .helloOk(helloOk)))
            case .agentCompact:
                DispatchQueue.global().asyncAfter(deadline: .now() + 0.3) { peer.send(ServerFrame(id: frame.id, .ack)) }
            default: peer.send(ServerFrame(id: frame.id, .ack))
            }
        }
        defer { server.stop() }
        let (connection, recorder) = await connect(server)
        defer { Task { await connection.stop() } }
        #expect(await recorder.wait { $0.contains(where: \.isConnected) })
        await #expect(throws: DaemonError.timedOut) {
            try await connection.request(.agentCompact(.init(agentId: "a")), timeout: .milliseconds(100))
        }
        try await Task.sleep(for: .milliseconds(400))
        #expect(await recorder.events == [.connected(helloOk)])
    }

    @Test("requests before hello.ok are refused as offline")
    func offline() async throws {
        let connection = DaemonConnection(config: .init(host: "127.0.0.1:9", token: nil, clientId: "c"))
        await #expect(throws: DaemonError.offline) { try await connection.request(.agentList) }
        await #expect(throws: DaemonError.offline) { try await connection.send(.agentList) }
    }

    @Test("stopping ends the event stream")
    func stops() async throws {
        let server = try await TestServer(script: helloThenAck)
        defer { server.stop() }
        let (connection, recorder) = await connect(server)
        #expect(await recorder.wait { $0.contains(where: \.isConnected) })
        await connection.stop()
        #expect(await recorder.wait { $0.last == .closed(.stopped) })
        await #expect(throws: DaemonError.offline) { try await connection.request(.agentList) }
    }
}

/// Replies a test holds back to send in another order.
final class HeldReplies: @unchecked Sendable {
    private let lock = NSLock()
    private var replies: [(TestServer.Peer, ServerFrame)] = []

    var count: Int { lock.withLock { replies.count } }

    func add(_ peer: TestServer.Peer, _ frame: ServerFrame) {
        lock.withLock { replies.append((peer, frame)) }
    }

    func flushReversed() {
        for (peer, frame) in lock.withLock({ replies.reversed() }) { peer.send(frame) }
    }
}
