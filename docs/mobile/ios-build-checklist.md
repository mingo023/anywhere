# First Xcode build of the iOS app

Built with Xcode 27.1 for the iOS Simulator and for devices (unsigned), all three targets, with no warnings in our code. `make -C packages/ios typecheck-ios` typechecks all of the iOS-only code as Mac Catalyst against the Command Line Tools SDK (iOS 17.2-era), with ActivityKit and the newer API stubbed from Apple's docs. It passes, but it can't stand in for a build. These are the steps for the first build, then what is still unverified.

## Needs

- **Xcode 26 or later.** `Package.swift` is `swift-tools-version: 6.2`, and Xcode 16.x ships Swift 6.1, so it can't open the package. With Xcode 26 the iOS 26 SDK is there too, so `Glass.swift` takes the `glassEffect(.regular, in:)` branch. Its signature matches the docs: `glassEffect(_ glass: Glass = .regular, in shape: some Shape = DefaultGlassEffectShape())`.
- A paid Apple Developer team. Push, App Groups and Time Sensitive Notifications need one, and a Personal Team can't do push.
- An iPhone on iOS 18 or later. The simulator runs the UI and Live Activities, but it has no camera for the QR code and gets no real APNs pushes.

## Steps

1. Generate the project:
   ```sh
   brew install xcodegen
   make -C packages/ios gen        # writes packages/ios/Anywhere.xcodeproj (not committed)
   open packages/ios/Anywhere.xcodeproj
   ```
2. **Packages.** Xcode resolves `swift-cmark` (branch `gfm`) and `SwiftTerm` (exactly 1.20.0) on its own; it doesn't read `Packages/AnywhereKit/Package.resolved`. On the first build Xcode asks you to trust **SwiftTermBuildInfoPlugin**, SwiftTerm's build-tool plugin: choose *Trust & Enable*. From the command line, pass `-skipPackagePluginValidation` to `xcodebuild`. SwiftTerm also compiles Metal shaders: on a fresh Xcode, run `xcodebuild -downloadComponent MetalToolchain` once.
3. **Signing.** Under *Signing & Capabilities*, set your team on all three targets: `Anywhere`, `AnywhereWidgets` and `AnywhereNotify`. Leave "Automatically manage signing" on. `make gen` rewrites the project, so set the team again after each run, or pass it on the command line: `xcodebuild -scheme Anywhere DEVELOPMENT_TEAM=<TEAMID> -allowProvisioningUpdates …`.
4. **Bundle IDs.** These are `dev.mingo.anywhere`, `dev.mingo.anywhere.widgets` and `dev.mingo.anywhere.notify`. If your team can't register them, change all of these together:
   - `bundleIdPrefix` and the three `PRODUCT_BUNDLE_IDENTIFIER`s in `packages/ios/project.yml`.
   - The App Group `group.dev.mingo.anywhere`, which appears in the three `.entitlements` files, in `Keychain.appGroup` (`Sources/Client/Keychain.swift`) and in `ActivityTitles.appGroup` (`Sources/ActivityModel/ActivityDisplay.swift`).
   - The relay's `BUNDLE_ID`, since the APNs topic is the bundle id.
