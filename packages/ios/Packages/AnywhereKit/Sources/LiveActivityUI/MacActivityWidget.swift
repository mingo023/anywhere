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
                HStack {
                    Label(context.attributes.macName, systemImage: "laptopcomputer")
                    Spacer()
                    Text(Self.summary(context.state))
                }
                .padding()
                .widgetURL(URL(string: "anywhere://mac/\(context.attributes.mac)"))
            } dynamicIsland: { context in
                DynamicIsland {
                    DynamicIslandExpandedRegion(.leading) { Label(context.attributes.macName, systemImage: "laptopcomputer") }
                    DynamicIslandExpandedRegion(.trailing) { Text(Self.summary(context.state)) }
                } compactLeading: {
                    Image(systemName: "laptopcomputer")
                } compactTrailing: {
                    Text("\(context.state.needsYou + context.state.working)")
                } minimal: {
                    Image(systemName: "laptopcomputer")
                }
            }
        }

        static func summary(_ state: MacActivity.ContentState) -> String {
            state.needsYou > 0 ? "\(state.needsYou) need you" : "\(state.working) working"
        }
    }
#endif
