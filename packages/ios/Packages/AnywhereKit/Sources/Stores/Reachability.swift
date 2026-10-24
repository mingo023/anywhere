import Client
import Foundation

/// How a Mac's link reads to the user, as the RN app's banners did: quiet for 4 s, then "Reconnecting…", then after 30 s
/// "unreachable".
public enum Reachability: Hashable, Sendable {
    case online
    /// Offline for under 4 s: a blip shows nothing.
    case connecting
    case reconnecting
    case unreachable
    case revoked
    case updateMac
    case updatePhone
    case stopped

    public static let reconnectingAfter: TimeInterval = 4
    public static let unreachableAfter: TimeInterval = 30

    public init(link: LinkState, offlineFor: TimeInterval) {
        switch link {
        case .online: self = .online
        case .connecting, .retrying:
            self = offlineFor < Self.reconnectingAfter ? .connecting : offlineFor < Self.unreachableAfter ? .reconnecting : .unreachable
        case .ended(.revoked): self = .revoked
        case .ended(.updateMac): self = .updateMac
        case .ended(.updatePhone): self = .updatePhone
        case .ended: self = .stopped
        }
    }

    /// Ended for good: only pairing again, an update or a restart brings the Mac back.
    public var isFinal: Bool {
        switch self {
        case .revoked, .updateMac, .updatePhone, .stopped: true
        case .online, .connecting, .reconnecting, .unreachable: false
        }
    }

    /// For the Macs list.
    public var label: String {
        switch self {
        case .online: "Connected"
        case .connecting: "Connecting…"
        case .reconnecting: "Reconnecting…"
        case .unreachable: "Can't reach this Mac"
        case .revoked: "Removed from this Mac"
        case .updateMac: "Update the Mac"
        case .updatePhone: "Update this app"
        case .stopped: "Disconnected"
        }
    }

    /// How long until the reading changes on its own, so a view redraws then and not every second.
    public static func nextChange(offlineFor: TimeInterval) -> TimeInterval? {
        if offlineFor < reconnectingAfter { return reconnectingAfter - offlineFor }
        if offlineFor < unreachableAfter { return unreachableAfter - offlineFor }
        return nil
    }
}

extension HostModel {
    public func reachability(at now: Date) -> Reachability {
        Reachability(link: link, offlineFor: offlineSince.map { now.timeIntervalSince($0) } ?? 0)
    }

    /// The id routes and deep links name this Mac by: its serverId once known, else the device id pairing gave.
    public var mac: String { hello?.serverId ?? credentials.serverId ?? id }
}
