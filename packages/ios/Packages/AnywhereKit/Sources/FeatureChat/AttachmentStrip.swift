import DesignSystem
import Stores
import SwiftUI

/// The draft's images as 56 pt tiles, each with a × to remove; one still uploading turns, one that failed is red and
/// says why under the tiles, with Retry and Remove. `locked` while an armed send waits on them.
@MainActor
struct AttachmentStrip: View {
    let images: [ChatStore.Attachment]
    let locked: Bool
    let detach: (String) -> Void
    let retry: (String) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: Space.xs) {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 6) {
                    ForEach(images) { image in
                        thumbnail(image)
                            .frame(width: 56, height: 56)
                            .clipShape(RoundedRectangle(cornerRadius: 8))
                            .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder((image.failed ? Palette.failed : Palette.hairline).color))
                            .overlay {
                                if image.path == nil && !image.failed { ProgressView().controlSize(.small) }
                                if image.failed {
                                    Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(.white, Palette.failed.color)
                                }
                            }
                            .overlay(alignment: .topTrailing) {
                                if !locked {
                                    Button {
                                        detach(image.id)
                                    } label: {
                                        Image(systemName: "xmark.circle.fill").font(.system(size: 16)).foregroundStyle(.white, .black.opacity(0.6))
                                            .padding(2)
                                            .frame(width: HitTarget.minimum, height: HitTarget.minimum, alignment: .topTrailing)
                                            .contentShape(Rectangle())
                                    }
                                    .buttonStyle(.plain)
                                    .accessibilityLabel("Remove \(image.name)")
                                }
                            }
                    }
                }
            }
            if let failed = images.first(where: \.failed) { failure(failed) }
        }
    }

    private func failure(_ image: ChatStore.Attachment) -> some View {
        HStack(spacing: Space.s) {
            Text(image.reason ?? "This image didn't upload").foregroundStyle(Palette.failedText.color).lineLimit(2)
            Spacer(minLength: 0)
            Button { retry(image.id) } label: { Text("Retry").hitTarget() }
            if !locked { Button { detach(image.id) } label: { Text("Remove").hitTarget() } }
        }
        .font(ChatFont.caption)
        .buttonStyle(.plain)
        .foregroundStyle(Palette.text.color)
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
