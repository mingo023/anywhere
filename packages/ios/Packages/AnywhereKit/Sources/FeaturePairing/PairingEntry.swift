import Client
import Foundation

/// Turns what the user scanned, pasted or typed into a pairing link.
public enum PairingEntry {
    /// pocketd's port in a release build, for an address typed without one.
    public static let defaultPort = 4517

    /// A scanned or pasted `anywhere://pair` link, or the "host code" pair `pocketd pair` prints.
    public nonisolated static func link(from text: String) -> PairingLink? {
        let text = text.trimmingCharacters(in: .whitespacesAndNewlines)
        if let link = PairingLink.parse(text) { return link }
        let words = text.split(whereSeparator: \.isWhitespace).map { $0.trimmingCharacters(in: CharacterSet(charactersIn: ".,;")) }
        if let link = words.lazy.compactMap({ PairingLink.parse($0) }).first { return link }
        guard let host = words.first(where: PairingLink.isHost), let code = words.first(where: PairingLink.isCode) else { return nil }
        return PairingLink(host: host, code: code, name: host)
    }

    /// An address and a code typed apart. The address may leave out pocketd's port.
    public nonisolated static func link(host: String, code: String) -> PairingLink? {
        guard let host = address(host) else { return nil }
        let code = code.trimmingCharacters(in: .whitespacesAndNewlines)
        guard PairingLink.isCode(code) else { return nil }
        return PairingLink(host: host, code: code, name: host)
    }

    /// `name:port`, adding the default port to a bare name, IPv4 or IPv6 address.
    public nonisolated static func address(_ typed: String) -> String? {
        let host = typed.trimmingCharacters(in: .whitespacesAndNewlines)
        if PairingLink.isHost(host) { return host }
        let bare = host.contains(":") && !host.hasPrefix("[") ? "[\(host)]" : host
        let withPort = "\(bare):\(defaultPort)"
        return PairingLink.isHost(withPort) ? withPort : nil
    }
}
