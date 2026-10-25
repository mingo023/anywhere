import Foundation

/// One text frame from pocketd: a message and, for a reply, the id of the request it answers.
public struct ServerFrame: Codable, Hashable, Sendable {
    public var id: String?
    public var message: ServerMessage

    public init(id: String? = nil, _ message: ServerMessage) {
        self.id = id
        self.message = message
    }

    public init(from decoder: any Decoder) throws {
        id = try decoder.container(keyedBy: IdKey.self).decodeIfPresent(String.self, forKey: .id)
        message = try ServerMessage(from: decoder)
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: IdKey.self)
        try c.encodeIfPresent(id, forKey: .id)
        try message.encode(to: encoder)
    }

    public static func decode(_ data: Data) throws -> ServerFrame {
        try WireCoding.decoder().decode(ServerFrame.self, from: data)
    }

    public func encoded() throws -> Data { try WireCoding.encoder().encode(self) }
}

/// One text frame to pocketd. Every request has an id; its reply echoes it.
public struct ClientFrame: Codable, Hashable, Sendable {
    public var id: String
    public var message: ClientMessage

    public init(id: String, _ message: ClientMessage) {
        self.id = id
        self.message = message
    }

    public init(from decoder: any Decoder) throws {
        id = try decoder.container(keyedBy: IdKey.self).decode(String.self, forKey: .id)
        message = try ClientMessage(from: decoder)
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: IdKey.self)
        try c.encode(id, forKey: .id)
        try message.encode(to: encoder)
    }

    public static func decode(_ data: Data) throws -> ClientFrame {
        try WireCoding.decoder().decode(ClientFrame.self, from: data)
    }

    public func encoded() throws -> Data { try WireCoding.encoder().encode(self) }
}

extension ServerMessage {
    /// A reply that more replies to the same request follow: launch and worktree progress.
    public var isInterim: Bool {
        switch self {
        case .agentCreating, .agentProgress, .worktreeProgress: true
        default: false
        }
    }
}
