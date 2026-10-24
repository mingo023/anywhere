import Foundation

/// pocketd's owner channel: NDJSON `ops.Msg` over its unix socket. The probe uses it as the desktop would, to open a
/// pairing code and to start a terminal; phones never reach it.
final class OwnerSocket {
    struct Message: Codable {
        var op: String?
        var ev: String?
        var id: String?
        var cmd: String?
        var cwd: String?
        var env: [String]?
        var cols: Int?
        var rows: Int?
        var text: String?
        var error: String?
        var errorCode: String?
        var pair: Offer?
    }

    struct Offer: Codable {
        var url: String
        var code: String
        var expiresAt: Int
    }

    struct Failure: Error, CustomStringConvertible {
        let description: String
    }

    private let fd: Int32
    private var buffer = Data()

    init(path: String) throws {
        fd = socket(AF_UNIX, SOCK_STREAM, 0)
        guard fd >= 0 else { throw Failure(description: "socket: \(errno)") }
        var addr = sockaddr_un()
        addr.sun_family = sa_family_t(AF_UNIX)
        let bytes = Array(path.utf8)
        guard bytes.count < MemoryLayout.size(ofValue: addr.sun_path) else { throw Failure(description: "socket path too long: \(path)") }
        withUnsafeMutableBytes(of: &addr.sun_path) { raw in
            raw.copyBytes(from: bytes)
            raw[bytes.count] = 0
        }
        let ok = withUnsafePointer(to: &addr) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) { connect(fd, $0, socklen_t(MemoryLayout<sockaddr_un>.size)) }
        }
        guard ok == 0 else {
            close(fd)
            throw Failure(description: "connect \(path): errno \(errno)")
        }
    }

    deinit { close(fd) }

    func send(_ message: Message) throws {
        var data = try JSONEncoder().encode(message)
        data.append(0x0a)
        let written = data.withUnsafeBytes { write(fd, $0.baseAddress, data.count) }
        guard written == data.count else { throw Failure(description: "write: errno \(errno)") }
    }

    /// The next line pocketd sends; blocks.
    func receive() throws -> Message {
        while true {
            if let newline = buffer.firstIndex(of: 0x0a) {
                let line = buffer[buffer.startIndex..<newline]
                buffer = Data(buffer[buffer.index(after: newline)...])
                return try JSONDecoder().decode(Message.self, from: line)
            }
            var chunk = [UInt8](repeating: 0, count: 4096)
            let n = read(fd, &chunk, chunk.count)
            guard n > 0 else { throw Failure(description: "pocketd closed the owner socket") }
            buffer.append(contentsOf: chunk[0..<n])
        }
    }

    /// Opens a pairing code for `host`. The code lives while this socket stays open.
    func beginPairing(host: String) throws -> Offer {
        try send(Message(op: "pair.begin", text: host))
        let reply = try receive()
        guard reply.ev == "pair.begin", let offer = reply.pair else {
            throw Failure(description: "pair.begin: \(reply.error ?? reply.ev ?? "no reply")")
        }
        return offer
    }

    /// Removes a paired phone, as the desktop's Settings → Phone → Revoke does.
    func revoke(deviceId: String) throws {
        try send(Message(op: "devices.revoke", id: deviceId))
        var reply = try receive()
        while reply.ev == "pair.ok" || reply.ev == "pair.expired" { reply = try receive() }
        guard reply.ev == "ok" else { throw Failure(description: "devices.revoke: \(reply.error ?? reply.ev ?? "no reply")") }
    }

    /// Starts `cmd` in a new terminal, as `pocketd run` does, and returns the terminal id.
    func spawn(_ cmd: String, cwd: String, env: [String]) throws -> String {
        try send(Message(op: "spawn", cmd: cmd, cwd: cwd, env: env, cols: 100, rows: 30))
        var reply = try receive()
        // The pairing this socket opened reports its end here too.
        while reply.ev == "pair.ok" || reply.ev == "pair.expired" { reply = try receive() }
        guard reply.ev == "spawned", let id = reply.id else { throw Failure(description: "spawn: \(reply.error ?? reply.ev ?? "no reply")") }
        return id
    }
}
