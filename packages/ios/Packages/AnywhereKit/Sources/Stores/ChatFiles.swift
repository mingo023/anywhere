import Foundation
import Wire

/// What the composer needs of the Mac's files: names to mention, a place for attached images, and the files and lines
/// the Files screens handed to this chat.
@MainActor
public protocol ChatFiles: AnyObject {
    /// Paths under `tree` that fuzzily match `query`, best first.
    func search(tree: String, query: String) async throws -> [String]
    /// Uploads an image and returns where it landed on the Mac, for the prompt to name.
    func upload(_ data: Data, name: String) async throws -> String
    /// What waits for the chat of `agentId` on `tree`, oldest first, handed over once.
    func takeAdditions(for agentId: String, in tree: String) -> [String]
}

extension FilesStore: ChatFiles {
    public func search(tree: String, query: String) async throws -> [String] {
        try await search(tree: tree, query: query, limit: Limits.fsSearchLimit)
    }

    public func upload(_ data: Data, name: String) async throws -> String {
        try await upload(data, name: name, id: Attachments.newId())
    }
}
