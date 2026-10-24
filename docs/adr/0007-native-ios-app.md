# The phone app is native SwiftUI, one local package of modules, served by typed pocketd verbs

The React Native app in `packages/app` is replaced by a native SwiftUI app in `packages/ios`. Nothing of its UI is kept; its wire usage was only a reference. We chose native because the phone has to do what RN made slow or impossible: a real terminal (SwiftTerm), Live Activities and the Dynamic Island, a Notification Service Extension, and chat, diff and file views that stay at 120 Hz with thousands of rows.

## Layout

- `packages/ios/project.yml` is an XcodeGen spec for three thin iOS targets: the app (`App/`), the widget extension with the Live Activity (`Widgets/`), and the Notification Service Extension (`NotificationService/`). The generated `.xcodeproj` is not committed.
- Everything else is the local SwiftPM package `packages/ios/Packages/AnywhereKit`, platforms iOS 18 and macOS 14. macOS exists only so the package builds and tests with `swift build` / `swift test` without Xcode.
- **Model modules never import SwiftUI** (`Wire`, `Client`, `Domain`, `ChatModel`, `MarkdownModel`, `Highlight`, `DiffModel`, `ActivityModel`, `PushSupport`). `Stores` imports only Observation. Views live in `DesignSystem`, `MarkdownView`, `CodeView` and the `Feature*` modules, which never import each other; navigation crosses features as `Route` values from `Domain`.
- Any API newer than iOS 17 / macOS 14 sits inside `#if os(iOS)`, because the local SDK is macOS 14.2. So does UIKit, ActivityKit and SwiftTerm's view.
- One file per component. Tests sit beside each module in `Tests/<Module>Tests`, swift-testing, named as sentences.

## State

`@MainActor @Observable` stores, no TCA. One `DaemonConnection` actor per Mac decodes frames off the main actor and hands batches to `AppModel.apply`, which routes each message to the store that owns it. Pure decisions (folding the timeline into rows, the outgoing queue, markdown blocks, diff hunks, graph lanes, Up next order) are value types and free functions, ported from the desktop's model crates with their tests.

## Wire

New verbs are added to protocol 3 behind caps, specified in `docs/mobile/protocol.md`: `terminal.v1`, `workspace.v1`, `git.v1`, `gh.v1`, `fs.v1`, `push.v1`, plus `agent.timeline{beforeSeq}`. Phones never get `exec` or raw `fs.*`; every git, GitHub and file verb takes a tree that must be a registered project, worktree or Local, and pocketd runs a fixed argv. Terminal output travels in binary WebSocket frames.

## Push

pocketd never holds Apple credentials. A small stateless relay (`packages/pushrelay`) holds the `.p8` and pushes only to handles a phone sealed itself; pocketd signs each send with that handle's secret. Alert content is AES-GCM sealed under a key only the phone and pocketd share, and the Notification Service Extension opens it. Live Activity state carries counts, statuses and agent ids only.

## Consequences

- Building the iOS targets needs Xcode; the package does not.
- Desktop and phone now both read git and GitHub: the desktop in Rust, pocketd in Go for phones. The argv and parsers are ported from `crates/git` so the two agree; moving the desktop onto pocketd's verbs is a later step.
