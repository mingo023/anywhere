import AppKit
import DesignSystem
import SwiftUI

/// Draws a SwiftUI scene offscreen to PNG at 2× scale.
@MainActor
enum GalleryRenderer {
    struct Image {
        let data: Data
        /// Variance of the pixels' luminance, 0...~6500: near 0 means a blank image.
        let variance: Double
    }

    static func render(_ scene: GalleryScene, settle: TimeInterval = 0.4) -> Image? {
        let appearance = NSAppearance(named: scene.dark ? .darkAqua : .aqua)
        let root = scene.view()
            .environment(\.colorScheme, scene.dark ? .dark : .light)
            // Drawn as in a focused window, so prominent buttons show their fill.
            .environment(\.controlActiveState, .key)
            .frame(width: scene.size.width, height: scene.size.height)
        let view = NSHostingView(rootView: root)
        let rect = NSRect(origin: .zero, size: scene.size)
        view.frame = rect
        view.appearance = appearance
        let window = NSWindow(contentRect: rect, styleMask: [.borderless], backing: .buffered, defer: false)
        window.appearance = appearance
        window.isReleasedWhenClosed = false
        window.contentView = view
        // Offscreen, but ordered in so SwiftUI treats the view as visible and loads lazy content.
        window.setFrameOrigin(NSPoint(x: -10_000 - rect.width, y: -10_000))
        window.orderFrontRegardless()
        defer { window.orderOut(nil) }

        let deadline = Date().addingTimeInterval(settle)
        while Date() < deadline {
            view.layoutSubtreeIfNeeded()
            RunLoop.main.run(until: Date().addingTimeInterval(0.02))
        }
        view.layoutSubtreeIfNeeded()

        let scale: CGFloat = 2
        guard let rep = NSBitmapImageRep(
            bitmapDataPlanes: nil, pixelsWide: Int(rect.width * scale), pixelsHigh: Int(rect.height * scale), bitsPerSample: 8,
            samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)
        else { return nil }
        rep.size = rect.size
        var drawn = false
        (appearance ?? NSAppearance.currentDrawing()).performAsCurrentDrawingAppearance {
            view.cacheDisplay(in: view.bounds, to: rep)
            drawn = true
        }
        guard drawn, let png = rep.representation(using: .png, properties: [:]) else { return nil }
        return Image(data: png, variance: variance(rep))
    }

    /// Luminance variance over a grid of samples.
    static func variance(_ rep: NSBitmapImageRep) -> Double {
        var values: [Double] = []
        let step = 4
        for y in stride(from: 0, to: rep.pixelsHigh, by: step) {
            for x in stride(from: 0, to: rep.pixelsWide, by: step) {
                guard let c = rep.colorAt(x: x, y: y)?.usingColorSpace(.deviceRGB) else { continue }
                values.append(255 * (0.299 * c.redComponent + 0.587 * c.greenComponent + 0.114 * c.blueComponent))
            }
        }
        guard !values.isEmpty else { return 0 }
        let mean = values.reduce(0, +) / Double(values.count)
        return values.reduce(0) { $0 + ($1 - mean) * ($1 - mean) } / Double(values.count)
    }
}
