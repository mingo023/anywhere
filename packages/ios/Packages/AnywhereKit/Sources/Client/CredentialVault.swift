import Foundation

/// Where paired Macs' credentials are kept: the keychain in the app, memory in tests, the gallery and the probe.
public protocol CredentialVault: Sendable {
    func all() throws -> [Credentials]
    func save(_ credentials: Credentials) throws
    func remove(deviceId: String) throws
}

extension CredentialsStore: CredentialVault {}

/// Credentials held in memory only.
public final class MemoryVault: CredentialVault, @unchecked Sendable {
    private let lock = NSLock()
    private var saved: [String: Credentials]

    public init(_ credentials: [Credentials] = []) {
        saved = Dictionary(credentials.map { ($0.deviceId, $0) }, uniquingKeysWith: { $1 })
    }

    public func all() throws -> [Credentials] {
        lock.withLock { saved.values.sorted { $0.deviceId < $1.deviceId } }
    }

    public func save(_ credentials: Credentials) throws {
        lock.withLock { saved[credentials.deviceId] = credentials }
    }

    public func remove(deviceId: String) throws {
        lock.withLock { saved[deviceId] = nil }
    }
}

extension Credentials {
    /// A Mac that revoked this phone keeps its record without a token, so pairing again only needs a new code.
    public var isRevoked: Bool { deviceToken.isEmpty }
}
