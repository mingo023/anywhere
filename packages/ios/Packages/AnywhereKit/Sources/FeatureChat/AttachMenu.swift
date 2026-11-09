import DesignSystem
import SwiftUI
#if os(iOS)
    import PhotosUI
    import UniformTypeIdentifiers
#endif

/// The composer's +: an image from Photos, the camera, Files or the clipboard. `attach` takes its data, a file name,
/// and a thumbnail; reading, encoding and the thumbnail happen off the main actor.
@MainActor
struct AttachMenu: View {
    typealias Attach = (_ data: Data, _ name: String, _ thumbnail: Data?) -> Void

    let enabled: Bool
    let attach: Attach
    #if os(iOS)
        @State private var photos: [PhotosPickerItem] = []
        @State private var choosingPhotos = false
        @State private var takingPhoto = false
        @State private var choosingFiles = false
    #endif

    var body: some View {
        #if os(iOS)
            Menu {
                Button("Photo Library", systemImage: "photo.on.rectangle") { choosingPhotos = true }
                if UIImagePickerController.isSourceTypeAvailable(.camera) {
                    Button("Take Photo", systemImage: "camera") { takingPhoto = true }
                }
                Button("Choose File", systemImage: "folder") { choosingFiles = true }
                if UIPasteboard.general.hasImages {
                    Button("Paste Image", systemImage: "doc.on.clipboard") {
                        Self.pasteClipboard(to: attach)
                    }
                }
            } label: {
                Image(systemName: "plus").font(.system(size: 15, weight: .medium)).foregroundStyle(Palette.text2.color).hitTarget()
            }
            .menuStyle(.button)
            .menuIndicator(.hidden)
            .buttonStyle(.plain)
            .fixedSize()
            .disabled(!enabled)
            .accessibilityLabel("Attach images")
            .photosPicker(isPresented: $choosingPhotos, selection: $photos, maxSelectionCount: 4, matching: .images)
            .onChange(of: photos) { _, items in
                guard !items.isEmpty else { return }
                photos = []
                for item in items {
                    let ext = item.supportedContentTypes.first?.preferredFilenameExtension ?? "jpg"
                    Task { @MainActor in
                        guard let data = try? await item.loadTransferable(type: Data.self) else { return }
                        add(ext: ext) { data }
                    }
                }
            }
            .fullScreenCover(isPresented: $takingPhoto) {
                CameraPicker { image in add(ext: "jpg") { image.jpegData(compressionQuality: 0.85) } }.ignoresSafeArea()
            }
            .fileImporter(isPresented: $choosingFiles, allowedContentTypes: [.image], allowsMultipleSelection: true) { result in
                guard case .success(let urls) = result else { return }
                for url in urls {
                    add(ext: url.pathExtension.isEmpty ? "jpg" : url.pathExtension) {
                        let scoped = url.startAccessingSecurityScopedResource()
                        defer { if scoped { url.stopAccessingSecurityScopedResource() } }
                        return try? Data(contentsOf: url)
                    }
                }
            }
        #else
            EmptyView()
        #endif
    }

    #if os(iOS)
        /// The clipboard's own JPEG or PNG bytes when it has them; a PNG re-encode of a photo can run to many MB.
        static func pasteClipboard(to attach: @escaping Attach) {
            let board = UIPasteboard.general
            for (type, ext) in [(UTType.jpeg, "jpg"), (.png, "png")] {
                if let data = board.data(forPasteboardType: type.identifier) { return add(ext: ext, { data }, to: attach) }
            }
            guard let image = board.image else { return }
            add(ext: "jpg", { image.jpegData(compressionQuality: 0.85) }, to: attach)
        }
    #endif

    private func add(ext: String, _ load: @escaping @Sendable () -> Data?) {
        Self.add(ext: ext, load, to: attach)
    }

    static func add(ext: String, _ load: @escaping @Sendable () -> Data?, to attach: @escaping Attach) {
        let name = "\(UUID().uuidString.prefix(8)).\(ext)"
        Task {
            let loaded = await Task.detached(priority: .userInitiated) { () -> (Data, Data?)? in
                guard let data = load() else { return nil }
                return (data, Thumbnail.make(data))
            }.value
            guard let (data, thumbnail) = loaded else { return }
            attach(data, name, thumbnail)
        }
    }
}
