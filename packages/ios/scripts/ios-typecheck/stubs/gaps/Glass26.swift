// iOS 26 SwiftUI API missing from the iOS 17.2 SDK, declared as developer.apple.com lists it.
import SwiftUI

public struct Glass: Equatable, Sendable {
    public static var regular: Glass { Glass() }
    public static var clear: Glass { Glass() }
    public func interactive(_ isEnabled: Bool = true) -> Glass { self }
    public func tint(_ color: Color?) -> Glass { self }
}

public struct DefaultGlassEffectShape: Shape {
    public init() {}
    public func path(in rect: CGRect) -> Path { Path(rect) }
}

extension View {
    public nonisolated func glassEffect(_ glass: Glass = .regular, in shape: some Shape = DefaultGlassEffectShape()) -> some View { self }
}
