import Foundation

/// Repo images by scope and source, decoded once.
@MainActor
enum RepoImageCache {
    static let cache = NSCache<NSString, PlatformImage>()

    static func image(_ repo: RepoImages, _ source: String) -> PlatformImage? {
        cache.object(forKey: key(repo, source))
    }

    static func load(_ repo: RepoImages, _ source: String) async -> PlatformImage? {
        guard let data = await repo.load(source), let image = PlatformImage(data: data) else { return nil }
        cache.setObject(image, forKey: key(repo, source))
        return image
    }

    private static func key(_ repo: RepoImages, _ source: String) -> NSString { (repo.scope + "\u{0}" + source) as NSString }
}
