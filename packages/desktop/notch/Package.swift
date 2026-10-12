// swift-tools-version:5.9
import PackageDescription

// The island that grows out of the notch to answer agents' asks, which pocket-desktop runs as a helper.
let package = Package(
    name: "AnywhereNotch",
    platforms: [.macOS(.v13)],
    products: [.executable(name: "anywhere-notch", targets: ["AnywhereNotch"])],
    targets: [
        .executableTarget(name: "AnywhereNotch"),
        .testTarget(name: "AnywhereNotchTests", dependencies: ["AnywhereNotch"]),
    ]
)
