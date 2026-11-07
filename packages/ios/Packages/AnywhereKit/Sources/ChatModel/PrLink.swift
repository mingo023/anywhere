/// A GitHub pull request, as a URL names it (desktop `chat/pr_link.rs PrRef`).
public struct PrLink: Hashable, Sendable {
    public let owner: String
    public let repo: String
    public let number: Int

    /// The PR a `http(s)://github.com/<owner>/<repo>/pull/<n>` URL points at, whatever follows the number.
    public init?(_ url: String) {
        guard let rest = url.hasPrefix("https://") ? url.dropFirst(8) : url.hasPrefix("http://") ? url.dropFirst(7) : nil else { return nil }
        let host = rest.hasPrefix("www.") ? rest.dropFirst(4) : rest
        let parts = host.split(omittingEmptySubsequences: false) { $0 == "/" || $0 == "#" || $0 == "?" }
        guard parts.count >= 5, parts[0].lowercased() == "github.com", parts[3] == "pull", !parts[1].isEmpty, !parts[2].isEmpty,
            parts[4].allSatisfy(\.isASCII), parts[4].allSatisfy(\.isNumber), let number = Int(parts[4])
        else { return nil }
        owner = String(parts[1])
        repo = String(parts[2])
        self.number = number
    }

    /// GitHub names are case-insensitive.
    public func same(_ other: PrLink) -> Bool {
        number == other.number && owner.lowercased() == other.owner.lowercased() && repo.lowercased() == other.repo.lowercased()
    }
}
