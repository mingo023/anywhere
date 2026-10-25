/// A relay handle from `/v1/register`.
public struct PushHandle: Codable, Hashable, Sendable {
    public var handle: String
    public var secret: String
    /// Unix seconds.
    public var expiresAt: Int
    public init(handle: String, secret: String, expiresAt: Int) {
        self.handle = handle
        self.secret = secret
        self.expiresAt = expiresAt
    }
}

/// What this device wants pushed.
public struct PushPrefs: Codable, Hashable, Sendable {
    public var needsYou: Bool
    public var done: Bool
    public var failed: Bool
    public var automations: Bool
    /// Show the ask's verb and subject on the Live Activity.
    public var details: Bool
    public init(needsYou: Bool, done: Bool, failed: Bool, automations: Bool, details: Bool) {
        self.needsYou = needsYou
        self.done = done
        self.failed = failed
        self.automations = automations
        self.details = details
    }
}

extension ClientMessage {
    public struct PushRegister: Codable, Hashable, Sendable {
        /// Base64 32-byte AES-256-GCM key that seals alert content.
        public var key: String
        /// development or production.
        public var env: String
        public var alert: PushHandle
        public var start: PushHandle?
        public var prefs: PushPrefs
        public var frequent: Bool?
        public init(key: String, env: String, alert: PushHandle, start: PushHandle? = nil, prefs: PushPrefs, frequent: Bool? = nil) {
            self.key = key
            self.env = env
            self.alert = alert
            self.start = start
            self.prefs = prefs
            self.frequent = frequent
        }
    }

    /// One Live Activity per Mac.
    public struct PushActivity: Codable, Hashable, Sendable {
        public var activityId: String
        public var handle: PushHandle?
        public var ended: Bool?
        public init(activityId: String, handle: PushHandle? = nil, ended: Bool? = nil) {
            self.activityId = activityId
            self.handle = handle
            self.ended = ended
        }
    }

    public struct PushPrefsUpdate: Codable, Hashable, Sendable {
        public var prefs: PushPrefs
        public init(prefs: PushPrefs) { self.prefs = prefs }
    }
}
