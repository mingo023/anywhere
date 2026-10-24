import Foundation

/// A string union that decodes any value, so a word a newer pocketd adds never fails a frame.
public protocol OpenString: RawRepresentable, Codable, Hashable, Sendable, ExpressibleByStringLiteral, CustomStringConvertible
where RawValue == String {
    init(rawValue: String)
}

extension OpenString {
    public init(stringLiteral value: String) { self.init(rawValue: value) }
    public init(from decoder: any Decoder) throws { self.init(rawValue: try decoder.singleValueContainer().decode(String.self)) }
    public func encode(to encoder: any Encoder) throws {
        var c = encoder.singleValueContainer()
        try c.encode(rawValue)
    }
    public var description: String { rawValue }
}

/// Any JSON value, for fields the protocol leaves open (`ToolDetail.other.input`).
public enum JSONValue: Codable, Hashable, Sendable {
    case null
    case bool(Bool)
    case number(Double)
    case string(String)
    case array([JSONValue])
    case object([String: JSONValue])

    public init(from decoder: any Decoder) throws {
        let c = try decoder.singleValueContainer()
        if c.decodeNil() {
            self = .null
        } else if let b = try? c.decode(Bool.self) {
            self = .bool(b)
        } else if let n = try? c.decode(Double.self) {
            self = .number(n)
        } else if let s = try? c.decode(String.self) {
            self = .string(s)
        } else if let a = try? c.decode([JSONValue].self) {
            self = .array(a)
        } else {
            self = .object(try c.decode([String: JSONValue].self))
        }
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.singleValueContainer()
        switch self {
        case .null: try c.encodeNil()
        case .bool(let b): try c.encode(b)
        case .number(let n):
            if n.rounded() == n, abs(n) < 1e15 { try c.encode(Int64(n)) } else { try c.encode(n) }
        case .string(let s): try c.encode(s)
        case .array(let a): try c.encode(a)
        case .object(let o): try c.encode(o)
        }
    }

    public subscript(key: String) -> JSONValue? {
        if case .object(let o) = self { o[key] } else { nil }
    }

    public var string: String? {
        if case .string(let s) = self { s } else { nil }
    }
}

/// The coders every frame goes through.
public enum WireCoding {
    public static func decoder() -> JSONDecoder { JSONDecoder() }

    public static func encoder() -> JSONEncoder {
        let e = JSONEncoder()
        e.outputFormatting = [.withoutEscapingSlashes]
        return e
    }
}

/// The key every message switches on.
enum TypeKey: String, CodingKey {
    case type
}

/// The correlation id of requests and their replies.
enum IdKey: String, CodingKey {
    case id
}

/// An error a decoder throws for a value a known message can't hold.
func malformed(_ decoder: any Decoder, _ what: String) -> DecodingError {
    .dataCorrupted(.init(codingPath: decoder.codingPath, debugDescription: what))
}
