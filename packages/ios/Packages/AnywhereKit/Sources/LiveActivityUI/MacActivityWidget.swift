#if os(iOS) && canImport(ActivityKit)
    import ActivityKit
    import ActivityModel
    import DesignSystem
    import SwiftUI
    import WidgetKit

    /// The Lock Screen and Dynamic Island for one Mac's sessions.
    public struct MacActivityWidget: Widget {
        public init() {}

        public var body: some WidgetConfiguration {
            ActivityConfiguration(for: MacActivity.self) { context in
                let s = Self.snapshot(context)
                LockScreenActivity(s)
                    .activitySystemActionForegroundColor(.primary)
                    .widgetURL(s.link)
            } dynamicIsland: { context in
                let s = Self.snapshot(context)
                return DynamicIsland {
                    DynamicIslandExpandedRegion(.leading) { IslandRegion.expandedLeading(s).padding(.leading, 4) }
                    DynamicIslandExpandedRegion(.trailing) { IslandRegion.expandedTrailing(s).padding(.trailing, 4) }
                    DynamicIslandExpandedRegion(.center) { IslandRegion.expandedCenter(s) }
                    DynamicIslandExpandedRegion(.bottom) { IslandRegion.expandedBottom(s) }
                } compactLeading: {
                    IslandRegion.compactLeading(s)
                } compactTrailing: {
                    IslandRegion.compactTrailing(s)
                } minimal: {
                    IslandRegion.minimal(s)
                }
                .widgetURL(s.link)
                .keylineTint(Color.of(ActivityGlyph.tint(ActivityDisplay.glyph(s.state)), dark: true))
            }
        }

        static func snapshot(_ context: ActivityViewContext<MacActivity>) -> ActivitySnapshot {
            let titles = UserDefaults(suiteName: ActivityTitles.appGroup).map(ActivityTitles.init(defaults:))
            let entry = titles?.entry(mac: context.attributes.mac, slot: context.state.top?.slot) ?? ActivityTitles.fallback
            return ActivitySnapshot(attributes: context.attributes, state: context.state, isStale: context.isStale, entry: entry)
        }
    }
#endif
