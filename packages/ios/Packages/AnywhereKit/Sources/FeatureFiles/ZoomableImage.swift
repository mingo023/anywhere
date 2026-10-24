import DesignSystem
import Foundation
import ImageIO
import SwiftUI

/// An image decoded off the main actor, fit to the screen; pinch or double-tap to zoom, drag to pan while zoomed.
@MainActor
struct ZoomableImage: View {
    let data: Data
    let name: String

    @State private var image: CGImage?
    @State private var failed = false
    @State private var scale: CGFloat = 1
    @State private var settled: CGFloat = 1
    @State private var offset: CGSize = .zero
    @State private var dragged: CGSize = .zero

    nonisolated static let maxScale: CGFloat = 6
    /// The longest side decoded; bigger images are downsampled so a 20 MB photo doesn't take 300 MB.
    nonisolated static let maxPixels = 4096

    var body: some View {
        GeometryReader { geo in
            Group {
                if let image {
                    Image(decorative: image, scale: 1)
                        .resizable()
                        .interpolation(.high)
                        .aspectRatio(contentMode: .fit)
                        .accessibilityLabel(name)
                        .scaleEffect(scale)
                        .offset(x: offset.width + dragged.width, y: offset.height + dragged.height)
                        .gesture(magnify.simultaneously(with: pan))
                        .onTapGesture(count: 2) { toggleZoom() }
                } else if failed {
                    Placeholder("This image can't be shown.", systemImage: "photo")
                } else {
                    ProgressView()
                }
            }
            .frame(width: geo.size.width, height: geo.size.height)
        }
        .background(Palette.background.color)
        .clipped()
        .task(id: data) {
            let data = data
            let decoded = await Task.detached(priority: .userInitiated) { Self.decode(data, maxPixels: Self.maxPixels) }.value
            image = decoded
            failed = decoded == nil
        }
    }

    private var magnify: some Gesture {
        MagnifyGesture()
            .onChanged { scale = Self.clamp(settled * $0.magnification) }
            .onEnded { _ in
                settled = scale
                if scale <= 1 { reset() }
            }
    }

    private var pan: some Gesture {
        DragGesture()
            .onChanged { if scale > 1 { dragged = $0.translation } }
            .onEnded { _ in
                offset.width += dragged.width
                offset.height += dragged.height
                dragged = .zero
            }
    }

    private func toggleZoom() {
        withAnimation(.spring(duration: 0.3)) {
            if scale > 1 {
                reset()
            } else {
                scale = 2.5
                settled = 2.5
            }
        }
        Haptics.tap()
    }

    private func reset() {
        withAnimation(.spring(duration: 0.3)) {
            scale = 1
            settled = 1
            offset = .zero
        }
    }

    nonisolated static func clamp(_ scale: CGFloat) -> CGFloat { min(max(scale, 0.8), maxScale) }

    /// The image, at most `maxPixels` on its longest side, decoded now rather than on first draw.
    nonisolated static func decode(_ data: Data, maxPixels: Int) -> CGImage? {
        guard let source = CGImageSourceCreateWithData(data as CFData, nil) else { return nil }
        let options: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceShouldCacheImmediately: true,
            kCGImageSourceThumbnailMaxPixelSize: maxPixels,
        ]
        return CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary)
    }
}
