/// The desktop's colours (packages/desktop/crates/theme/src/theme.rs), by the same names.
/// Where the desktop relies on its translucent window, the phone takes the solid colour it reads as.
public enum Palette {
    public static let text = Token(0x111113ff, 0xebebebff)
    public static let textBody = Token(0x3f3f46ff, 0xbcbcbcff)
    public static let text2 = Token(0x6f6f78ff, 0xa1a1a1ff)
    public static let text3 = Token(0x8b8b94ff, 0x818181ff)
    public static let text4 = Token(0xa1a1aaff, 0x6c6c6cff)
    public static let text5 = Token(0xb4b4bcff, 0x616161ff)
    public static let text6 = Token(0xd4d4d8ff, 0x414141ff)
    /// The band of light crossing a working label.
    public static let shimmer = Token(0xd4d4d8ff, 0xffffffff)
    /// Text and glyphs on a `text` fill.
    public static let onText = Token(0xffffffff, 0x171717ff)

    /// The screen behind grouped content. The desktop's dark window is glass over the desktop; the phone's is solid.
    public static let background = Token(0xf4f4f5ff, 0x111111ff)
    /// Grouped rows and cards.
    public static let surface = Token(0xffffffff, 0x1f1f21ff)
    public static let surfaceRaised = Token(0xffffffff, 0x2a2a2cff)
    /// A recessed block inside a page: tables, code blocks.
    public static let well = Token(0xf4f4f5ff, 0xebebeb0d)
    public static let popover = Token(0xffffffff, 0x232323ff)

    public static let fill1 = Token(0x11111308, 0xebebeb0d)
    public static let fill2 = Token(0x1111130b, 0xebebeb14)
    public static let fill3 = Token(0x1111130e, 0xebebeb1a)
    public static let fill4 = Token(0x11111311, 0xebebeb1f)
    public static let hairline = Token(0x11111312, 0xebebeb12)
    public static let separator = Token(0x11111317, 0xebebeb14)
    public static let separatorStrong = Token(0x1111131f, 0xebebeb24)
    public static let selection = Token(0x11111324, 0xebebeb33)

    /// No hue: the accent is ink, so links and focus read as weight rather than colour.
    public static let accent = Token(0x3f3f46ff, 0xd4d4d4ff)
    public static let accentBackground = Token(0x1111131c, 0xebebeb33)
    public static let accentTint = Token(0x11111317, 0xebebeb29)
    public static let link = Token(0x0068daff, 0x419cffff)

    public static let waiting = Token(0xad5700ff, 0xffb224ff)
    public static let waitingText = Token(0xad5700ff, 0xffca16ff)
    /// The amber of dots and badges, bright in light mode too.
    public static let waitingDot = Token.fixed(0xffb224ff)
    public static let onWaiting = Token.fixed(0x1a1306ff)
    public static let waitingBackground = Token.fixed(0xffb2242e)
    public static let failed = Token(0xcd2b31ff, 0xe5484dff)
    public static let failedText = Token(0xcd2b31ff, 0xff9592ff)
    public static let failedBackground = Token.fixed(0xe5484d1c)
    public static let success = Token(0x2b9a66ff, 0x30a46cff)
    public static let successText = Token(0x18794eff, 0x3dd68cff)
    public static let successBackground = Token.fixed(0x30a46c21)
    public static let working = Token(0xcc4e00ff, 0xf76b15ff)
    public static let merged = Token(0x8250dfff, 0xa371f7ff)
    public static let modified = Token(0xad5700ff, 0xffca16ff)
    public static let teal = Token(0x0e7c86ff, 0x0bd8b6ff)

    /// A chat's work rail: the line a reply's steps hang from.
    public static let rail = Token(0xdcdcdeff, 0x323237ff)
    public static let bubble = Token(0xf1f1f2ff, 0x2c2c30ff)
    public static let mention = Token(0x0284c7ff, 0x5bb8f0ff)
    /// A prompt's opening `/command`.
    public static let command = Token(0x5b5bd6ff, 0xb1a9ffff)
    public static let agentClaude = Token.fixed(0xd97757ff)

    public static let diffAddBackground = Token.fixed(0x30a46c1c)
    public static let diffAddText = Token(0x18794eff, 0x3dd68cff)
    public static let diffDelBackground = Token.fixed(0xe5484d17)
    public static let diffDelText = Token(0xcd2b31ff, 0xff9592ff)
    public static let diffAddWord = Token.fixed(0x30a46c40)
    public static let diffDelWord = Token.fixed(0xe5484d38)

    public static let graphHead = Token(0x1a5cffff, 0x57a3f8ff)
    public static let graphUpstream = Token(0x652d90ff, 0xad80d7ff)
    public static let graphBase = Token.fixed(0xea5c00ff)
    public static let graphLanes: [Token] = [.fixed(0xffb000ff), .fixed(0xdc267fff), .fixed(0x994f00ff), .fixed(0x40b0a6ff), .fixed(0xb66dffff)]

    public static let markdownText = Token(0x000000d9, 0xffffffd9)
    public static let markdownText2 = Token(0x00000080, 0xffffff8c)
    public static let markdownGrid = Token(0x0000001a, 0xffffff1a)
    public static let markdownAside = Token(0xf5f5f7ff, 0x323232ff)
    public static let markdownCodeBackground = Token(0xf9f9f9ff, 0x262626ff)
    public static let markdownCodeBorder = Token(0xf0f0f0ff, 0x323232ff)

    public static let syntaxKeyword = Token(0x8e4ec6ff, 0xd19dffff)
    public static let syntaxFunction = Token(0x3e63ddff, 0x9eb1ffff)
    public static let syntaxString = Token(0x18794eff, 0x3dd68cff)
    public static let syntaxComment = Token(0xa1a1aaff, 0x818181ff)
}
