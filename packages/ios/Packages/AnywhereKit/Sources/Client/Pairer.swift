import Foundation
import Network
import Wire

/// Why a pairing code didn't pair.
public enum PairingFailure: Error, Hashable, Sendable {
    /// The code timed out or was replaced by a newer one on the Mac.
    case expired
    case used
    /// The code doesn't match the one the Mac shows.
    case invalid
    /// Too many wrong codes: the Mac stops pairing for a minute.
    case locked
    /// Too many tries from this phone's address.
    case rateLimited
    /// The Mac couldn't reach Tailscale, or this phone couldn't reach the Mac.
    case unreachable
    case updateMac
    case updatePhone
    /// Anything else the Mac said.
    case failed(String)

    init(_ error: ServerError) {
        switch error.code {
        case .pairExpired?: self = .expired
        case .pairUsed?: self = .used
        case .pairInvalid?: self = .invalid
        case .pairLocked?: self = .locked
        case .rateLimited?: self = .rateLimited
        case .serverTooOld?: self = .updateMac
        case .clientTooOld?: self = .updatePhone
        default: self = error.message == malformedMessage ? .updateMac : .failed(error.message)
        }
    }
}

/// Trades a pairing code for a device token on a socket of its own: `pair` → `pair.ok`, then pocketd closes it and the phone
/// says hello with the token on a `DaemonConnection`.
public enum Pairer {
    public static func pair(
        host: String, code: String, phoneName: String, platform: String = "ios", timeout: Duration = .seconds(10)
    ) async throws(PairingFailure) -> PairOk {
        guard let url = DaemonEndpoint.url(host: host) else { throw .unreachable }
        let attempt = Attempt(url: url, frame: ClientFrame(id: "pair", .pair(.init(code: code, name: phoneName, platform: platform))))
        let result = await withTaskCancellationHandler {
            await attempt.run(timeout: timeout)
        } onCancel: {
            attempt.cancel()
        }
        return try result.get()
    }

    /// One socket's life, finished exactly once by whichever comes first: the reply, a close, a failure or the timeout.
    private final class Attempt: @unchecked Sendable {
        private let url: URL
        private let frame: ClientFrame
        private let queue = DispatchQueue(label: "dev.mingo.anywhere.pairing")
        private var connection: NWConnection?
        private var continuation: CheckedContinuation<Result<PairOk, PairingFailure>, Never>?

        init(url: URL, frame: ClientFrame) {
            self.url = url
            self.frame = frame
        }

        func run(timeout: Duration) async -> Result<PairOk, PairingFailure> {
            await withCheckedContinuation { continuation in
                queue.async { [self] in
                    self.continuation = continuation
                    start()
                    let seconds = Double(timeout.components.seconds) + Double(timeout.components.attoseconds) / 1e18
                    queue.asyncAfter(deadline: .now() + seconds) { [self] in finish(.failure(.unreachable)) }
                }
            }
        }

        func cancel() {
            queue.async { [self] in finish(.failure(.unreachable)) }
        }

        private func start() {
            let parameters: NWParameters = url.scheme == "wss" ? .tls : .tcp
            let ws = NWProtocolWebSocket.Options()
            ws.autoReplyPing = true
            parameters.defaultProtocolStack.applicationProtocols.insert(ws, at: 0)
            let connection = NWConnection(to: .url(url), using: parameters)
            self.connection = connection
            connection.stateUpdateHandler = { [self] state in
                switch state {
                case .ready:
                    send()
                    receive()
                case .failed, .cancelled, .waiting(.posix(.ECONNREFUSED)):
                    finish(.failure(.unreachable))
                default:
                    // Other `.waiting` (no route to the tailnet yet) retries on its own until the timeout.
                    break
                }
            }
            connection.start(queue: queue)
        }

        private func send() {
            guard let data = try? frame.encoded() else { return finish(.failure(.updatePhone)) }
            let context = NWConnection.ContentContext(identifier: "pair", metadata: [NWProtocolWebSocket.Metadata(opcode: .text)])
            connection?.send(content: data, contentContext: context, isComplete: true, completion: .idempotent)
        }

        private func receive() {
            connection?.receiveMessage { [self] content, context, _, error in
                let metadata = context?.protocolMetadata(definition: NWProtocolWebSocket.definition) as? NWProtocolWebSocket.Metadata
                if metadata?.opcode == .close {
                    if case .privateCode(let code)? = metadata?.closeCode, Int(code) == CloseCode.versionMismatch {
                        let reason = content.flatMap { String(data: $0, encoding: .utf8) }
                        return finish(.failure(reason == ErrorCode.serverTooOld.rawValue ? .updateMac : .updatePhone))
                    }
                    return finish(.failure(.failed("The Mac closed the connection.")))
                }
                if error != nil { return finish(.failure(.unreachable)) }
                if metadata?.opcode == .text, let content, let reply = try? ServerFrame.decode(content), reply.id == frame.id {
                    switch reply.message {
                    case .pairOk(let ok): return finish(.success(ok))
                    case .error(let e): return finish(.failure(PairingFailure(e)))
                    default: break
                    }
                }
                receive()
            }
        }

        private func finish(_ result: Result<PairOk, PairingFailure>) {
            guard let continuation else { return }
            self.continuation = nil
            continuation.resume(returning: result)
            connection?.stateUpdateHandler = nil
            connection?.cancel()
            connection = nil
        }
    }
}
