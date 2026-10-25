import SwiftUI

/// One screen state the macOS gallery renders to PNG for review. Each feature lists its own in `<Feature>Gallery.scenes`.
public struct GalleryScene: Identifiable {
    public var id: String { name }
    public let name: String
    public let size: CGSize
    public let dark: Bool
    public let view: @MainActor () -> AnyView

    public init(_ name: String, size: CGSize = CGSize(width: 393, height: 852), dark: Bool = false, view: @escaping @MainActor () -> AnyView) {
        self.name = name
        self.size = size
        self.dark = dark
        self.view = view
    }
}
