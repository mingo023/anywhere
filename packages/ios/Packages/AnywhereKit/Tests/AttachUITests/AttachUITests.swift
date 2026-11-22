import CoreGraphics
import Foundation
import ImageIO
import Testing
import UniformTypeIdentifiers
@testable import AttachUI

@Test("an attached photo's thumbnail is 112 px on its long side, and data that isn't an image has none")
func thumbnails() throws {
    let context = try #require(CGContext(data: nil, width: 1200, height: 800, bitsPerComponent: 8, bytesPerRow: 0, space: CGColorSpaceCreateDeviceRGB(),
                                         bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
    let photo = NSMutableData()
    let destination = try #require(CGImageDestinationCreateWithData(photo, UTType.png.identifier as CFString, 1, nil))
    CGImageDestinationAddImage(destination, try #require(context.makeImage()), nil)
    #expect(CGImageDestinationFinalize(destination))
    let thumbnail = try #require(Thumbnail.make(photo as Data))
    let image = try #require(CGImageSourceCreateWithData(thumbnail as CFData, nil).flatMap { CGImageSourceCreateImageAtIndex($0, 0, nil) })
    #expect(image.width == 112 && image.height == 75)
    #expect(Thumbnail.make(Data("not an image".utf8)) == nil)
}
