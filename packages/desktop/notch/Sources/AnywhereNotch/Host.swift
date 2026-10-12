import AppKit
import SwiftUI

/// Takes the click that wakes it, as the panel's app is never the active one.
final class Host: NSHostingView<IslandView> {
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }
}
