/// Whether inline `code` names a path, and if so whether it names a folder (desktop `pocket/chat/markdown.rs path_in`):
/// no whitespace, no `://`, holds a letter, and holds a slash or names a file with an icon of its own.
public func pathFolder(in code: String) -> Bool? {
    guard !code.contains(where: \.isWhitespace), !code.contains("://"), code.contains(where: \.isLetter) else { return nil }
    guard code.contains("/") || FileIcons.has(code) else { return nil }
    return code.hasSuffix("/")
}
