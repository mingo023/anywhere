import DesignSystem
import Stores
import SwiftUI

/// The draft's images as 56 pt tiles, each with a × to remove; one still uploading turns, one that failed is red and
/// says why under the tiles, with Retry and Remove. `locked` while an armed send waits on them.
@MainActor
public struct AttachmentStrip: View {
    let images: [ChatStore.Attachment]
    let locked: Bool
    let detach: (String) -> Void
    let retry: (String) -> Void

    public init(images: [ChatStore.Attachment], locked: Bool, detach: @escaping (String) -> Void, retry: @escaping (String) -> Void) {
        self.images = images
        self.locked = locked
        self.detach = detach
        self.retry = retry
    }

    public var body: some View {
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
                                    LucideIcon(.triangleAlert, size: 18).foregroundStyle(Palette.failed.color)
                                }
                            }
                            .overlay(alignment: .topTrailing) {
                                if !locked {
                                    Button {
                                        detach(image.id)
                                    } label: {
                                        LucideIcon(.close, size: 10, weight: 2.5).foregroundStyle(.white).frame(width: 16, height: 16).background(.black.opacity(0.6), in: Circle())
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
        .font(.system(size: 12))
        .buttonStyle(.plain)
        .foregroundStyle(Palette.text.color)
    }

    @ViewBuilder
    private func thumbnail(_ image: ChatStore.Attachment) -> some View {
        if let data = image.thumbnail, let picture = Self.image(data) {
            picture.resizable().scaledToFill()
        } else {
            LucideIcon(.imagePlus, size: 20).foregroundStyle(Palette.text3.color).frame(maxWidth: .infinity, maxHeight: .infinity)
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
