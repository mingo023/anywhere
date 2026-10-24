# Anywhere for iPhone and iPad

The native SwiftUI phone app ([ADR 0007](../../docs/adr/0007-native-ios-app.md)). Nearly all of it is the local Swift package [`Packages/AnywhereKit`](Packages/AnywhereKit); the Xcode targets in `App/`, `Widgets/` and `NotificationService/` only compose it. The module map and the rules every module follows are in [`docs/mobile/plan.md`](../../docs/mobile/plan.md), and the wire contract in [`docs/mobile/protocol.md`](../../docs/mobile/protocol.md).

## Without Xcode

The package builds and tests on macOS 14 or later with a Swift 6.2 toolchain, for example from [swiftly](https://www.swift.org/install):

```sh
make build      # every module, for macOS
make test       # every test, swift-testing
make bench      # decode timings at repo scale, in release
```

Run one module with its own scratch folder so parallel builds don't share `.build`:

```sh
cd Packages/AnywhereKit
swift build --scratch-path /tmp/ak-me --target Wire
swift test --scratch-path /tmp/ak-me --filter WireTests
```

Code that needs the iOS SDK (UIKit, ActivityKit, WidgetKit, SwiftTerm's view, iOS 26 glass) sits inside `#if os(iOS)` and is only compiled by Xcode.

## With Xcode

```sh
brew install xcodegen
make gen        # writes Anywhere.xcodeproj from project.yml
open Anywhere.xcodeproj
```

Set your team under Signing for the three targets, then run the `Anywhere` scheme. The app needs push notifications, an App Group `group.dev.mingo.anywhere` and keychain sharing, all named in the `.entitlements` files.
