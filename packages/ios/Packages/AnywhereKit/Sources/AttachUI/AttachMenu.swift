import DesignSystem
import SwiftUI
#if os(iOS)
    import PhotosUI
    import UniformTypeIdentifiers
#endif

/// The composer's +: an image from Photos, the camera, Files or the clipboard. `attach` takes its data, a file name,
/// and a thumbnail; reading, encoding and the thumbnail happen off the main actor.
@MainActor
public struct AttachMenu: View {
    public typealias Attach = (_ data: Data, _ name: String, _ thumbnail: Data?) -> Void

    let enabled: Bool
    /// Sits in a 34 pt filled circle, as in the chat's composer bar.
    let circled: Bool
    let attach: Attach
    /// A picked image whose bytes couldn't be read, as a photo still in iCloud while offline.
    let unreadable: () -> Void
    #if os(iOS)
        @State private var photos: [PhotosPickerItem] = []
        @State private var choosingPhotos = false
        @State private var takingPhoto = false
        @State private var choosingFiles = false
    #endif

    public init(enabled: Bool, circled: Bool = false, attach: @escaping Attach, unreadable: @escaping () -> Void = {}) {
        self.enabled = enabled
        self.circled = circled
        self.attach = attach
        self.unreadable = unreadable
    }

    public var body: some View {
        #if os(iOS)
            Menu {
                Button { choosingPhotos = true } label: { Label("Photo Library", lucide: .imagePlus) }
                if UIImagePickerController.isSourceTypeAvailable(.camera) {
                    Button { takingPhoto = true } label: { Label("Take Photo", lucide: .camera) }
                }
                Button { choosingFiles = true } label: { Label("Choose File", lucide: .folder) }
                if UIPasteboard.general.hasImages {
                    Button {
                        Self.pasteClipboard(to: attach)
                    } label: {
                        Label("Paste Image", lucide: .copy)
                    }
                }
            } label: {
                plus
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
                        guard let data = try? await item.loadTransferable(type: Data.self) else { return unreadable() }
                        add(ext: ext) { data }
                    }
                }
            }
            .fullScreenCover(isPresented: $takingPhoto) {
                CameraPicker { image in add(ext: "jpg") { image.jpegData(compressionQuality: 0.85) } }.ignoresSafeArea()
            }
            .fileImporter(isPresented: $choosingFiles, allowedContentTypes: [.image], allowsMultipleSelection: true) { result in
                guard case .success(let urls) = result else { return unreadable() }
                for url in urls {
                    add(ext: url.pathExtension.isEmpty ? "jpg" : url.pathExtension) {
                        let scoped = url.startAccessingSecurityScopedResource()
                        defer { if scoped { url.stopAccessingSecurityScopedResource() } }
                        return try? Data(contentsOf: url)
                    }
                }
            }
        #else
            plus
        #endif
    }

    @ViewBuilder
    private var plus: some View {
        if circled {
            LucideIcon(.plus, size: 17).foregroundStyle(Palette.text.color).frame(width: 34, height: 34)
                .background(Palette.fill3.color, in: Circle()).hitTarget()
        } else {
            LucideIcon(.plus, size: 16).foregroundStyle(Palette.text2.color).hitTarget()
        }
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
        Self.add(ext: ext, load, to: attach, unreadable: unreadable)
    }

    static func add(ext: String, _ load: @escaping @Sendable () -> Data?, to attach: @escaping Attach, unreadable: @escaping () -> Void = {}) {
        let name = "\(UUID().uuidString.prefix(8)).\(ext)"
        Task {
            let loaded = await Task.detached(priority: .userInitiated) { () -> (Data, Data?)? in
                guard let data = load() else { return nil }
                return (data, Thumbnail.make(data))
            }.value
            guard let (data, thumbnail) = loaded else { return unreadable() }
            attach(data, name, thumbnail)
        }
    }
}
