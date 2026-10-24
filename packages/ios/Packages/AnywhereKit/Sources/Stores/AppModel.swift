import Client
import Observation
import Wire

/// The app's one model: every paired Mac.
@MainActor
@Observable
public final class AppModel {
    public private(set) var hosts: [HostModel] = []
    /// Where pairing saves each Mac; nil keeps nothing.
    @ObservationIgnored public let vault: (any CredentialVault)?
    /// What the vault holds, so a host is saved again only when its record changes.
    @ObservationIgnored private var saved: [String: Credentials] = [:]

    public init(vault: (any CredentialVault)? = nil) {
        self.vault = vault
    }

    /// Every saved Mac, connecting; a Mac that revoked this phone waits for a new code instead.
    public static func restore(from vault: any CredentialVault) -> AppModel {
        let app = AppModel(vault: vault)
        for credentials in (try? vault.all()) ?? [] {
            app.saved[credentials.deviceId] = credentials
            let host = app.add(credentials, start: !credentials.isRevoked)
            if credentials.isRevoked { host.apply([.closed(.revoked)]) }
        }
        return app
    }

    /// Adds a Mac and starts connecting to it; a Mac already added is returned as is.
    @discardableResult
    public func add(_ credentials: Credentials, start: Bool = true) -> HostModel {
        if let host = hosts.first(where: { $0.id == credentials.deviceId }) { return host }
        let host = HostModel(credentials: credentials)
        hosts.append(host)
        watch(host)
        if start { host.start() }
        return host
    }

    public func remove(_ deviceId: String) {
        guard let at = hosts.firstIndex(where: { $0.id == deviceId }) else { return }
        hosts[at].stop()
        hosts.remove(at: at)
    }

    /// The Mac with this `hello.ok` serverId, as deep links name it, or the device id a Mac is known by before its first hello.
    public func host(mac serverId: String) -> HostModel? {
        hosts.first { ($0.hello?.serverId ?? $0.credentials.serverId) == serverId } ?? hosts.first { $0.id == serverId }
    }

    /// Stops using a Mac and deletes what this phone kept for it.
    public func forget(_ deviceId: String) {
        remove(deviceId)
        saved[deviceId] = nil
        try? vault?.remove(deviceId: deviceId)
    }

    /// Dials a Mac whose link ended (after an update) with a fresh connection.
    public func restart(_ deviceId: String) {
        guard let at = hosts.firstIndex(where: { $0.id == deviceId }) else { return }
        let credentials = saved[deviceId] ?? hosts[at].credentials
        guard !credentials.isRevoked else { return }
        hosts[at].stop()
        let host = HostModel(credentials: credentials)
        hosts[at] = host
        watch(host)
        host.start()
    }

    /// Saves a Mac `pair.ok` admitted this phone to, replacing any earlier record of the same Mac, and connects to it.
    @discardableResult
    public func paired(_ ok: PairOk, link: PairingLink) -> HostModel {
        let earlier = hosts.first { $0.credentials.host == link.host }
        let credentials = Credentials(
            host: link.host, deviceToken: ok.token, deviceId: ok.deviceId, macName: link.name,
            clientId: earlier?.credentials.clientId ?? Credentials.newClientId(), serverId: earlier.flatMap { saved[$0.id]?.serverId })
        if let earlier { forget(earlier.id) }
        save(credentials)
        return add(credentials)
    }

    /// Pairs with the Mac a link names: trades its code for a token, saves it, and connects.
    @discardableResult
    public func pair(_ link: PairingLink, phoneName: String) async throws(PairingFailure) -> HostModel {
        let ok = try await Pairer.pair(host: link.host, code: link.code, phoneName: phoneName)
        return paired(ok, link: link)
    }

    /// Keeps the record in step with what the Mac said: its serverId and name once it says hello, and no token once it
    /// revokes this phone (the host stays, so pairing again needs only a new code).
    func noticed(_ host: HostModel) {
        var record = saved[host.id] ?? host.credentials
        if host.link == .ended(.revoked) {
            record.deviceToken = ""
        } else if let hello = host.hello {
            record.serverId = hello.serverId
            record.macName = hello.hostname
        }
        save(record)
    }

    private func save(_ credentials: Credentials) {
        guard saved[credentials.deviceId] != credentials else { return }
        saved[credentials.deviceId] = credentials
        try? vault?.save(credentials)
    }

    private func watch(_ host: HostModel) {
        withObservationTracking {
            _ = host.link
            _ = host.hello
        } onChange: { [weak self, weak host] in
            Task { @MainActor in
                guard let self, let host, self.hosts.contains(where: { $0 === host }) else { return }
                self.noticed(host)
                self.watch(host)
            }
        }
    }
}
