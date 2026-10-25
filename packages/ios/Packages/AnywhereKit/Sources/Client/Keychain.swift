import Foundation
import Security

/// Generic-password items of one service, readable after first unlock and never synced or backed up off this device.
/// With an access group, the app's extensions read the same items.
public struct Keychain: Sendable {
    public struct Failure: Error, Hashable, Sendable {
        public let status: OSStatus
    }

    /// Shared by the app and its extensions; an App Group id serves as a keychain access group.
    public static let appGroup = "group.dev.mingo.anywhere"

    public let service: String
    public let accessGroup: String?

    public init(service: String, accessGroup: String? = nil) {
        self.service = service
        self.accessGroup = accessGroup
    }

    public func data(for account: String) throws -> Data? {
        var query = base(account)
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        var item: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &item)
        if status == errSecItemNotFound { return nil }
        guard status == errSecSuccess else { throw Failure(status: status) }
        return item as? Data
    }

    public func set(_ data: Data, for account: String) throws {
        let update = [kSecValueData as String: data] as CFDictionary
        var status = SecItemUpdate(base(account) as CFDictionary, update)
        if status == errSecItemNotFound {
            var add = base(account)
            add[kSecValueData as String] = data
            add[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
            status = SecItemAdd(add as CFDictionary, nil)
        }
        guard status == errSecSuccess else { throw Failure(status: status) }
    }

    public func remove(_ account: String) throws {
        let status = SecItemDelete(base(account) as CFDictionary)
        guard status == errSecSuccess || status == errSecItemNotFound else { throw Failure(status: status) }
    }

    public func accounts() throws -> [String] {
        var query = base(nil)
        query[kSecReturnAttributes as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitAll
        var items: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &items)
        if status == errSecItemNotFound { return [] }
        guard status == errSecSuccess else { throw Failure(status: status) }
        let list = items as? [[String: Any]] ?? []
        return list.compactMap { $0[kSecAttrAccount as String] as? String }.sorted()
    }

    private func base(_ account: String?) -> [String: Any] {
        var query: [String: Any] = [kSecClass as String: kSecClassGenericPassword, kSecAttrService as String: service]
        if let account { query[kSecAttrAccount as String] = account }
        if let accessGroup { query[kSecAttrAccessGroup as String] = accessGroup }
        #if os(iOS)
            query[kSecUseDataProtectionKeychain as String] = true
        #endif
        return query
    }
}
