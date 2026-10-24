import ActivityModel
import Client
import CryptoKit
import Foundation
import Observation
import PushSupport
import Wire

#if os(iOS)
    import UserNotifications
#endif

extension PushEnv {
    /// Xcode builds get development tokens; TestFlight and the App Store, production.
    public static var current: PushEnv {
        #if DEBUG
            .development
        #else
            .production
        #endif
    }
}

/// This phone's push setup, shared by every Mac: permission, APNs tokens, what to push, and the relay handles sealed for them.
@MainActor
@Observable
public final class PushCenter {
    public static let shared = PushCenter(
        defaults: UserDefaults(suiteName: Keychain.appGroup) ?? .standard,
        keys: Keychain(service: NoticeBox.keychainService, accessGroup: Keychain.appGroup))

    public enum Permission: Hashable, Sendable {
        case undetermined, denied, granted
    }

    public private(set) var permission = Permission.undetermined
    public private(set) var alertToken: String?
    public private(set) var startToken: String?
    /// The system lets this app's Live Activities update often; pocketd coalesces harder without it.
    public private(set) var frequent: Bool?
    public private(set) var prefs: PushPrefs
    public private(set) var liveActivities: Bool
    /// The session on screen, whose alerts stay quiet while the app is in front; opening it clears its delivered alerts.
    public var visibleAgent: String? {
        didSet {
            #if os(iOS)
                guard let agent = visibleAgent, agent != oldValue else { return }
                Task { UNUserNotificationCenter.current().removeDeliveredNotifications(withIdentifiers: await Self.delivered { $0["agent"] as? String == agent }) }
            #endif
        }
    }

    #if os(iOS)
        nonisolated static func delivered(_ matches: @Sendable ([AnyHashable: Any]) -> Bool) async -> [String] {
            await UNUserNotificationCenter.current().deliveredNotifications()
                .filter { matches($0.request.content.userInfo) }
                .map(\.request.identifier)
        }
    #endif

    /// Clears the Mac's delivered alerts for sessions that no longer want a look, since a Mac can't take a banner back.
    func clearDelivered(mac: String, keep: Set<String>) {
        #if os(iOS)
            Task {
                let ids = await Self.delivered { Self.outdated(mac: $0["mac"] as? String, agent: $0["agent"] as? String, kind: $0["kind"] as? String, on: mac, keep: keep) }
                if !ids.isEmpty { UNUserNotificationCenter.current().removeDeliveredNotifications(withIdentifiers: ids) }
            }
        #endif
    }

    /// A session's alert from `on` is outdated once the session is no longer among `keep`; automation and test alerts stay.
    nonisolated static func outdated(mac: String?, agent: String?, kind: String?, on: String, keep: Set<String>) -> Bool {
        guard mac == on, let agent, !agent.isEmpty, kind != "automation", kind != "test" else { return false }
        return !keep.contains(agent)
    }

    /// Starts a Mac's Live Activity once it turns busy; `LiveActivities.install` sets it on iOS.
    @ObservationIgnored public var startActivity: (@MainActor (MacActivity, MacActivity.ContentState) -> Void)?
    @ObservationIgnored public var endActivities: (@MainActor () -> Void)?
    @ObservationIgnored public var relay: RelayClient
    @ObservationIgnored public let env: PushEnv
    @ObservationIgnored let defaults: UserDefaults
    @ObservationIgnored let keys: Keychain
    @ObservationIgnored private var handles = HandleCache()
    @ObservationIgnored private var sealing: [String: Task<PushHandle, any Error>] = [:]
    @ObservationIgnored private var stores: [Weak] = []
    /// Each Mac's Live Activity and its update token, by Mac serverId, kept until that Mac hears of it.
    @ObservationIgnored private(set) var activities: [String: (id: String, token: String)] = [:]

    private struct Weak {
        weak var store: PushStore?
    }

    static let prefsKey = "push.prefs"
    static let liveActivitiesKey = "push.liveActivities"
    public static let defaultPrefs = PushPrefs(needsYou: true, done: true, failed: true, automations: true, details: false)

    public init(defaults: UserDefaults, keys: Keychain, relay: RelayClient = RelayClient(), env: PushEnv = .current) {
        self.defaults = defaults
        self.keys = keys
        self.relay = relay
        self.env = env
        prefs = defaults.data(forKey: Self.prefsKey).flatMap { try? JSONDecoder().decode(PushPrefs.self, from: $0) } ?? Self.defaultPrefs
        liveActivities = defaults.object(forKey: Self.liveActivitiesKey) as? Bool ?? true
    }

