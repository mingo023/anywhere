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
            Copy(title: "Couldn't pair", message: "The code expired. Make a new one on your Mac and try again.", suggestsTailscale: false)
        case .used:
            Copy(title: "Couldn't pair", message: "The code was already used. Make a new one on your Mac and try again.", suggestsTailscale: false)
        case .invalid:
            Copy(title: "Couldn't pair", message: "The code doesn't match. Check it against the one your Mac shows, or scan the QR code instead.", suggestsTailscale: false)
        case .locked:
            Copy(title: "Couldn't pair", message: "Too many wrong codes. \(mac) stopped pairing for a minute. Make a new code then.", suggestsTailscale: false)
        case .rateLimited:
            Copy(title: "Couldn't pair", message: "Too many tries. Wait a minute, then try again.", suggestsTailscale: false)
        case .unreachable:
            Copy(
                title: "Can't reach \(mac)",
                message: "Zoes reaches your Mac over Tailscale. Check that Tailscale is connected on both, and that the Mac is awake. If it isn't installed here yet, install it and sign in with the same account as your Mac.",
                suggestsTailscale: true)
        case .updateMac:
            Copy(title: "Update Zoes on \(mac)", message: "It's older than this app. Update it on the Mac, then pair again.", suggestsTailscale: false)
        case .updatePhone:
            Copy(title: "Update this app", message: "\(mac) runs a newer Zoes than this app speaks. Update it from the App Store.", suggestsTailscale: false)
        case .failed(let message):
            Copy(title: "Couldn't pair", message: message, suggestsTailscale: false)
        }
    }
}
