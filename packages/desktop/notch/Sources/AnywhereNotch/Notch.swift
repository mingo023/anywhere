import AppKit

struct Notch: Equatable {
    /// The notch's centre and the screen's top, in AppKit's coordinates.
    var x: CGFloat
    var top: CGFloat
    var width: CGFloat
    var height: CGFloat

    /// Under the notch of the screen with one; else the top centre of the main screen, as tall as its menu bar.
    static func find() -> Notch? {
        let notched = NSScreen.screens.first { $0.safeAreaInsets.top > 0 }
        guard let screen = notched ?? NSScreen.screens.first else { return nil }
        let f = screen.frame
        if let s = notched {
            let (left, right) = (s.auxiliaryTopLeftArea?.width ?? 0, s.auxiliaryTopRightArea?.width ?? 0)
            let width = f.width - left - right
            return Notch(x: f.minX + left + width / 2, top: f.maxY, width: width, height: s.safeAreaInsets.top)
        }
        return Notch(x: f.midX, top: f.maxY, width: 0, height: max(f.maxY - screen.visibleFrame.maxY, 24))
    }
}
