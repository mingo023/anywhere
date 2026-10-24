import AppShell
import SwiftUI

@main
struct AnywhereApp: App {
    @UIApplicationDelegateAdaptor(AppDelegate.self) private var delegate

    var body: some Scene {
        WindowGroup {
            RootView(app: delegate.app)
        }
    }
}
