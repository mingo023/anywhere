import AppShell
import Client
import Stores
import SwiftUI

@main
struct AnywhereApp: App {
    @UIApplicationDelegateAdaptor(AppDelegate.self) private var delegate
    @State private var app = AppModel.restore(from: CredentialsStore(keychain: Keychain(service: CredentialsStore.service, accessGroup: Keychain.appGroup)))

    var body: some Scene {
        WindowGroup {
            RootView(app: app)
        }
    }
}
