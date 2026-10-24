import Domain
import Foundation
import Wire

/// An image the chat can't attach.
public enum AttachmentError: Error, Hashable, Sendable {
    case unsupportedType
    case tooLarge
    case empty
}

/// How an image goes up with `attach.put`: the types the desktop's composer takes, 512 KiB chunks, 20 MB in all.
public enum Attachments {
    public static let types: Set<String> = ["png", "jpg", "jpeg", "gif", "webp", "heic"]

    public static func check(_ data: Data, name: String) throws(AttachmentError) {
        guard types.contains(FileText.ext(name).lowercased()) else { throw .unsupportedType }
        guard !data.isEmpty else { throw .empty }
        guard data.count <= Limits.attachTotal else { throw .tooLarge }
    }

    public static func chunkCount(_ size: Int) -> Int { max(1, (size + Limits.attachChunk - 1) / Limits.attachChunk) }

    /// Chunk `index` of `data`; the last one says so.
    public static func chunk(_ index: Int, of data: Data, uploadId: String, name: String) -> ClientMessage.AttachPut {
        let start = data.startIndex + index * Limits.attachChunk
        let end = min(start + Limits.attachChunk, data.endIndex)
        return ClientMessage.AttachPut(
            uploadId: uploadId, name: name, index: index, data: data.subdata(in: start..<end), last: index == chunkCount(data.count) - 1)
    }

    /// An upload id pocketd takes: 1 to 64 of A-Z a-z 0-9 _ -.
    public static func newId() -> String { UUID().uuidString.lowercased() }
}
