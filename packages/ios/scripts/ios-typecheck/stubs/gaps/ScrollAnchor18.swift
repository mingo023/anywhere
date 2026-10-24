// iOS 18's scroll anchor roles; declarations as developer.apple.com lists them.
import SwiftUI

public struct ScrollAnchorRole: Hashable, Sendable {
    public static var initialOffset: ScrollAnchorRole { .init() }
    public static var sizeChanges: ScrollAnchorRole { .init() }
    public static var alignment: ScrollAnchorRole { .init() }
}

extension View {
    public func defaultScrollAnchor(_ anchor: UnitPoint?, for role: ScrollAnchorRole) -> some View { self }
}
