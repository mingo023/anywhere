import AppIntents
import AppShell
import LiveActivityUI
import SwiftUI

@main
@MainActor
struct AnywhereApp: App {
    @UIApplicationDelegateAdaptor(AppDelegate.self) private var delegate

    var body: some Scene {
        WindowGroup {
            #if DEBUG
                if let scenario = BenchRun.scenario {
                    BenchView(scenario: scenario)
                } else {
                    RootView(app: delegate.app)
                }
            #else
                RootView(app: delegate.app)
            #endif
        }
    }
}

struct AnywhereIntents: AppIntentsPackage {
    static var includedPackages: [any AppIntentsPackage.Type] { [LiveActivityIntents.self] }
}
