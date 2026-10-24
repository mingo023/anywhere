#if os(iOS) && canImport(ActivityKit)
    import ActivityKit
    import ActivityModel
    import Foundation

    /// ActivityKit's side of the Live Activity: tokens to the Macs, ends back to them, and starting one when a Mac turns busy.
    @MainActor
    public enum LiveActivities {
        private static var watched: Set<String> = []

        public static func install(_ center: PushCenter) {
            center.startActivity = { attributes, state in start(attributes, state, center) }
            center.endActivities = {
                Task {
                    for activity in Activity<MacActivity>.activities { await activity.end(nil, dismissalPolicy: .immediate) }
                }
            }
            let info = ActivityAuthorizationInfo()
            center.frequent(info.frequentPushesEnabled)
            Task { for await on in info.frequentPushEnablementUpdates { center.frequent(on) } }
            Task { for await token in Activity<MacActivity>.pushToStartTokenUpdates { center.received(startToken: token) } }
            Task { for await activity in Activity<MacActivity>.activityUpdates { watch(activity, center) } }
            for activity in Activity<MacActivity>.activities { watch(activity, center) }
        }

        /// One activity per Mac; ActivityKit refuses a start while the app is in the background, which is when pocketd starts it by push instead.
        static func start(_ attributes: MacActivity, _ state: MacActivity.ContentState, _ center: PushCenter) {
            guard center.liveActivities, ActivityAuthorizationInfo().areActivitiesEnabled,
                !Activity<MacActivity>.activities.contains(where: { $0.attributes.mac == attributes.mac && $0.activityState == .active })
            else { return }
            let content = ActivityContent(state: state, staleDate: Date().addingTimeInterval(300), relevanceScore: state.relevance)
            _ = try? Activity.request(attributes: attributes, content: content, pushType: .token)
        }

        private static func watch(_ activity: Activity<MacActivity>, _ center: PushCenter) {
            guard watched.insert(activity.id).inserted else { return }
            let mac = activity.attributes.mac
            Task {
                for await token in activity.pushTokenUpdates { center.activity(mac: mac, id: activity.id, token: token) }
            }
            Task {
                for await state in activity.activityStateUpdates where state == .ended || state == .dismissed {
                    center.activityEnded(mac: mac, id: activity.id)
                    watched.remove(activity.id)
                    return
                }
            }
        }
    }
#endif
