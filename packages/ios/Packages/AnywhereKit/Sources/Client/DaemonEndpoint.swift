import Foundation

/// Where a Mac's pocketd listens.
public enum DaemonEndpoint {
    /// Plain `ws://` where WireGuard already encrypts (loopback and the tailnet), else `wss://`.
    public static func url(host: String) -> URL? {
        guard let colon = host.lastIndex(of: ":") else { return nil }
        let name = String(host[..<colon])
        return URL(string: "\(onTailnet(name) ? "ws" : "wss")://\(host)")
    }

    public static func onTailnet(_ hostname: String) -> Bool {
        if hostname == "localhost" || hostname == "[::1]" || hostname.hasSuffix(".ts.net") { return true }
        if hostname.lowercased().hasPrefix("[fd7a:115c:a1e0:") { return true }
        guard let octets = ipv4(hostname) else { return false }
        return octets[0] == 127 || (octets[0] == 100 && (64...127).contains(octets[1]))
    }

    /// Four decimal octets with no leading zeros, so `100.064.0.1` isn't read as the tailnet.
    private static func ipv4(_ hostname: String) -> [Int]? {
        let parts = hostname.split(separator: ".", omittingEmptySubsequences: false)
        guard parts.count == 4 else { return nil }
        var octets: [Int] = []
        for part in parts {
            guard (1...3).contains(part.count), part.allSatisfy({ $0.isASCII && $0.isNumber }), part == "0" || !part.hasPrefix("0"),
                let n = Int(part), n <= 255
            else { return nil }
            octets.append(n)
        }
        return octets
    }
}
