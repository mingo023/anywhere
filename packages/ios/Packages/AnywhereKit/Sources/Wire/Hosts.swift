/// How ssh reaches a host: an alias from `~/.ssh/config`, or an address. An empty `user` is the Mac's user.
public enum HostTarget: Codable, Hashable, Sendable {
    case alias(name: String)
    case address(addr: String, port: Int, user: String? = nil, keyFile: String? = nil)

    enum Keys: String, CodingKey {
        case kind, name, addr, port, user, keyFile
    }

    public init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        switch try c.decode(String.self, forKey: .kind) {
        case "alias": self = .alias(name: try c.decode(String.self, forKey: .name))
        case "address":
            self = .address(
                addr: try c.decode(String.self, forKey: .addr), port: try c.decode(Int.self, forKey: .port),
                user: try c.decodeIfPresent(String.self, forKey: .user), keyFile: try c.decodeIfPresent(String.self, forKey: .keyFile))
        case let kind: throw malformed(decoder, "unknown host target kind \(kind)")
        }
    }

    public func encode(to encoder: any Encoder) throws {
        var c = encoder.container(keyedBy: Keys.self)
        switch self {
        case .alias(let name):
            try c.encode("alias", forKey: .kind)
            try c.encode(name, forKey: .name)
        case .address(let addr, let port, let user, let keyFile):
            try c.encode("address", forKey: .kind)
            try c.encode(addr, forKey: .addr)
            try c.encode(port, forKey: .port)
            try c.encodeIfPresent(user, forKey: .user)
            try c.encodeIfPresent(keyFile, forKey: .keyFile)
        }
    }
}

/// Why a host's link failed, or a sign-in was refused.
public struct HostFailure: OpenString {
    public let rawValue: String
    public init(rawValue: String) { self.rawValue = rawValue }
    public static let auth: Self = "auth"
    public static let unknownKey: Self = "unknown_key"
    public static let keyChanged: Self = "key_changed"
    public static let noSuchHost: Self = "no_such_host"
    public static let refused: Self = "refused"
    public static let timedOut: Self = "timed_out"
    public static let dropped: Self = "dropped"
    public static let throttled: Self = "throttled"
    public static let notRunning: Self = "not_running"
    public static let unpaired: Self = "unpaired"
    public static let pathTooLong: Self = "path_too_long"
    public static let other: Self = "other"
}

/// A host's connection. `retrying` tries again by itself; `failed` waits for `host.retry`.
public struct HostLinkState: Codable, Hashable, Sendable {
    public struct State: OpenString {
        public let rawValue: String
        public init(rawValue: String) { self.rawValue = rawValue }
        public static let connecting: Self = "connecting"
        public static let online: Self = "online"
        public static let retrying: Self = "retrying"
        public static let failed: Self = "failed"
    }

    public var state: State
    /// Online only.
    public var version: String?
    /// Online only, in seconds.
    public var uptime: Int?
    /// Retrying and failed only.
    public var failure: HostFailure?

    public init(state: State, version: String? = nil, uptime: Int? = nil, failure: HostFailure? = nil) {
        self.state = state
        self.version = version
        self.uptime = uptime
        self.failure = failure
    }
}

public struct Host: Codable, Hashable, Sendable, Identifiable {
    public var id: String
    public var name: String
    public var target: HostTarget
    public var windows: Bool?
    /// The local end of the tunnel to the host's pocketd.
    public var socket: String
    public var link: HostLinkState

    public init(id: String, name: String, target: HostTarget, windows: Bool? = nil, socket: String, link: HostLinkState) {
        self.id = id
        self.name = name
        self.target = target
        self.windows = windows
        self.socket = socket
        self.link = link
    }
}

/// `host.list`: the whole list, as a reply or pushed to an owner.
public struct HostList: Codable, Hashable, Sendable {
    public var hosts: [Host]
    public init(hosts: [Host]) { self.hosts = hosts }
}

/// `host.update`: the host `host.add` added, or one pushed after it changed.
public struct HostUpdate: Codable, Hashable, Sendable {
    public var host: Host
    public init(host: Host) { self.host = host }
}

/// A step of a `host.install`. `running`, `done` and `error` end it.
public struct InstallProgress: Codable, Hashable, Sendable {
    public struct Kind: OpenString {
        public let rawValue: String
        public init(rawValue: String) { self.rawValue = rawValue }
        public static let step: Self = "step"
        public static let warn: Self = "warn"
        public static let running: Self = "running"
        public static let done: Self = "done"
        public static let error: Self = "error"
    }

    public struct Step: OpenString {
        public let rawValue: String
        public init(rawValue: String) { self.rawValue = rawValue }
        public static let detect: Self = "detect"
        public static let download: Self = "download"
        public static let verify: Self = "verify"
        public static let unpack: Self = "unpack"
        public static let link: Self = "link"
        public static let service: Self = "service"
        public static let start: Self = "start"
    }

    public var kind: Kind
    public var step: Step?
    public var text: String?

    public init(kind: Kind, step: Step? = nil, text: String? = nil) {
        self.kind = kind
        self.step = step
        self.text = text
    }
}

public struct HostProgress: Codable, Hashable, Sendable {
    public var hostId: String
    public var update: Bool
    public var progress: InstallProgress
    public init(hostId: String, update: Bool, progress: InstallProgress) {
        self.hostId = hostId
        self.update = update
        self.progress = progress
    }
}

/// A timeline item: a card asking the user to add a host, with `hint` to fill its form.
public struct HostLink: Codable, Hashable, Sendable {
    public var environmentId: String?
    public var hint: String?
    public var reason: String?
    public init(environmentId: String? = nil, hint: String? = nil, reason: String? = nil) {
        self.environmentId = environmentId
        self.hint = hint
        self.reason = reason
    }
}

extension ClientMessage {
    public struct HostAdd: Codable, Hashable, Sendable {
        public var target: HostTarget
        public var name: String?
        public init(target: HostTarget, name: String? = nil) {
            self.target = target
            self.name = name
        }
    }

    /// `host.retry`.
    public struct HostRef: Codable, Hashable, Sendable {
        public var hostId: String
        public init(hostId: String) { self.hostId = hostId }
    }

    public struct HostRemove: Codable, Hashable, Sendable {
        public var hostId: String
        public var uninstall: Bool?
        public init(hostId: String, uninstall: Bool? = nil) {
            self.hostId = hostId
            self.uninstall = uninstall
        }
    }

    public struct HostInstall: Codable, Hashable, Sendable {
        public var hostId: String
        public var update: Bool?
        public var latest: Bool?
        public var archiveDir: String?
        public init(hostId: String, update: Bool? = nil, latest: Bool? = nil, archiveDir: String? = nil) {
            self.hostId = hostId
            self.update = update
            self.latest = latest
            self.archiveDir = archiveDir
        }
    }

    public struct HostSignIn: Codable, Hashable, Sendable {
        public var hostId: String
        public var password: String
        public init(hostId: String, password: String) {
            self.hostId = hostId
            self.password = password
        }
    }
}
