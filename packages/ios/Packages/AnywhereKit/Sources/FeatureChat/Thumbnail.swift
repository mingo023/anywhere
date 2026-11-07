import Foundation
import ImageIO
import UniformTypeIdentifiers

/// The small JPEG an attachment tile shows and a saved draft keeps, so neither decodes the whole photo.
enum Thumbnail {
    static let side = 112

    /// `data`'s image scaled to `side` px on its long edge, upright; nil for data that isn't an image.
    nonisolated static func make(_ data: Data, side: Int = side) -> Data? {
        let options = [
            kCGImageSourceCreateThumbnailFromImageAlways: true, kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceThumbnailMaxPixelSize: side,
        ] as CFDictionary
        guard let source = CGImageSourceCreateWithData(data as CFData, nil), let image = CGImageSourceCreateThumbnailAtIndex(source, 0, options) else {
            return nil
        }
        let out = NSMutableData()
        guard let destination = CGImageDestinationCreateWithData(out, UTType.jpeg.identifier as CFString, 1, nil) else { return nil }
        CGImageDestinationAddImage(destination, image, [kCGImageDestinationLossyCompressionQuality: 0.8] as CFDictionary)
        return CGImageDestinationFinalize(destination) ? out as Data : nil
    }
}
