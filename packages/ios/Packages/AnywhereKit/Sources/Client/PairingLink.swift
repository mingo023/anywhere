import Foundation

/// `anywhere://pair?v=1&h=<host:port>&c=<code>&n=<Mac name>`, from the Mac's QR code.
public struct PairingLink: Hashable, Sendable {
    public var host: String
    public var code: String
    /// The Mac's name, or its host when the link names none.
    public var name: String

    public init(host: String, code: String, name: String) {
        self.host = host
        self.code = code
        self.name = name
    }

    static let prefix = "anywhere://pair?"

    public static func parse(_ link: String) -> PairingLink? {
        guard link.hasPrefix(prefix), let params = query(String(link.dropFirst(prefix.count))) else { return nil }
        let host = params["h"] ?? ""
        let code = params["c"] ?? ""
        guard params["v"] == "1", isCode(code), isHost(host) else { return nil }
        let name = params["n"] ?? ""
        return PairingLink(host: host, code: code, name: name.isEmpty ? host : name)
    }

    /// A pairing code is the 22 URL-safe base64 characters the Mac shows.
    public static func isCode(_ code: String) -> Bool {
        code.utf8.count == 22 && code.utf8.allSatisfy { c in
            (c >= 0x41 && c <= 0x5a) || (c >= 0x61 && c <= 0x7a) || (c >= 0x30 && c <= 0x39) || c == 0x5f || c == 0x2d
        }
    }

    /// `name:port` or `[v6]:port`, port 1 to 65535.
    public static func isHost(_ host: String) -> Bool {
        guard let colon = host.lastIndex(of: ":") else { return false }
        let name = host[..<colon]
        let port = host[host.index(after: colon)...]
        guard (1...5).contains(port.count), port.allSatisfy(\.isASCII), port.allSatisfy(\.isNumber), let n = Int(port), (1...65535).contains(n) else {
            return false
        }
        if name.hasPrefix("[") {
            let inner = name.dropFirst()
            guard inner.hasSuffix("]") else { return false }
            let v6 = inner.dropLast()
            return !v6.isEmpty && v6.allSatisfy { $0 == ":" || $0.isHexDigit && $0.isASCII }
        }
        return !name.isEmpty && name.allSatisfy { $0.isASCII && ($0.isLetter || $0.isNumber || $0 == "." || $0 == "-") }
    }

    /// Splits by hand so a bad escape refuses the whole link.
    private static func query(_ search: String) -> [String: String]? {
        var params: [String: String] = [:]
        for part in search.split(separator: "&", omittingEmptySubsequences: false) {
            guard let eq = part.firstIndex(of: "="), eq > part.startIndex else { continue }
            let raw = part[part.index(after: eq)...].replacingOccurrences(of: "+", with: " ")
            guard let value = raw.removingPercentEncoding else { return nil }
            params[String(part[..<eq])] = value
        }
        return params
    }
}
