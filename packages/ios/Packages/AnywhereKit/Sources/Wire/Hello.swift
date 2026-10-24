public struct VersionRange: Codable, Hashable, Sendable {
    public var min: Int
    public var max: Int
    public init(min: Int, max: Int) {
        self.min = min
        self.max = max
    }

    public static let phone = VersionRange(min: Caps.protocolMin, max: Caps.protocolMax)
}

/// What a peer may do; each verb needs one.
public struct Scope: OpenString {
    public let rawValue: String
    public init(rawValue: String) { self.rawValue = rawValue }
    public static let observe: Self = "observe"
    public static let drive: Self = "drive"
    public static let approve: Self = "approve"
    public static let spawn: Self = "spawn"
    public static let owner: Self = "owner"
}

public struct HostState: Codable, Hashable, Sendable {
    public var tailnet: Bool
    public var keepingAwake: Bool
    /// The owner setting phone.terminals: phones may type into, open and close terminals. Nil from an older pocketd.
    public var terminals: Bool?
    /// The version pocketd last failed to upgrade to.
    public var upgradeFailed: String?

    public init(tailnet: Bool, keepingAwake: Bool, terminals: Bool? = nil, upgradeFailed: String? = nil) {
        self.tailnet = tailnet
        self.keepingAwake = keepingAwake
        self.terminals = terminals
        self.upgradeFailed = upgradeFailed
    }
}

public struct HelloOk: Codable, Hashable, Sendable {
    public var serverId: String
    public var hostname: String
    public var protocolVersion: Int
    public var caps: [String]?
    public var `protocol`: VersionRange?
    public var scopes: [Scope]?
    public var host: HostState?
    public var version: String?

    public init(
        serverId: String, hostname: String, protocolVersion: Int, caps: [String]? = nil, protocol: VersionRange? = nil,
        scopes: [Scope]? = nil, host: HostState? = nil, version: String? = nil
    ) {
        self.serverId = serverId
        self.hostname = hostname
        self.protocolVersion = protocolVersion
        self.caps = caps
        self.protocol = `protocol`
        self.scopes = scopes
        self.host = host
        self.version = version
    }

    public func has(_ cap: String) -> Bool { caps?.contains(cap) ?? false }

    /// A Mac whose newest protocol is older than the phone's oldest.
    public var serverTooOld: Bool { (self.protocol?.max ?? protocolVersion) < Caps.protocolMin }
}

public struct PairOk: Codable, Hashable, Sendable {
    public var deviceId: String
    public var token: String
    public init(deviceId: String, token: String) {
        self.deviceId = deviceId
        self.token = token
    }
}

/// A code to pair another phone with, from `pair.begin`.
public struct PairOffer: Codable, Hashable, Sendable {
    public var url: String
    public var code: String
    /// Unix milliseconds.
    public var expiresAt: Int
    public init(url: String, code: String, expiresAt: Int) {
        self.url = url
        self.code = code
        self.expiresAt = expiresAt
    }
}

public struct PairDone: Codable, Hashable, Sendable {
    public var deviceId: String
    public var name: String
    public init(deviceId: String, name: String) {
        self.deviceId = deviceId
        self.name = name
    }
}

public struct HostChanged: Codable, Hashable, Sendable {
    public var host: HostState
    public init(host: HostState) { self.host = host }
}

extension ClientMessage {
    public struct Hello: Codable, Hashable, Sendable {
        public var token: String?
        public var clientId: String
        public var protocolVersion: Int
        public var caps: [String]?
        public var `protocol`: VersionRange?

        public init(token: String?, clientId: String, protocolVersion: Int = Caps.protocolMax, caps: [String]? = Caps.phone, protocol: VersionRange? = .phone) {
            self.token = token
            self.clientId = clientId
            self.protocolVersion = protocolVersion
            self.caps = caps
            self.protocol = `protocol`
        }
    }

    public struct Pair: Codable, Hashable, Sendable {
        public var code: String
        public var name: String
        /// "ios" or "android".
        public var platform: String
        public var `protocol`: VersionRange?

        public init(code: String, name: String, platform: String = "ios", protocol: VersionRange? = .phone) {
            self.code = code
            self.name = name
            self.platform = platform
            self.protocol = `protocol`
        }
    }
}
