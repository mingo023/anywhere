import Foundation

public struct FsEntry: Codable, Hashable, Sendable {
    public var name: String
    public var dir: Bool
    public var link: Bool?
    public var size: Int
    /// Unix seconds.
    public var mtime: Int
    public var ignored: Bool?
    /// A changed file's status letter, or the shared letter of a folder's changes.
    public var git: String?
    public init(name: String, dir: Bool, link: Bool? = nil, size: Int, mtime: Int, ignored: Bool? = nil, git: String? = nil) {
        self.name = name
        self.dir = dir
        self.link = link
        self.size = size
        self.mtime = mtime
        self.ignored = ignored
        self.git = git
    }
}

/// A folder's entries, folders first, then by name.
public struct FsList: Codable, Hashable, Sendable {
    public var tree: String
    public var dir: String
    public var entries: [FsEntry]
    public init(tree: String, dir: String, entries: [FsEntry]) {
        self.tree = tree
        self.dir = dir
        self.entries = entries
    }
}

public struct FileKind: OpenString {
    public let rawValue: String
    public init(rawValue: String) { self.rawValue = rawValue }
    public static let text: Self = "text"
    public static let image: Self = "image"
    public static let binary: Self = "binary"
    public static let tooLarge: Self = "tooLarge"
    public static let unreadable: Self = "unreadable"
}

public struct FsRead: Codable, Hashable, Sendable {
    public var tree: String
    public var path: String
    public var kind: FileKind
    public var size: Int
    public var mtime: Int
    public var text: String?
    public var truncated: Bool?
    public var language: String?
    public var mime: String?
    /// An image's bytes, up to 20 MB; base64 on the wire.
    public var data: Data?
    public init(
        tree: String, path: String, kind: FileKind, size: Int, mtime: Int, text: String? = nil, truncated: Bool? = nil, language: String? = nil,
        mime: String? = nil, data: Data? = nil
    ) {
        self.tree = tree
        self.path = path
        self.kind = kind
        self.size = size
        self.mtime = mtime
        self.text = text
        self.truncated = truncated
        self.language = language
        self.mime = mime
        self.data = data
    }
}

public struct FsSearch: Codable, Hashable, Sendable {
    public var paths: [String]
    public init(paths: [String]) { self.paths = paths }
}

/// An upload landed, at `path` under pocketd's attachments.
public struct AttachDone: Codable, Hashable, Sendable {
    public var uploadId: String
    public var path: String
    public init(uploadId: String, path: String) {
        self.uploadId = uploadId
        self.path = path
    }
}

extension ClientMessage {
    public struct FsList: Codable, Hashable, Sendable {
        public var tree: String
        /// "" is the tree's root.
        public var dir: String
        public init(tree: String, dir: String) {
            self.tree = tree
            self.dir = dir
        }
    }

    public struct FsRead: Codable, Hashable, Sendable {
        public var tree: String
        public var path: String
        public var max: Int?
        public init(tree: String, path: String, max: Int? = nil) {
            self.tree = tree
            self.path = path
            self.max = max
        }
    }

    public struct FsSearch: Codable, Hashable, Sendable {
        public var tree: String
        public var query: String
        public var limit: Int?
        public init(tree: String, query: String, limit: Int? = nil) {
            self.tree = tree
            self.query = query
            self.limit = limit
        }
    }

    /// One chunk of an image upload: at most 512 KiB decoded, 20 MB in all.
    public struct AttachPut: Codable, Hashable, Sendable {
        public var uploadId: String
        public var name: String
        public var index: Int
        public var data: Data
        public var last: Bool
        public init(uploadId: String, name: String, index: Int, data: Data, last: Bool) {
            self.uploadId = uploadId
            self.name = name
            self.index = index
            self.data = data
            self.last = last
        }
    }
}
