import SwiftUI

/// Where a markdown file's own images come from, as the Files preview reads them: `load` takes a source as written
/// (`img/logo.png`) and returns its bytes, nil when it isn't an image in the repo. `scope` names the file the sources
/// are relative to, so two files' `logo.png` are cached apart.
public struct RepoImages: Sendable {
    let scope: String
    let load: @Sendable (String) async -> Data?

    public init(scope: String, load: @escaping @Sendable (String) async -> Data?) {
        self.scope = scope
        self.load = load
    }
}

private struct RepoImagesKey: EnvironmentKey {
    static let defaultValue: RepoImages? = nil
}

extension EnvironmentValues {
    /// Set by the Files preview; chat and PR markdown has none, so its relative images stay chips.
    public var repoImages: RepoImages? {
        get { self[RepoImagesKey.self] }
        set { self[RepoImagesKey.self] = newValue }
    }
}
