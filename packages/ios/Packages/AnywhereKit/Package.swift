// swift-tools-version: 6.2
import PackageDescription

let cmark: [Target.Dependency] = [
    .product(name: "cmark-gfm", package: "swift-cmark"),
    .product(name: "cmark-gfm-extensions", package: "swift-cmark"),
]

/// Each module, the modules it imports, and whether it has a test target.
let modules: [(name: String, deps: [Target.Dependency], tested: Bool)] = [
    ("Wire", [], true),
    ("Client", ["Wire"], true),
    ("Domain", [], true),
    ("ChatModel", ["Wire", "Domain"], true),
    ("MarkdownModel", cmark, true),
    ("Highlight", [], true),
    ("DiffModel", ["Wire"], true),
    ("ActivityModel", [], true),
    ("PushSupport", ["Wire"], true),
    ("Fixtures", ["Wire"], true),
    ("Stores", ["Wire", "Client", "Domain", "ChatModel", "DiffModel", "PushSupport", "ActivityModel"], true),
    ("DesignSystem", ["Domain"], true),
    ("MarkdownView", ["MarkdownModel", "Highlight", "DesignSystem"], true),
    ("CodeView", ["Highlight", "DiffModel", "DesignSystem"], true),
    ("FeatureChat", ["Stores", "ChatModel", "Wire", "MarkdownView", "CodeView", "DesignSystem", "Domain", "Fixtures"], true),
    ("FeatureSessions", ["Stores", "DesignSystem", "Domain", "Fixtures"], true),
    ("FeatureTerminal", ["Stores", "DesignSystem", "Domain", "Fixtures", .product(name: "SwiftTerm", package: "SwiftTerm")], true),
    ("FeatureGit", ["Stores", "Client", "CodeView", "DiffModel", "DesignSystem", "Domain", "Fixtures"], true),
    ("FeatureGitHub", ["Stores", "MarkdownView", "DesignSystem", "Domain", "Fixtures"], true),
    ("FeatureFiles", ["Stores", "CodeView", "MarkdownView", "DesignSystem", "Domain", "Fixtures"], true),
    ("FeaturePairing", ["Stores", "Client", "DesignSystem", "Domain", "Fixtures"], true),
    ("FeatureSettings", ["Stores", "DesignSystem", "Domain", "Wire", "Client", "Fixtures"], true),
    ("AppShell", [
        "Stores", "Domain", "DesignSystem", "FeatureChat", "FeatureSessions", "FeatureTerminal", "FeatureGit",
        "FeatureGitHub", "FeatureFiles", "FeaturePairing", "FeatureSettings", "Fixtures",
    ], true),
    ("LiveActivityUI", ["ActivityModel", "DesignSystem"], true),
]

let executables: [(name: String, deps: [Target.Dependency])] = [
    ("anywhere-probe", ["Wire", "Client", "Stores", "PushSupport"]),
    ("anywhere-gallery", ["Fixtures", "DesignSystem", "AppShell", "Stores", "FeatureChat", "FeatureSessions", "FeatureTerminal", "FeatureGit", "FeatureGitHub", "FeatureFiles", "FeaturePairing", "FeatureSettings", "MarkdownView", "CodeView", "LiveActivityUI"]),
    ("anywhere-bench", [
        "Wire", "Fixtures", "MarkdownModel", "ChatModel", "DiffModel", "Highlight", "CodeView", "Domain", "Client", "Stores", "DesignSystem",
        "FeatureChat", "FeatureGit", "FeatureFiles", .product(name: "SwiftTerm", package: "SwiftTerm"),
    ]),
]

let package = Package(
    name: "AnywhereKit",
    platforms: [.iOS(.v18), .macOS(.v14)],
    products: modules.map { .library(name: $0.name, targets: [$0.name]) },
    dependencies: [
        .package(url: "https://github.com/swiftlang/swift-cmark", branch: "gfm"),
        .package(url: "https://github.com/migueldeicaza/SwiftTerm", exact: "1.20.0"),
    ],
    targets: modules.map { .target(name: $0.name, dependencies: $0.deps) }
        + modules.filter(\.tested).map {
            .testTarget(name: "\($0.name)Tests", dependencies: $0.name == "Fixtures" ? ["Fixtures"] : [.byName(name: $0.name), "Fixtures"])
        }
        + executables.map { .executableTarget(name: $0.name, dependencies: $0.deps) },
    swiftLanguageModes: [.v6]
)
