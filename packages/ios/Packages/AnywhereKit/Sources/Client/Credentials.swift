import Foundation

/// What the phone keeps for one paired Mac.
public struct Credentials: Codable, Hashable, Sendable, Identifiable {
    /// `name:port` of the Mac's pocketd.
    public var host: String
    public var deviceToken: String
    /// This phone's id on that Mac, from `pair.ok`; it keys the record.
    public var deviceId: String
    public var macName: String
    /// The id this phone says hello with.
    public var clientId: String
    /// The Mac's `hello.ok` serverId, once it has answered a hello.
    public var serverId: String?

    public var id: String { deviceId }

    public init(host: String, deviceToken: String, deviceId: String, macName: String, clientId: String, serverId: String? = nil) {
        self.host = host
        self.deviceToken = deviceToken
        self.deviceId = deviceId
        self.macName = macName
        self.clientId = clientId
        self.serverId = serverId
    }

    /// `ios-<ms base36>-<8 random>`.
    public static func newClientId(os: String = "ios", now: Date = Date()) -> String {
        let ms = String(Int(now.timeIntervalSince1970 * 1000), radix: 36)
        let alphabet = Array("0123456789abcdefghijklmnopqrstuvwxyz")
        let random = String((0..<8).map { _ in alphabet.randomElement()! })
        return "\(os)-\(ms)-\(random)"
    }
}

/// Every paired Mac's credentials, one keychain item each.
public struct CredentialsStore: Sendable {
    public static let service = "dev.mingo.anywhere.macs"

    public let keychain: Keychain

    public init(keychain: Keychain = Keychain(service: CredentialsStore.service)) {
        self.keychain = keychain
    }

    /// The saved Macs; a record that no longer decodes is skipped.
    public func all() throws -> [Credentials] {
        try keychain.accounts().compactMap { account in
            try keychain.data(for: account).flatMap { try? JSONDecoder().decode(Credentials.self, from: $0) }
        }
    }

    public func save(_ credentials: Credentials) throws {
        try keychain.set(JSONEncoder().encode(credentials), for: credentials.deviceId)
    }

    public func remove(deviceId: String) throws {
        try keychain.remove(deviceId)
    }
}
