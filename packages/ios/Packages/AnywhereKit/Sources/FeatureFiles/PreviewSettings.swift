import CodeView

/// The code preview's settings, as the More menu changes them (desktop Files settings).
enum PreviewSettings {
    /// Wrap is on at compact width until the reader picks.
    static func wrap(_ stored: Bool?, compact: Bool) -> Bool { stored ?? compact }

    /// The text size one step smaller or larger; nil past the ends.
    static func step(_ size: Int, by delta: Int) -> Int? {
        let next = size + delta
        return CodeStyle.textSizes.contains(next) ? next : nil
    }
}