5. **Capabilities.** Automatic signing creates the App IDs from the entitlements files, so check that each of these ended up on:

   | Target | Capability | Entitlement |
   |---|---|---|
   | Anywhere | Push Notifications | `aps-environment` = development (Xcode switches it to production when it exports for TestFlight or the App Store) |
   | Anywhere | Time Sensitive Notifications | `com.apple.developer.usernotifications.time-sensitive` |
   | all three | App Groups: `group.dev.mingo.anywhere` | `com.apple.security.application-groups` |
   | all three | Keychain Sharing: `$(AppIdentifierPrefix)dev.mingo.anywhere` | `keychain-access-groups` |

   The code reaches the shared keychain through the App Group alone (`kSecAttrAccessGroup = "group.dev.mingo.anywhere"`). Apple says an app can "use app group names as keychain access group names, without adding them to the Keychain access groups entitlement" ([Sharing access to keychain items](https://developer.apple.com/documentation/security/sharing-access-to-keychain-items-among-a-collection-of-apps)). Keychain Sharing is therefore optional, and if signing complains about it you can delete the `keychain-access-groups` key from all three files.
6. **Push relay.** The phone sends its APNs tokens to the relay, which hands back sealed handles. pocketd never sees the tokens or the APNs key.
   - Create an APNs auth key (`.p8`) in the developer portal under Keys, with Apple Push Notifications service enabled.
   - The default relay is `https://push.getanywhere.dev`. It can only push to bundle `dev.mingo.anywhere` under the key it holds. For your own team, deploy `packages/pushrelay` (see its README, `fly.toml`) with `APNS_KEY_P8`, `APNS_KEY_ID`, `APNS_TEAM_ID`, `BUNDLE_ID`, and `SEAL_KEY`/`MAC_KEY` (`openssl rand -base64 32` each).
   - Point the phone and the Mac at it. On the phone, `RelayClient.defaultBase` is in `Sources/PushSupport/RelayClient.swift`. On the Mac, run `pocketd config set push.relay https://<host>`.
   - Debug builds register their tokens as `development` (`PushEnv.current` follows `#if DEBUG`), and the relay picks the APNs host from that.
7. **Run** the `Anywhere` scheme on the phone. Allow notifications and local network access when asked.
8. **Pair.** On the Mac, open "Pair phone" in the desktop app, then scan its QR code from the phone (Scan, which asks for the camera). Or paste the `anywhere://pair?...` link or type the code. The phone has to reach the Mac directly, over Tailscale or the LAN.
9. **Smoke test.**
   - Sessions and chat stream in.
   - A terminal attaches and the key bar types.
   - Lock the phone with a session waiting on you: the alert arrives with Allow and Deny, and its decrypted title comes from the Notification Service Extension.
   - The Live Activity starts and shows on the Lock Screen and in the Dynamic Island, and Allow on it resolves the request.

## Fixed by this audit (found by `make typecheck-ios`)

- `FeaturePairing/ScanView.swift`: cast to `AVMetadataMachineReadableCodeObject`. There is no `AVMachineReadableCodeObject` type ([docs](https://developer.apple.com/documentation/avfoundation/avmetadatamachinereadablecodeobject)).
- `LiveActivityUI/AnswerIntent.swift`: `title` is now computed (`static var title: LocalizedStringResource { … }`). As a stored `static let`, it fails Swift 6's global-state check under any SDK where `LocalizedStringResource` isn't `Sendable`, and the iOS 17.2 one isn't. The requirement is `static var title: LocalizedStringResource { get }`.
- `App/AnywhereApp.swift`: now `@MainActor`, so the synthesized init may call `@UIApplicationDelegateAdaptor`'s main-actor initializer under any SDK.

## Checked, no change needed

- **SwiftTerm 1.20.0 (iOS).**
  - The `TerminalViewDelegate` requirements are `sizeChanged`, `setTerminalTitle`, `hostCurrentDirectoryUpdate`, `send`, `scrolled`, `requestOpenLink` and `rangeChanged`. `bell`, `clipboardCopy`, `clipboardRead` and `iTermContent` have defaults. `PhoneScreen.Coordinator` implements every required one.
  - `TerminalView.init(frame:font:)`, `feed(byteArray: ArraySlice<UInt8>)`, `font`, `getTerminal()`, `getOptimalFrameSize()`, the settable `inputAccessoryView`, `nativeForegroundColor` and `nativeBackgroundColor` all match.
  - Keystrokes leave through `send(data:)` and then `terminalDelegate?.send(source:data:)`. Emulator replies go through `send(source: Terminal, data:)`, which is the one `PhoneTerminalView` silences.
- **ActivityKit.**
  - These match the docs: `Activity.request(attributes:content:pushType:)`, which throws and isn't async; `end(_:dismissalPolicy:) async`; `activities`, `activityUpdates`, `pushToStartTokenUpdates` (iOS 17.2) and `pushTokenUpdates`; `activityStateUpdates`; `ActivityAuthorizationInfo().frequentPushesEnabled` and `frequentPushEnablementUpdates`; `ActivityContent(state:staleDate:relevanceScore:)`.
  - `Activity` isn't `Sendable` (it conforms only to `Identifiable`), and `LiveActivities` uses it only on the main actor.
  - The push payload's `attributes-type` is `MacActivity`, the type's name.
- **WidgetKit.** `ActivityConfiguration(for:content:dynamicIsland:)`, `DynamicIsland(expanded:compactLeading:compactTrailing:minimal:)`, `.widgetURL`, `.keylineTint`, `DynamicIslandExpandedRegion(_:priority:content:)`, `ActivityViewContext.attributes/state/isStale` and `activitySystemActionForegroundColor` all match.
- **App Intents.**
  - `LiveActivityIntent` exists on iOS 17 and Mac Catalyst but not macOS, so `AnswerIntent` keeps its `os(iOS)` guard.
  - `AppIntentsPackage.includedPackages: [any AppIntentsPackage.Type]`, `isDiscoverable` and `perform() async throws` all match.
  - The app and the widget each declare an `AppIntentsPackage` that includes `LiveActivityIntents`, and both link `LiveActivityUI`, so `AnswerIntent` is compiled into both.
- **UserNotifications.**
  - `UNUserNotificationCenterDelegate` isn't main-actor, so `AppDelegate`'s `nonisolated` async `willPresent` and `didReceive` are right.
  - `didReceive(_:withContentHandler:)` and `setBadgeCount(_:withCompletionHandler:)` match.
  - pocketd sends `mutable-content: 1`, which the Notification Service Extension needs.
- **XcodeGen.**
  - `options.deploymentTarget.iOS`, local `packages:` with `path:`, `package:` with `product:`, `sdk:`, `excludes` (relative to the source path), `INFOPLIST_FILE` and `CODE_SIGN_ENTITLEMENTS` all match the ProjectSpec.
  - `embed` "defaults to true for application target", so the app embeds both extensions.
- **Extension-safe API.** `LiveActivityUI`, `ActivityModel`, `DesignSystem`, `Domain`, `PushSupport`, `Client` and `Wire` (what the extensions link) don't use `UIApplication` or `openURL`.
- **`#if os(iOS)` guards on macOS.** Every guard in `Sources` wraps UIKit, AVFoundation, WebKit, ActivityKit, iOS-only modifiers (`navigationBarTitleDisplayMode`, `.insetGrouped`, `textInputAutocapitalization`, `keyboardType`, `topBarTrailing`), `LiveActivityIntent` or `UNUserNotificationCenter`. `UNUserNotificationCenter.current()` traps in an executable without a bundle, such as the gallery or the tests. `PhotosPicker` exists on macOS 13, but the PhotosUI SwiftUI overlay doesn't load with Swift 6.2 against the 14.2 SDK (`cannot find 'PhotosPicker' in scope`), so that guard stays too.

## Residual risks

- **SwiftUI isolation.** The local SDKs predate `View`, `Widget` and `App` being `@MainActor` (the current docs list all three as `@MainActor @preconcurrency`). The typecheck therefore warns about main-actor calls in `MacActivityWidget.body`, and Xcode shouldn't. Xcode's SDK can also raise isolation or `Sendable` errors that neither the macOS build nor the Catalyst typecheck sees.
- **iOS 18 and 26 API by hand.** The Catalyst SDK is iOS 17.2. ActivityKit, WidgetKit's Live Activity types, `glassEffect` and `PhotosPicker` were checked against stubs copied from the docs, not against Apple's headers.
- **SwiftTerm 1.20.0 against the iOS 26 SDK.** Never compiled. It typechecks for Catalyst (iOS 17.2) in Swift 5 mode.
- **App Intents from a static Swift package.** Xcode 16 and later extract App Intents metadata from package targets through `AppIntentsPackage`. Whether the Live Activity's Allow and Deny buttons find `AnswerIntent` at run time is untested.
- **`UIDevice.current.name`.** Since iOS 16 it returns the model ("iPhone") unless the app has the `com.apple.developer.device-information.user-assigned-device-name` entitlement. Every phone will pair as "iPhone".
- **`PushEnv` follows `DEBUG`, not the provisioning profile.** A Release build run straight from Xcode is development-signed but registers `production`, and APNs then rejects its token (BadDeviceToken).
- **No asset catalog**, so there's no app icon. It builds, but App Store validation will refuse it.
- **`swift-cmark` tracks branch `gfm`**, so Xcode may resolve a newer commit than the one tested.
- **Runtime-only behaviour.** None of these could be exercised here:
  - camera permission and QR scanning;
  - Local Network permission over Tailscale;
  - background launch for notification actions and the Live Activity intent, where `Answering.handler` is set in `didFinishLaunching` before the intent runs;
  - Notification Service Extension keychain reads while the phone is locked (items are `AfterFirstUnlockThisDeviceOnly`);
  - push-to-start Live Activities.
