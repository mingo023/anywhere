import Foundation
import Network
import Wire

/// A real WebSocket server on loopback, standing in for pocketd. `script` answers each text frame the phone sends.
final class TestServer: @unchecked Sendable {
    final class Peer: @unchecked Sendable {
        let connection: NWConnection

        init(_ connection: NWConnection) { self.connection = connection }

        func send(_ frame: ServerFrame) {
            send(try! frame.encoded(), .text)
        }

        func sendRaw(_ text: String) {
            send(Data(text.utf8), .text)
        }

        func sendBinary(_ data: Data) {
            send(data, .binary)
        }

        func close(code: UInt16, reason: String = "") {
            let metadata = NWProtocolWebSocket.Metadata(opcode: .close)
            metadata.closeCode = .privateCode(code)
            let context = NWConnection.ContentContext(identifier: "close", metadata: [metadata])
            // The socket stays up until the phone answers with its own close, as a real server's does.
            connection.send(content: Data(reason.utf8), contentContext: context, isComplete: true, completion: .idempotent)
        }

        /// Drops the socket without a close frame, as a sleeping Mac would.
        func cut() { connection.cancel() }

        private func send(_ data: Data, _ opcode: NWProtocolWebSocket.Opcode) {
            let metadata = NWProtocolWebSocket.Metadata(opcode: opcode)
            let context = NWConnection.ContentContext(identifier: "send", metadata: [metadata])
            connection.send(content: data, contentContext: context, isComplete: true, completion: .idempotent)
        }
    }

    let queue = DispatchQueue(label: "test.server")
    private let listener: NWListener
    private let script: @Sendable (Peer, ClientFrame) -> Void
    private let lock = NSLock()
    private var _received: [ClientFrame] = []
    private var _accepted = 0

    var received: [ClientFrame] { lock.withLock { _received } }
    var accepted: Int { lock.withLock { _accepted } }
    var host: String { "127.0.0.1:\(listener.port!.rawValue)" }

    init(script: @escaping @Sendable (Peer, ClientFrame) -> Void) async throws {
        let parameters = NWParameters.tcp
        let ws = NWProtocolWebSocket.Options()
        ws.autoReplyPing = true
        parameters.defaultProtocolStack.applicationProtocols.insert(ws, at: 0)
        listener = try NWListener(using: parameters, on: .any)
        self.script = script
        listener.newConnectionHandler = { [weak self] connection in self?.accept(connection) }
        await withCheckedContinuation { (ready: CheckedContinuation<Void, Never>) in
            let once = Once()
            listener.stateUpdateHandler = { state in
                if case .ready = state, once.first() { ready.resume() }
            }
            listener.start(queue: queue)
        }
    }

    func stop() { listener.cancel() }

    private func accept(_ connection: NWConnection) {
        lock.withLock { _accepted += 1 }
        let peer = Peer(connection)
        connection.start(queue: queue)
        receive(peer)
    }

    private func receive(_ peer: Peer) {
        peer.connection.receiveMessage { [weak self] content, context, _, error in
            guard let self, error == nil else { return }
            let metadata = context?.protocolMetadata(definition: NWProtocolWebSocket.definition) as? NWProtocolWebSocket.Metadata
            if metadata?.opcode == .text, let content, let frame = try? ClientFrame.decode(content) {
                lock.withLock { _received.append(frame) }
                script(peer, frame)
            }
            if metadata?.opcode == .close {
                peer.connection.cancel()
            } else {
                receive(peer)
            }
        }
    }
}

final class Once: @unchecked Sendable {
    private let lock = NSLock()
    private var done = false

    func first() -> Bool {
        lock.withLock {
            defer { done = true }
            return !done
        }
    }
}

let helloOk = HelloOk(serverId: "mac-1", hostname: "Mingo's Mac", protocolVersion: 3, caps: Caps.phone, protocol: .phone)
