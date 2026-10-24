import Client
import Observation
import Wire

/// The app's one model: every paired Mac.
@MainActor
@Observable
public final class AppModel {
    public private(set) var hosts: [HostModel] = []

    public init() {}

    /// Every saved Mac, connecting.
    public static func restore(from store: CredentialsStore) -> AppModel {
        let app = AppModel()
        for credentials in (try? store.all()) ?? [] { app.add(credentials) }
        return app
    }

    /// Adds a Mac and starts connecting to it; a Mac already added is returned as is.
    @discardableResult
    public func add(_ credentials: Credentials, start: Bool = true) -> HostModel {
        if let host = hosts.first(where: { $0.id == credentials.deviceId }) { return host }
        let host = HostModel(credentials: credentials)
        hosts.append(host)
        if start { host.start() }
        return host
    }

    public func remove(_ deviceId: String) {
        guard let at = hosts.firstIndex(where: { $0.id == deviceId }) else { return }
        hosts[at].stop()
        hosts.remove(at: at)
    }

    /// The Mac with this `hello.ok` serverId, as deep links name it.
    public func host(mac serverId: String) -> HostModel? {
        hosts.first { ($0.hello?.serverId ?? $0.credentials.serverId) == serverId }
    }
}
