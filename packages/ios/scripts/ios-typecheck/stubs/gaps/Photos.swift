// PhotosUI's SwiftUI overlay doesn't load against the swapped-in stdlib; declarations as developer.apple.com lists them.
import CoreTransferable
import SwiftUI
import UniformTypeIdentifiers

public struct PhotosPickerItem: Equatable, Hashable, Sendable {
    public var supportedContentTypes: [UTType] { [] }
    public func loadTransferable<T: Transferable>(type: T.Type) async throws -> T? { nil }
}

public struct PHPickerFilterShim: Sendable {
    public static var images: PHPickerFilterShim { .init() }
}

public struct PhotosPicker<Label: View>: View {
    public init(selection: Binding<[PhotosPickerItem]>, maxSelectionCount: Int? = nil, matching filter: PHPickerFilterShim? = nil,
                @ViewBuilder label: () -> Label) {}
    public var body: some View { EmptyView() }
}
