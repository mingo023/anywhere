import DesignSystem
import Domain
import Foundation
import Stores
import Wire

/// What the settings say about a Mac and what this phone may do on it, as plain values.
public enum MacFacts {
    public struct Grant: Hashable, Sendable, Identifiable {
        public var id: String { scope.rawValue }
        public var scope: Scope
        public var title: String
        public var allowed: Bool
    }

    /// The four phone scopes, each marked as granted or not. A Mac older than scopes.v1 sends none: every one is assumed.
    public nonisolated static func grants(_ scopes: [Scope]?) -> [Grant] {
        let all: [(Scope, String)] = [
            (.observe, "See sessions and changes"),
            (.drive, "Send prompts and type"),
            (.approve, "Answer permissions"),
            (.spawn, "Start sessions and worktrees"),
        ]
        return all.map { scope, title in Grant(scope: scope, title: title, allowed: scopes?.contains(scope) ?? true) }
    }

    /// A Mac's line in the Macs list: "Connected · 0.1.13", or how the link reads when it isn't up.
    public nonisolated static func status(_ reachability: Reachability, version: String?) -> String {
        switch reachability {
        case .online: version.map { "Connected · \($0)" } ?? "Connected"
        case .connecting: "Connecting…"
        case .reconnecting: "Reconnecting…"
        case .unreachable: "Couldn't reconnect · Check Tailscale"
        case .stopped: "Offline · Check Tailscale"
        case .revoked: "Access removed"
        case .updateMac, .updatePhone: "Versions don't match"
        }
    }

    /// The line under a Mac's name on its own screen; `lastSeen` is when it went, in ms.
    public nonisolated static func detailStatus(_ reachability: Reachability, lastSeen: Int?, timeZone: TimeZone = .current) -> String {
        switch reachability {
        case .unreachable, .stopped: lastSeen.map { "Offline · last seen \(TimeText.clock(ms: $0, timeZone: timeZone))" } ?? "Offline"
        default: status(reachability, version: nil)
        }
    }

    /// The dot beside the status: green up, grey while dialling or stopped, red when it can't be used.
    public nonisolated static func dot(_ reachability: Reachability) -> Token {
        switch reachability {
        case .online: Palette.success
        case .connecting, .reconnecting: Palette.text3
        case .unreachable, .revoked: Palette.failed
        case .stopped, .updateMac, .updatePhone: Palette.text4
        }
    }

    /// "2 working · 1 needs you", or nil when neither is any.
    public nonisolated static func activity(working: Int, needsYou: Int) -> String? {
        let parts = [working > 0 ? "\(working) working" : nil, needsYou > 0 ? "\(needsYou) needs you" : nil].compactMap { $0 }
        return parts.isEmpty ? nil : parts.joined(separator: " · ")
    }

    /// A desktop's icon where its name says it is one; the wire doesn't say what hardware a Mac is.
    public nonisolated static func icon(forName name: String) -> Lucide {
        let desktops = ["mini", "studio", "imac", "mac pro"]
        return desktops.contains { name.localizedCaseInsensitiveContains($0) } ? .monitor : .laptop
    }

    /// Which of four layouts a Mac's own screen takes.
    public enum Standing: Hashable, Sendable {
        case online, offline, versions, removed
    }

    public nonisolated static func standing(_ reachability: Reachability) -> Standing {
        switch reachability {
        case .online: .online
        case .connecting, .reconnecting, .unreachable, .stopped: .offline
        case .updateMac, .updatePhone: .versions
        case .revoked: .removed
        }
    }

    /// Why a Mac's own screen offers a way back instead of its settings, or nil while it has them.
    public nonisolated static func endedMessage(_ reachability: Reachability, mac: String, version: String?) -> String? {
        switch reachability {
        case .revoked:
            "\(mac) removed this iPhone. Pair again to see and drive its sessions."
        case .updateMac:
            "Zoes on the Mac is \(version.map { "\($0), " } ?? "")older than this app needs to send prompts and answers. Update it on the Mac, then try again."
        case .updatePhone:
            "\(mac) runs a newer Zoes than this app speaks. Update this app from the App Store."
        default: nil
        }
    }

    /// What a Mac's alerts to this phone are doing, in a word, for its own screen.
    public nonisolated static func alerts(_ state: PushStore.State, permission: PushCenter.Permission) -> String {
        switch state {
        case .registered: "On"
        case .registering: "Registering…"
        case .waiting: permission == .denied ? "Off" : "Waiting"
        case .failed: "Failed"
        case .unsupported: "Unavailable"
        }
    }

    /// What to say under a Mac's alerts when "On" isn't the whole story.
    public nonisolated static func alertsNote(_ state: PushStore.State, permission: PushCenter.Permission) -> String? {
        switch state {
        case .unsupported: "Update Zoes on this Mac to get notifications."
        case .waiting: permission == .denied ? "Notifications are off for Zoes on this iPhone." : "Not registered yet."
        case .failed(let why): why
        case .registering, .registered: nil
        }
    }
}