    public func granted(_ granted: Bool) {
        let permission: Permission = granted ? .granted : .denied
        guard permission != self.permission else { return }
        self.permission = permission
        registerEverywhere()
    }

    public func received(alertToken token: Data) {
        guard token.hex != alertToken else { return }
        alertToken = token.hex
        registerEverywhere()
    }

    public func received(startToken token: Data) {
        guard token.hex != startToken else { return }
        startToken = token.hex
        registerEverywhere()
    }

    public func frequent(_ on: Bool) {
        guard on != frequent else { return }
        frequent = on
        registerEverywhere()
    }

    public func set(_ prefs: PushPrefs) {
        guard prefs != self.prefs else { return }
        self.prefs = prefs
        defaults.set(try? JSONEncoder().encode(prefs), forKey: Self.prefsKey)
        for store in live { store.sendPrefs() }
    }

    /// Off ends every Live Activity and stops the Macs starting one by push.
    public func set(liveActivities on: Bool) {
        guard on != liveActivities else { return }
        liveActivities = on
        defaults.set(on, forKey: Self.liveActivitiesKey)
        if !on { endActivities?() }
        registerEverywhere()
    }

    /// A Live Activity's update token for the Mac `mac`; sent now if that Mac is connected, else once it is.
    public func activity(mac: String, id: String, token: Data) {
        activities[mac] = (id, token.hex)
        store(mac: mac)?.sendActivity()
    }

    public func activityEnded(mac: String, id: String) {
        guard activities[mac]?.id == id else { return }
        activities[mac] = nil
        store(mac: mac)?.sendEnded(id)
    }

    /// Records a Mac's count of sessions wanting a look; the app's badge is the sum across Macs.
    func badge(mac: String, count: Int) {
        let total = BadgeLedger(defaults: defaults).record(mac: mac, count: count)
        #if os(iOS)
            UNUserNotificationCenter.current().setBadgeCount(total, withCompletionHandler: nil)
        #else
            _ = total
        #endif
    }

    /// Drops what this phone kept for a Mac it no longer gets pushes from.
    func forget(mac: String) {
        try? keys.remove(mac)
        BadgeLedger(defaults: defaults).forget(mac: mac)
        activities[mac] = nil
    }

    func attach(_ store: PushStore) {
        stores.removeAll { $0.store == nil }
        stores.append(Weak(store: store))
    }

    private var live: [PushStore] { stores.compactMap(\.store) }

    func store(mac: String) -> PushStore? { live.first { $0.mac == mac } }

    private func registerEverywhere() {
        for store in live { Task { await store.register() } }
    }

    /// What `push.register` tells the Mac `mac`, or nil until this phone may be pushed to.
    func registration(mac: String) async throws -> ClientMessage.PushRegister? {
        guard permission == .granted, let alertToken else { return nil }
        let key = try key(mac: mac)
        let alert = try await handle(token: alertToken, kind: .alert)
        var start: PushHandle?
        if liveActivities, let startToken { start = try? await handle(token: startToken, kind: .start) }
        return .init(key: key, env: env.rawValue, alert: alert, start: start, prefs: prefs, frequent: frequent)
    }

    /// The relay handle for a token, sealed once however many Macs ask at the same time.
    func handle(token: String, kind: RelayKind) async throws -> PushHandle {
        if let h = handles.handle(token: token, kind: kind, env: env, now: Date()) { return h }
        let key = "\(kind.rawValue):\(token)"
        if let running = sealing[key] { return try await running.value }
        let task = Task { [relay, env] in try await relay.register(token: token, kind: kind, env: env) }
        sealing[key] = task
        defer { sealing[key] = nil }
        let h = try await task.value
        handles.keep(h, token: token, kind: kind, env: env)
        return h
    }

    /// The Mac's sealing key, made the first time this phone registers with it. The Notification Service Extension reads it from the shared keychain.
    func key(mac: String) throws -> String {
        if let data = try keys.data(for: mac) { return data.base64EncodedString() }
        let data = NoticeBox.newKey().withUnsafeBytes { Data($0) }
        try keys.set(data, for: mac)
        return data.base64EncodedString()
    }
}
