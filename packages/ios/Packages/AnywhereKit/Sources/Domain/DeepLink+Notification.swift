import Foundation

extension DeepLink {
    /// The link a tapped notification opens: its `link` field, else the `mac` and `agent` the Notice names.
    public static func notification(_ userInfo: [AnyHashable: Any]) -> DeepLink? {
        if let link = userInfo["link"] as? String, let parsed = parse(link) { return parsed }
        guard let mac = userInfo["mac"] as? String, !mac.isEmpty else { return nil }
        guard let agent = userInfo["agent"] as? String, !agent.isEmpty else { return .mac(mac) }
        return .agent(mac: mac, agentId: agent)
    }
}
