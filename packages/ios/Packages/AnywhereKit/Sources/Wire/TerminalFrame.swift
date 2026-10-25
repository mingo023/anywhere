import Foundation

/// A binary WebSocket frame of terminal bytes, server to phone only:
/// byte 0 kind, byte 1 the id's length n, then n ASCII bytes of terminal id, then the payload.
public struct TerminalFrame: Hashable, Sendable {
    public enum Kind: UInt8, Sendable {
        /// Replace the screen: reset, then feed.
        case snapshot = 1
        case output = 2
    }

    public enum ParseError: Error, Equatable {
        case tooShort
        case unknownKind(UInt8)
        case badId
    }

    public var kind: Kind
    public var terminalId: String
    /// Fed to the terminal emulator as-is.
    public var payload: Data

    public init(kind: Kind, terminalId: String, payload: Data) {
        self.kind = kind
        self.terminalId = terminalId
        self.payload = payload
    }

    public init(parsing data: Data) throws(ParseError) {
        let bytes = data.startIndex
        guard data.count >= 2 else { throw .tooShort }
        guard let kind = Kind(rawValue: data[bytes]) else { throw .unknownKind(data[bytes]) }
        let n = Int(data[bytes + 1])
        guard n > 0 else { throw .badId }
        guard data.count >= 2 + n else { throw .tooShort }
        let idBytes = data[(bytes + 2)..<(bytes + 2 + n)]
        guard idBytes.allSatisfy({ $0 >= 0x20 && $0 < 0x7f }), let id = String(bytes: idBytes, encoding: .ascii) else { throw .badId }
        self.kind = kind
        terminalId = id
        payload = Data(data[(bytes + 2 + n)...])
    }

    /// The frame's bytes, as pocketd writes them.
    public var encoded: Data {
        var out = Data([kind.rawValue, UInt8(terminalId.utf8.count)])
        out.append(contentsOf: terminalId.utf8)
        out.append(payload)
        return out
    }
}
