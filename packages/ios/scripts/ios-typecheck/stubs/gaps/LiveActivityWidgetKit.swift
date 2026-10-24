// WidgetKit's Live Activity API (iOS-only, absent from Mac Catalyst), declared as developer.apple.com lists it.
import ActivityKit
import SwiftUI
import WidgetKit

public struct ActivityViewContext<Attributes> where Attributes: ActivityAttributes {
    public var attributes: Attributes { fatalError() }
    public var state: Attributes.ContentState { fatalError() }
    public var isStale: Bool { false }
    public var activityID: String { "" }
}

public struct ActivityConfiguration<Attributes>: WidgetConfiguration, Sendable where Attributes: ActivityAttributes {
    @MainActor @preconcurrency
    public init<Content>(for attributesType: Attributes.Type, @ViewBuilder content: @escaping (ActivityViewContext<Attributes>) -> Content,
                         dynamicIsland: @escaping (ActivityViewContext<Attributes>) -> DynamicIsland) where Content: View {}
    public var body: some WidgetConfiguration { EmptyWidgetConfiguration() }
}

public struct DynamicIslandExpandedRegionPosition {
    public static var leading: Self { .init() }
    public static var trailing: Self { .init() }
    public static var center: Self { .init() }
    public static var bottom: Self { .init() }
}

public struct DynamicIslandExpandedRegion<Content> where Content: View {
    public init(_ position: DynamicIslandExpandedRegionPosition, priority: Double = 0, @ViewBuilder content: () -> Content) {}
}

public struct DynamicIslandExpandedContent<Content> where Content: View {}

@resultBuilder
public enum DynamicIslandExpandedContentBuilder {
    public static func buildBlock<each C: View>(_ regions: repeat DynamicIslandExpandedRegion<each C>) -> DynamicIslandExpandedContent<EmptyView> { .init() }
}

public struct DynamicIsland {
    public init<Expanded, CompactLeading, CompactTrailing, Minimal>(
        @DynamicIslandExpandedContentBuilder expanded: @escaping () -> DynamicIslandExpandedContent<Expanded>,
        @ViewBuilder compactLeading: @escaping () -> CompactLeading, @ViewBuilder compactTrailing: @escaping () -> CompactTrailing,
        @ViewBuilder minimal: @escaping () -> Minimal
    ) where Expanded: View, CompactLeading: View, CompactTrailing: View, Minimal: View {}
    public func widgetURL(_ url: URL?) -> DynamicIsland { self }
    public func keylineTint(_ color: Color?) -> DynamicIsland { self }
}

extension View {
    @MainActor @preconcurrency
    public func activitySystemActionForegroundColor(_ color: Color?) -> some View { self }
}
