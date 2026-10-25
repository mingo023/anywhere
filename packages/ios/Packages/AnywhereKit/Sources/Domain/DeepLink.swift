import Foundation

/// What an `anywhere://` link opens: from the scheme, a notification tap, or the Live Activity.
public enum DeepLink: Hashable, Sendable {
    /// `anywhere://pair?…`; the pairing screen reads the link itself.
    case pair(URL)
    /// `anywhere://mac/<serverId>`.
    case mac(String)
    /// `anywhere://mac/<serverId>/agent/<agentId>`.
    case agent(mac: String, agentId: String)

    public static func parse(_ url: URL) -> DeepLink? {
        guard url.scheme?.lowercased() == "anywhere" else { return nil }
        let host = url.host(percentEncoded: false)
        if host == "pair" { return .pair(url) }
        guard host == "mac" else { return nil }
        let parts = url.path(percentEncoded: true).split(separator: "/").map { $0.removingPercentEncoding ?? String($0) }
        switch parts.count {
        case 1: return .mac(parts[0])
        case 3 where parts[1] == "agent": return .agent(mac: parts[0], agentId: parts[2])
        default: return nil
        }
    }

    public static func parse(_ string: String) -> DeepLink? {
        URL(string: string).flatMap(parse)
    }

    /// The stack it opens.
    public var routes: [Route] {
        switch self {
        case .pair: [.pairing]
        case .mac(let mac): [.sessions(mac: mac)]
        case .agent(let mac, let agentId): [.sessions(mac: mac), .chat(mac: mac, agentId: agentId)]
        }
    }

    public var url: URL {
        switch self {
        case .pair(let url): url
        case .mac(let mac): URL(string: "anywhere://mac/\(Self.escape(mac))")!
        case .agent(let mac, let agentId): URL(string: "anywhere://mac/\(Self.escape(mac))/agent/\(Self.escape(agentId))")!
        }
    }

    private static func escape(_ part: String) -> String {
        part.addingPercentEncoding(withAllowedCharacters: .urlPathAllowed.subtracting(CharacterSet(charactersIn: "/"))) ?? part
    }
}
