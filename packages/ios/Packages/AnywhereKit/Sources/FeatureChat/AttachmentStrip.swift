import DesignSystem
import Stores
import SwiftUI

/// The draft's images as 56 pt tiles, each with a × to remove; one still uploading turns, one that failed is red.
@MainActor
struct AttachmentStrip: View {
    let images: [ChatStore.Attachment]
    let detach: (String) -> Void

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 6) {
                ForEach(images) { image in
                    thumbnail(image)
                        .frame(width: 56, height: 56)
                        .clipShape(RoundedRectangle(cornerRadius: 8))
                        .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder((image.failed ? Palette.failed : Palette.hairline).color))
                        .overlay {
                            if image.path == nil && !image.failed { ProgressView().controlSize(.small) }
                        }
                        .overlay(alignment: .topTrailing) {
                            Button {
                                detach(image.id)
                            } label: {
                                Image(systemName: "xmark.circle.fill").font(.system(size: 16)).foregroundStyle(.white, .black.opacity(0.6))
                            }
                            .buttonStyle(.plain)
                            .padding(2)
                            .accessibilityLabel("Remove \(image.name)")
                        }
                }
            }
        }
    }

    @ViewBuilder
    private func thumbnail(_ image: ChatStore.Attachment) -> some View {
        if let data = image.thumbnail, let picture = Self.image(data) {
            picture.resizable().scaledToFill()
        } else {
            Image(systemName: "photo").foregroundStyle(Palette.text3.color).frame(maxWidth: .infinity, maxHeight: .infinity)
                .background(Palette.fill3.color)
        }
    }

    static func image(_ data: Data) -> Image? {
        #if os(iOS)
            UIImage(data: data).map { Image(uiImage: $0) }
        #else
            NSImage(data: data).map { Image(nsImage: $0) }
        #endif
    }
}
