import Client

extension PairingFailure {
    /// What the phone says when pairing fails, and what to do next.
    public struct Copy: Hashable, Sendable {
        public var title: String
        public var message: String
        /// Pairing works only over Tailscale, so an unreachable Mac offers to open it.
        public var suggestsTailscale: Bool
    }

    public nonisolated func copy(mac: String) -> Copy {
        switch self {
        case .expired:
            Copy(title: "This code expired", message: "Codes last 5 minutes. Make a new one on your Mac and try again.", suggestsTailscale: false)
        case .used:
            Copy(title: "This code was already used", message: "Make a new one on your Mac and try again.", suggestsTailscale: false)
        case .invalid:
            Copy(title: "This code doesn't match", message: "Check it against the code your Mac shows, or scan the QR code instead.", suggestsTailscale: false)
        case .locked:
            Copy(title: "Too many wrong codes", message: "\(mac) stopped pairing for a minute. Make a new code then.", suggestsTailscale: false)
        case .rateLimited:
            Copy(title: "Too many tries", message: "Wait a minute, then try again.", suggestsTailscale: false)
        case .unreachable:
            Copy(
                title: "Can't reach \(mac)",
                message: "Your iPhone reaches your Mac over Tailscale. Check that Tailscale is connected on both, and that the Mac is awake.",
                suggestsTailscale: true)
        case .updateMac:
            Copy(title: "Update Anywhere on \(mac)", message: "It's older than this app. Update it on the Mac, then pair again.", suggestsTailscale: false)
        case .updatePhone:
            Copy(title: "Update this app", message: "\(mac) runs a newer Anywhere than this app speaks. Update it from the App Store.", suggestsTailscale: false)
        case .failed(let message):
            Copy(title: "Pairing failed", message: message, suggestsTailscale: false)
        }
    }
}
