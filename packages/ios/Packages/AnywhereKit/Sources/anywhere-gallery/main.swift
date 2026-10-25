import AppKit
import AppShell
import CodeView
import DesignSystem
import FeatureChat
import FeatureFiles
import FeatureGit
import FeatureGitHub
import FeaturePairing
import FeatureSessions
import FeatureSettings
import FeatureTerminal
import Foundation
import LiveActivityUI
import MarkdownView
import SwiftUI

// Renders every gallery scene to PNG: anywhere-gallery <outdir> [filter]
// Each scene is laid out in an offscreen window at its size and appearance, given a few run-loop turns so lazy stacks,
// tasks and onAppear settle, then drawn with cacheDisplay.

@MainActor
func everyScene() -> [GalleryScene] {
    AppShellGallery.scenes + PairingGallery.scenes + SettingsGallery.scenes + NotificationsGallery.scenes + SessionsGallery.scenes
        + ChatGallery.scenes + TerminalGallery.scenes + GitGallery.scenes + GitHubGallery.scenes + FilesGallery.scenes
        + MarkdownGallery.scenes + CodeGallery.scenes + LiveActivityGallery.scenes
}

/// Lazy content an offscreen render could miss: rows of a LazyVStack in a ScrollView, each a different shade.
@MainActor
func lazyCheck() -> GalleryScene {
    GalleryScene("gallery-check-lazy", size: CGSize(width: 200, height: 400)) {
        AnyView(ScrollView {
            LazyVStack(spacing: 0) {
                ForEach(0..<100, id: \.self) { i in
                    Color(hue: Double(i % 10) / 10, saturation: 0.8, brightness: 0.9).frame(height: 20)
                }
            }
        })
    }
}

@MainActor
func run(_ args: [String]) throws -> Int32 {
    guard args.count >= 2 else {
        FileHandle.standardError.write(Data("usage: anywhere-gallery <outdir> [filter]\n".utf8))
        return 2
    }
    let out = URL(fileURLWithPath: args[1], isDirectory: true)
    let filter = args.count > 2 ? args[2] : nil
    try FileManager.default.createDirectory(at: out, withIntermediateDirectories: true)
    NSApplication.shared.setActivationPolicy(.prohibited)

    guard let check = GalleryRenderer.render(lazyCheck()), check.variance > 100 else {
        print("FAIL gallery-check-lazy: a ScrollView's LazyVStack drew nothing")
        return 1
    }
    print("gallery-check-lazy: lazy rows drawn, variance \(String(format: "%.1f", check.variance))")

    let scenes = everyScene().filter { filter == nil || $0.name.contains(filter!) }
    var failed = 0
    for scene in scenes {
        let started = Date()
        guard let png = GalleryRenderer.render(scene) else {
            print("FAIL \(scene.name): nothing drawn")
            failed += 1
            continue
        }
        try png.data.write(to: out.appendingPathComponent("\(scene.name).png"))
        let ms = Int(Date().timeIntervalSince(started) * 1000)
        print("\(scene.name).png  \(Int(scene.size.width))×\(Int(scene.size.height))  variance \(String(format: "%.1f", png.variance))  \(ms) ms")
    }
    print("\(scenes.count - failed) of \(scenes.count) scenes rendered to \(out.path)")
    return failed == 0 ? 0 : 1
}

let code = try MainActor.assumeIsolated { try run(CommandLine.arguments) }
exit(code)
