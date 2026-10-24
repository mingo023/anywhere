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
            RootView(app: delegate.app)
        }
    }
}

struct AnywhereIntents: AppIntentsPackage {
    static var includedPackages: [any AppIntentsPackage.Type] { [LiveActivityIntents.self] }
}
