import Domain
import SwiftUI

/// The dot beside a Mac's name: green online, amber while redialing, red when it can't be used, grey once stopped.
public struct LinkDot: View {
    let tone: LinkTone

    public init(_ tone: LinkTone) {
        self.tone = tone
    }

    public var body: some View {
        Circle().fill(Self.color(tone).color).frame(width: 7, height: 7)
    }

    public nonisolated static func color(_ tone: LinkTone) -> Token {
        switch tone {
        case .up: Palette.success
        case .trying: Palette.waitingDot
        case .lost: Palette.failed
        case .off: Palette.text4
        }
    }
}
