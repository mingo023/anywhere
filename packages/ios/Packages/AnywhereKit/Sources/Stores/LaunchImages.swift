import ChatModel
import Client
import Foundation
import Observation

/// The images attached to a session being started. Each uploads as it's picked, and the first prompt names where they
/// landed, as a chat's does.
@MainActor
@Observable
public final class LaunchImages {
    public private(set) var list: [ChatStore.Attachment] = []
    /// An image picked that couldn't be read.
    public private(set) var notice: String?
    @ObservationIgnored private let files: (any ChatFiles)?
    @ObservationIgnored private let macName: String
    /// Each image's data until it lands, so a failed upload can go again.
    @ObservationIgnored private var data: [String: Data] = [:]

    public init(files: (any ChatFiles)?, macName: String, list: [ChatStore.Attachment] = []) {
        self.files = files
        self.list = list
        self.macName = macName
    }

    /// Start waits while an image uploads, and for a failed one to go again or be removed.
    public var blocksStart: Bool { list.uploading || list.failed }

    public func attach(_ image: Data, name: String, thumbnail: Data? = nil) {
        let id = UUID().uuidString
        notice = nil
        list.append(.init(id: id, name: name, thumbnail: thumbnail))
        data[id] = image
        upload(id)
    }

    public func unreadable() {
        notice = Composer.unreadableImage
    }

    public func retry(_ id: String) {
        guard data[id] != nil else { return }
        list.mark(id) {
            $0.failed = false
            $0.reason = nil
        }
        upload(id)
    }

    public func detach(_ id: String) {
        list.removeAll { $0.id == id }
        data[id] = nil
    }

    func upload(_ id: String) {
        guard let image = data[id], let name = list.first(where: { $0.id == id })?.name else { return }
        guard let files else { return failed(id, DaemonError.offline) }
        Task { [weak self] in
            do {
                let path = try await files.upload(image, name: name)
                self?.landed(id, path)
            } catch {
                self?.failed(id, error)
            }
        }
    }

    func landed(_ id: String, _ path: String) {
        data[id] = nil
        list.mark(id) { $0.path = path }
    }

    func failed(_ id: String, _ error: any Error) {
        let reason = FilesStore.describe(error, mac: macName)
        list.mark(id) {
            $0.failed = true
            $0.reason = reason
        }
    }
}
