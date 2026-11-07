import Stores

#if os(iOS)
    import UIKit
#endif

/// Work the phone keeps its link up for when it locks or leaves the app mid-way, as long as iOS allows.
@MainActor
enum Background {
    static func keep(_ name: String, _ work: @MainActor () async -> Void) async {
        #if os(iOS)
            let held = Held()
            held.task = UIApplication.shared.beginBackgroundTask(withName: name) { held.end() }
            await work()
            held.end()
        #else
            await work()
        #endif
    }

    #if os(iOS)
        @MainActor private final class Held {
            var task = UIBackgroundTaskIdentifier.invalid

            func end() {
                guard task != .invalid else { return }
                UIApplication.shared.endBackgroundTask(task)
                task = .invalid
            }
        }
    #endif
}

extension WorkspaceStore {
    /// Runs what is left of a launch, keeping the link up if the phone locks meanwhile.
    func runKept(_ requestId: String) {
        Task { await Background.keep("New session") { await run(requestId) } }
    }
}
