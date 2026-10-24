/// A named place over its project's own checkout. Its id never starts with "/".
public struct Local: Codable, Hashable, Sendable, Identifiable {
    public var id: String
    public var project: String
    public var name: String

    public init(id: String, project: String, name: String) {
        self.id = id
        self.project = project
        self.name = name
    }

    /// 1 to 64 of A-Z, a-z, 0-9, _ and -.
    public static func isValidId(_ id: String) -> Bool {
        (1...64).contains(id.utf8.count) && id.utf8.allSatisfy { c in
            (c >= 0x61 && c <= 0x7a) || (c >= 0x41 && c <= 0x5a) || (c >= 0x30 && c <= 0x39) || c == 0x5f || c == 0x2d
        }
    }
}

/// Every Local in creation order, sent whole on hello and on each change.
public struct LocalList: Codable, Hashable, Sendable {
    public var locals: [Local]
    public init(locals: [Local]) { self.locals = locals }
}

extension ClientMessage {
    public struct LocalCreate: Codable, Hashable, Sendable {
        public var localId: String
        public var project: String
        public var name: String
        public init(localId: String, project: String, name: String) {
            self.localId = localId
            self.project = project
            self.name = name
        }
    }

    public struct LocalRename: Codable, Hashable, Sendable {
        public var localId: String
        public var title: String
        public init(localId: String, title: String) {
            self.localId = localId
            self.title = title
        }
    }

    public struct LocalRef: Codable, Hashable, Sendable {
        public var localId: String
        public init(localId: String) { self.localId = localId }
    }
}
