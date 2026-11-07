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
            center.renewActivity = { attributes, state in start(attributes, state, center, renewing: true) }
            center.endActivities = {
                Task {
                    for activity in Activity<MacActivity>.activities { await activity.end(nil, dismissalPolicy: .immediate) }
                }
            }
            let info = ActivityAuthorizationInfo()
            center.frequent(info.frequentPushesEnabled)
            Task { for await on in info.frequentPushEnablementUpdates { center.frequent(on) } }
            Task { for await token in Activity<MacActivity>.pushToStartTokenUpdates { center.received(startToken: token) } }
            Task { for await _ in Activity<MacActivity>.activityUpdates { settle(center) } }
            settle(center)
        }

        /// One activity per Mac; ActivityKit refuses a start while the app is in the background, which is when pocketd starts it by push instead.
        static func start(_ attributes: MacActivity, _ state: MacActivity.ContentState, _ center: PushCenter, renewing: Bool = false) {
            guard center.liveActivities, !starting.contains(attributes.mac) else { return }
            starting.insert(attributes.mac)
            // The checks and the request are synchronous calls to the system, ~0.7 s on the main thread.
            Task.detached(priority: .userInitiated) {
                let mine = Activity<MacActivity>.activities.filter { $0.attributes.mac == attributes.mac && shown($0) }
                if ActivityAuthorizationInfo().areActivitiesEnabled,
                    let ended = MacActivity.replaced(mine.map { (id: $0.id, stale: $0.activityState == .stale) }, renewing: renewing)
                {
                    for activity in mine where ended.contains(activity.id) { await activity.end(nil, dismissalPolicy: .immediate) }
                    let content = ActivityContent(state: state, staleDate: Date().addingTimeInterval(300), relevanceScore: state.relevance)
                    _ = try? Activity.request(attributes: attributes, content: content, pushType: .token)
                }
                await MainActor.run { _ = starting.remove(attributes.mac) }
            }
        }

        private static var starting: Set<String> = []

        /// A stale activity is still on screen, saying its Mac is offline until pocketd, given its token, updates it.
        nonisolated private static func shown(_ activity: Activity<MacActivity>) -> Bool {
            activity.activityState == .active || activity.activityState == .stale
        }

        /// Ends all but one shown activity per Mac before watching the rest, so the token pocketd gets is the one left's.
        private static func settle(_ center: PushCenter) {
            let all = Activity<MacActivity>.activities
            let held = Set(center.activities.values.map(\.id))
            let extras = MacActivity.extras(
                all.filter(shown).map { (id: $0.id, mac: $0.attributes.mac, stale: $0.activityState == .stale, staleDate: $0.content.staleDate) }, held: held)
            for activity in all where !extras.contains(activity.id) { watch(activity, center) }
            guard !extras.isEmpty else { return }
            Task {
                for activity in Activity<MacActivity>.activities where extras.contains(activity.id) { await activity.end(nil, dismissalPolicy: .immediate) }
            }
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
