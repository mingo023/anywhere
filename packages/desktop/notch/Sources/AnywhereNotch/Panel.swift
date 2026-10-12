import AppKit

/// Borderless, as AppKit keeps titled windows below the menu bar; non-activating, so a click types into it without taking the frontmost app's place.
final class Panel: NSPanel {
    /// Room for the open island; its clear rest lets clicks through to the menu bar.
    static let size = CGSize(width: 520, height: 300)

    init() {
        super.init(contentRect: CGRect(origin: .zero, size: Self.size), styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: false)
        isFloatingPanel = true
        level = .popUpMenu
        collectionBehavior = [.canJoinAllSpaces, .stationary, .ignoresCycle, .fullScreenAuxiliary]
        hidesOnDeactivate = false
        isReleasedWhenClosed = false
        isOpaque = false
        backgroundColor = .clear
        hasShadow = false
        // The island is black in light mode too, so its field and placeholder take dark's colours.
        appearance = NSAppearance(named: .darkAqua)
    }

    override var canBecomeKey: Bool { true }

    /// Hangs the panel from the top of the screen, centred on `notch`.
    func place(_ notch: Notch) {
        setFrameTopLeftPoint(CGPoint(x: notch.x - Self.size.width / 2, y: notch.top))
    }
}
