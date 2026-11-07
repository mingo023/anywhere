import DesignSystem
import SwiftUI
#if os(iOS)
    import PhotosUI
    import UniformTypeIdentifiers
#endif

/// The composer's +: an image from Photos, the camera, Files or the clipboard. `attach` takes its data, a file name,
/// and a thumbnail made off the main actor.
@MainActor
struct AttachMenu: View {
    let enabled: Bool
    let attach: (_ data: Data, _ name: String, _ thumbnail: Data?) -> Void
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
                        if let data = UIPasteboard.general.image?.pngData() { add(data, ext: "png") }
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
                        add(data, ext: ext)
                    }
                }
            }
            .fullScreenCover(isPresented: $takingPhoto) {
                CameraPicker { add($0, ext: "jpg") }.ignoresSafeArea()
            }
            .fileImporter(isPresented: $choosingFiles, allowedContentTypes: [.image], allowsMultipleSelection: true) { result in
                guard case .success(let urls) = result else { return }
                for url in urls {
                    let scoped = url.startAccessingSecurityScopedResource()
                    defer { if scoped { url.stopAccessingSecurityScopedResource() } }
                    if let data = try? Data(contentsOf: url) { add(data, ext: url.pathExtension.isEmpty ? "jpg" : url.pathExtension) }
                }
            }
        #else
            EmptyView()
        #endif
    }

    private func add(_ data: Data, ext: String) {
        let name = "\(UUID().uuidString.prefix(8)).\(ext)"
        Task {
            let thumbnail = await Task.detached(priority: .userInitiated) { Thumbnail.make(data) }.value
            attach(data, name, thumbnail)
        }
    }
}
