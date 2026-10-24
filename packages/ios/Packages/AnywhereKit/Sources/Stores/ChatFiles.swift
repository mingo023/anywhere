import Client
import Foundation
import Wire

/// What the composer needs of the Mac's files: names to mention and a place for attached images.
public protocol ChatFiles: Sendable {
    /// Paths under `tree` that fuzzily match `query`, best first.
    func search(tree: String, query: String) async throws -> [String]
    /// Uploads an image and returns where it landed on the Mac, for the prompt to name.
    func upload(_ data: Data, name: String) async throws -> String
}

/// `fs.search` and `attach.put` over a Mac's connection.
public struct ConnectionFiles: ChatFiles {
    /// Bytes an `attach.put` chunk holds at most, decoded.
    static let chunk = 512 * 1024
    let connection: DaemonConnection

    public init(_ connection: DaemonConnection) { self.connection = connection }

    public func search(tree: String, query: String) async throws -> [String] {
        guard case .fsSearch(let found) = try await connection.request(.fsSearch(.init(tree: tree, query: query))) else { return [] }
        return found.paths
    }

    public func upload(_ data: Data, name: String) async throws -> String {
        let id = Self.uploadId()
        let count = max((data.count + Self.chunk - 1) / Self.chunk, 1)
        for index in 0..<count {
            let part = data.subdata(in: index * Self.chunk..<min((index + 1) * Self.chunk, data.count))
            let reply = try await connection.request(.attachPut(.init(uploadId: id, name: name, index: index, data: part, last: index == count - 1)))
            if case .attachDone(let done) = reply { return done.path }
        }
        throw DaemonError.closed
    }

    static func uploadId() -> String {
        let letters = Array("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789")
        return String((0..<24).map { _ in letters.randomElement()! })
    }
}
