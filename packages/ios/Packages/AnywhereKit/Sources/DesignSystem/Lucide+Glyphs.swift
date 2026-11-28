/// Lucide icons, the desktop's set, by name.
public enum Lucide: String, CaseIterable, Sendable {
    case activity
    case sunMoon
    case arrowDown
    case arrowDownWideNarrow
    case arrowRight
    case arrowUp
    case atSign
    case chevronLeft
    case bell
    case bellOff
    case blocks
    case bolt
    case bot
    case branch
    case camera
    case cameraOff
    case check
    case chevronDown
    case chevronUp
    case circleAlert
    case circleCheck
    case circleX
    case clock
    case cloud
    case cloudOff
    case code
    case comment
    case commit
    case compose
    case copy
    case cornerDownRight
    case diffMultiple
    case discard
    case explorer
    case external
    case eye
    case eyeOff
    case file
    case fileDiff
    case filePlus
    case fileText
    case filter
    case fit
    case flashlight
    case flow
    case folder
    case folderOpen
    case folderPlus
    case chevronRight
    case gitBranch
    case github
    case globe
    case grip
    case imagePlus
    case inbox
    case info
    case keyboard
    case laptop
    case listChecks
    case listFlat
    case listTree
    case lock
    case merge
    case messageCircle
    case mic
    case minus
    case monitor
    case monitorSmartphone
    case ellipsis
    case package
    case pencil
    case phone
    case pin
    case play
    case plus
    case pointer
    case prompt
    case pullRequest
    case pullRequestClosed
    case pullRequestDraft
    case reload
    case reply
    case rotateCw
    case runCancelled
    case runSkipped
    case scanQrCode
    case search
    case send
    case server
    case sessions
    case settings
    case shield
    case shieldAlert
    case sidebar
    case sidebarCollapse
    case sidebarExpand
    case sliders
    case sparkle
    case splitDown
    case splitRight
    case star
    case stop
    case terminal
    case textWrap
    case trash
    case unfold
    case unlink
    case upload
    case volume
    case triangleAlert
    case wifi
    case worktree
    case close
}

// Generated from the desktop's Lucide SVGs (packages/desktop/crates/theme/assets/icons) and the iOS redesign's.
// The SVG bodies on Lucide's 24-unit grid; `LucidePath` turns them into paths.

extension Lucide {
    var svg: String {
        switch self {
        case .activity: "<path d=\"M22 12h-2.48a2 2 0 0 0-1.93 1.46l-2.35 8.36a.25.25 0 0 1-.48 0L9.24 2.18a.25.25 0 0 0-.48 0l-2.35 8.36A2 2 0 0 1 4.49 12H2\"/>"
        case .sunMoon: "<circle cx=\"12\" cy=\"12\" r=\"10\"/><path d=\"M12 18a6 6 0 0 0 0-12v12z\"/>"
        case .arrowDown: "<path d=\"M12 5v14\"/><path d=\"m19 12-7 7-7-7\"/>"
        case .arrowDownWideNarrow: "<path d=\"m3 16 4 4 4-4\"/><path d=\"M7 20V4\"/><path d=\"M11 4h10\"/><path d=\"M11 8h7\"/><path d=\"M11 12h4\"/>"
        case .arrowRight: "<path d=\"M5 12h14\"/><path d=\"m12 5 7 7-7 7\"/>"
        case .arrowUp: "<path d=\"m5 12 7-7 7 7\"/><path d=\"M12 19V5\"/>"
        case .atSign: "<circle cx=\"12\" cy=\"12\" r=\"4\"/><path d=\"M16 8v5a3 3 0 0 0 6 0v-1a10 10 0 1 0-4 8\"/>"
        case .chevronLeft: "<path d=\"m15 18-6-6 6-6\"/>"
        case .bell: "<path d=\"M10.268 21a2 2 0 0 0 3.464 0\"/><path d=\"M3.262 15.326A1 1 0 0 0 4 17h16a1 1 0 0 0 .74-1.673C19.41 13.956 18 12.499 18 8A6 6 0 0 0 6 8c0 4.499-1.411 5.956-2.738 7.326\"/>"
        case .bellOff: "<path d=\"M10.268 21a2 2 0 0 0 3.464 0\"/><path d=\"M17 17H4a1 1 0 0 1-.74-1.673C4.59 13.956 6 12.499 6 8a6 6 0 0 1 .258-1.742\"/><path d=\"m2 2 20 20\"/><path d=\"M8.668 3.01A6 6 0 0 1 18 8c0 2.687.77 4.653 1.707 6.05\"/>"
        case .blocks: "<path d=\"M10 22V7a1 1 0 0 0-1-1H4a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-5a1 1 0 0 0-1-1H2\"/><rect x=\"14\" y=\"2\" width=\"8\" height=\"8\" rx=\"1\"/>"
        case .bolt: "<path d=\"M15.914 4a1.5 1.5 0 00-2.474-1.561l-9 9A1.5 1.5 0 005.5 14h4.002a.5.5 0 01.471.666L8.086 20a1.5 1.5 0 002.475 1.56l9-9A1.5 1.5 0 0018.5 10h-3.997a.5.5 0 01-.472-.667z\"/>"
        case .bot: "<path d=\"M12 8V4H8\"/><rect width=\"16\" height=\"12\" x=\"4\" y=\"8\" rx=\"2\"/><path d=\"M2 14h2\"/><path d=\"M20 14h2\"/><path d=\"M15 13v2\"/><path d=\"M9 13v2\"/>"
        case .branch: "<path d=\"M15 6a9 9 0 0 0-9 9V3\"/><circle cx=\"18\" cy=\"6\" r=\"3\"/><circle cx=\"6\" cy=\"18\" r=\"3\"/>"
        case .camera: "<path d=\"M13.997 4a2 2 0 0 1 1.76 1.05l.486.9A2 2 0 0 0 18.003 7H20a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V9a2 2 0 0 1 2-2h1.997a2 2 0 0 0 1.759-1.048l.489-.904A2 2 0 0 1 10.004 4z\"/><circle cx=\"12\" cy=\"13\" r=\"3\"/>"
        case .cameraOff: "<path d=\"M14.564 14.558a3 3 0 1 1-4.122-4.121\"/><path d=\"m2 2 20 20\"/><path d=\"M20 20H4a2 2 0 0 1-2-2V9a2 2 0 0 1 2-2h1.997a2 2 0 0 0 .819-.175\"/><path d=\"M9.695 4.024A2 2 0 0 1 10.004 4h3.993a2 2 0 0 1 1.76 1.05l.486.9A2 2 0 0 0 18.003 7H20a2 2 0 0 1 2 2v7.344\"/>"
        case .check: "<path d=\"M20 6 9 17l-5-5\"/>"
        case .chevronDown: "<path d=\"m6 9 6 6 6-6\"/>"
        case .chevronUp: "<path d=\"m18 15-6-6-6 6\"/>"
        case .circleAlert: "<circle cx=\"12\" cy=\"12\" r=\"10\"/><line x1=\"12\" x2=\"12\" y1=\"8\" y2=\"12\"/><line x1=\"12\" x2=\"12.01\" y1=\"16\" y2=\"16\"/>"
        case .circleCheck: "<circle cx=\"12\" cy=\"12\" r=\"10\"/><path d=\"m9 12 2 2 4-4\"/>"
        case .circleX: "<circle cx=\"12\" cy=\"12\" r=\"10\"/><path d=\"m15 9-6 6\"/><path d=\"m9 9 6 6\"/>"
        case .clock: "<circle cx=\"12\" cy=\"12\" r=\"10\"/><path d=\"M12 6v6l4 2\"/>"
        case .cloud: "<path d=\"M17.5 19H9a7 7 0 1 1 6.71-9h1.79a4.5 4.5 0 1 1 0 9Z\"/>"
        case .cloudOff: "<path d=\"m2 2 20 20\"/><path d=\"M5.782 5.782A7 7 0 0 0 9 19h8.5a4.5 4.5 0 0 0 1.307-.193\"/><path d=\"M21.532 16.5A4.5 4.5 0 0 0 17.5 10h-1.79A7.008 7.008 0 0 0 10 5.07\"/>"
        case .code: "<path d=\"m16 18 6-6-6-6\"/><path d=\"m8 6-6 6 6 6\"/>"
        case .comment: "<path d=\"M22 17a2 2 0 0 1-2 2H6.828a2 2 0 0 0-1.414.586l-2.202 2.202A.71.71 0 0 1 2 21.286V5a2 2 0 0 1 2-2h16a2 2 0 0 1 2 2z\"/>"
        case .commit: "<circle cx=\"12\" cy=\"12\" r=\"3\"/><line x1=\"3\" x2=\"9\" y1=\"12\" y2=\"12\"/><line x1=\"15\" x2=\"21\" y1=\"12\" y2=\"12\"/>"
        case .compose: "<path d=\"M12 3H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7\"/><path d=\"M18.375 2.625a1 1 0 0 1 3 3l-9.013 9.014a2 2 0 0 1-.853.505l-2.873.84a.5.5 0 0 1-.62-.62l.84-2.873a2 2 0 0 1 .506-.852z\"/>"
        case .copy: "<rect width=\"14\" height=\"14\" x=\"8\" y=\"8\" rx=\"2\" ry=\"2\"/><path d=\"M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2\"/>"
        case .cornerDownRight: "<path d=\"m15 10 5 5-5 5\"/><path d=\"M4 4v7a4 4 0 0 0 4 4h12\"/>"
        case .diffMultiple: "<path d=\"M6 22a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h8a2.4 2.4 0 0 1 1.704.706l3.588 3.588A2.4 2.4 0 0 1 20 8v12a2 2 0 0 1-2 2z\"/><path d=\"M9 10h6\"/><path d=\"M12 13V7\"/><path d=\"M9 17h6\"/>"
        case .discard: "<path d=\"M9 14 4 9l5-5\"/><path d=\"M4 9h10.5a5.5 5.5 0 0 1 5.5 5.5a5.5 5.5 0 0 1-5.5 5.5H11\"/>"
        case .explorer: "<path d=\"M15 2h-4a2 2 0 0 0-2 2v11a2 2 0 0 0 2 2h8a2 2 0 0 0 2-2V8\"/><path d=\"M16.706 2.706A2.4 2.4 0 0 0 15 2v5a1 1 0 0 0 1 1h5a2.4 2.4 0 0 0-.706-1.706z\"/><path d=\"M5 7a2 2 0 0 0-2 2v11a2 2 0 0 0 2 2h8a2 2 0 0 0 1.732-1\"/>"
        case .external: "<path d=\"M15 3h6v6\"/><path d=\"M10 14 21 3\"/><path d=\"M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6\"/>"
        case .eye: "<path d=\"M2.062 12.348a1 1 0 0 1 0-.696 10.75 10.75 0 0 1 19.876 0 1 1 0 0 1 0 .696 10.75 10.75 0 0 1-19.876 0\"/><circle cx=\"12\" cy=\"12\" r=\"3\"/>"
        case .eyeOff: "<path d=\"M10.733 5.076a10.744 10.744 0 0 1 11.205 6.575 1 1 0 0 1 0 .696 10.747 10.747 0 0 1-1.444 2.49\"/><path d=\"M14.084 14.158a3 3 0 0 1-4.242-4.242\"/><path d=\"M17.479 17.499a10.75 10.75 0 0 1-15.417-5.151 1 1 0 0 1 0-.696 10.75 10.75 0 0 1 4.446-5.143\"/><path d=\"m2 2 20 20\"/>"
        case .file: "<path d=\"M6 22a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h8a2.4 2.4 0 0 1 1.704.706l3.588 3.588A2.4 2.4 0 0 1 20 8v12a2 2 0 0 1-2 2z\"/><path d=\"M14 2v5a1 1 0 0 0 1 1h5\"/>"
        case .fileDiff: "<path d=\"M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z\"/><path d=\"M9 10h6\"/><path d=\"M12 13V7\"/><path d=\"M9 17h6\"/>"
        case .filePlus: "<path d=\"M6 22a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h8a2.4 2.4 0 0 1 1.704.706l3.588 3.588A2.4 2.4 0 0 1 20 8v12a2 2 0 0 1-2 2z\"/><path d=\"M14 2v5a1 1 0 0 0 1 1h5\"/><path d=\"M9 15h6\"/><path d=\"M12 18v-6\"/>"
        case .fileText: "<path d=\"M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z\"/><path d=\"M14 2v4a2 2 0 0 0 2 2h4\"/><path d=\"M10 9H8\"/><path d=\"M16 13H8\"/><path d=\"M16 17H8\"/>"
        case .filter: "<path d=\"M10 20a1 1 0 0 0 .553.895l2 1A1 1 0 0 0 14 21v-7a2 2 0 0 1 .517-1.341L21.74 4.67A1 1 0 0 0 21 3H3a1 1 0 0 0-.742 1.67l7.225 7.989A2 2 0 0 1 10 14z\"/>"
        case .fit: "<path d=\"M11 19H5v-6\"/><path d=\"M13 5h6v6\"/><path d=\"M19 5 5 19\"/>"
        case .flashlight: "<path d=\"M18 6c0 2-2 2-2 4v10a2 2 0 0 1-2 2h-4a2 2 0 0 1-2-2V10c0-2-2-2-2-4V2h12z\"/><line x1=\"6\" x2=\"18\" y1=\"6\" y2=\"6\"/><line x1=\"12\" x2=\"12\" y1=\"12\" y2=\"12\"/>"
        case .flow: "<rect width=\"8\" height=\"8\" x=\"3\" y=\"3\" rx=\"2\"/><path d=\"M7 11v4a2 2 0 0 0 2 2h4\"/><rect width=\"8\" height=\"8\" x=\"13\" y=\"13\" rx=\"2\"/>"
        case .folder: "<path d=\"M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z\"/>"
        case .folderOpen: "<path d=\"m6 14 1.5-2.9A2 2 0 0 1 9.24 10H20a2 2 0 0 1 1.94 2.5l-1.54 6a2 2 0 0 1-1.95 1.5H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h3.9a2 2 0 0 1 1.69.9l.81 1.2a2 2 0 0 0 1.67.9H18a2 2 0 0 1 2 2v2\"/>"
        case .folderPlus: "<path d=\"M12 10v6\"/><path d=\"M9 13h6\"/><path d=\"M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z\"/>"
        case .chevronRight: "<path d=\"m9 18 6-6-6-6\"/>"
        case .gitBranch: "<line x1=\"6\" x2=\"6\" y1=\"3\" y2=\"15\"/><circle cx=\"18\" cy=\"6\" r=\"3\"/><circle cx=\"6\" cy=\"18\" r=\"3\"/><path d=\"M18 9a9 9 0 0 1-9 9\"/>"
        case .github: "<path d=\"M15 22v-4a4.8 4.8 0 0 0-1-3.5c3 0 6-2 6-5.5.08-1.25-.27-2.48-1-3.5.28-1.15.28-2.35 0-3.5 0 0-1 0-3 1.5-2.64-.5-5.36-.5-8 0C6 2 5 2 5 2c-.3 1.15-.3 2.35 0 3.5A5.403 5.403 0 0 0 4 9c0 3.5 3 5.5 6 5.5-.39.49-.68 1.05-.85 1.65-.17.6-.22 1.23-.15 1.85v4\"/><path d=\"M9 18c-4.51 2-5-2-7-2\"/>"
        case .globe: "<circle cx=\"12\" cy=\"12\" r=\"10\"/><path d=\"M12 2a14.5 14.5 0 0 0 0 20 14.5 14.5 0 0 0 0-20\"/><path d=\"M2 12h20\"/>"
        case .grip: "<circle cx=\"9\" cy=\"12\" r=\"1\"/><circle cx=\"9\" cy=\"5\" r=\"1\"/><circle cx=\"9\" cy=\"19\" r=\"1\"/><circle cx=\"15\" cy=\"12\" r=\"1\"/><circle cx=\"15\" cy=\"5\" r=\"1\"/><circle cx=\"15\" cy=\"19\" r=\"1\"/>"
        case .imagePlus: "<path d=\"M16 5h6\"/><path d=\"M19 2v6\"/><path d=\"M21 11.5V19a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h7.5\"/><path d=\"m21 15-3.086-3.086a2 2 0 0 0-2.828 0L6 21\"/><circle cx=\"9\" cy=\"9\" r=\"2\"/>"
        case .inbox: "<polyline points=\"22 12 16 12 14 15 10 15 8 12 2 12\"/><path d=\"M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z\"/>"
        case .info: "<circle cx=\"12\" cy=\"12\" r=\"10\"/><path d=\"M12 16v-4\"/><path d=\"M12 8h.01\"/>"
        case .keyboard: "<path d=\"M10 8h.01\"/><path d=\"M12 12h.01\"/><path d=\"M14 8h.01\"/><path d=\"M16 12h.01\"/><path d=\"M18 8h.01\"/><path d=\"M6 8h.01\"/><path d=\"M7 16h10\"/><path d=\"M8 12h.01\"/><rect width=\"20\" height=\"16\" x=\"2\" y=\"4\" rx=\"2\"/>"
        case .laptop: "<path d=\"M18 5a2 2 0 0 1 2 2v8.526a2 2 0 0 0 .212.897l1.068 2.127a1 1 0 0 1-.9 1.45H3.62a1 1 0 0 1-.9-1.45l1.068-2.127A2 2 0 0 0 4 15.526V7a2 2 0 0 1 2-2z\"/><path d=\"M20.054 15.987H3.946\"/>"
        case .listChecks: "<path d=\"M13 5h8\"/><path d=\"M13 12h8\"/><path d=\"M13 19h8\"/><path d=\"m3 17 2 2 4-4\"/><path d=\"m3 7 2 2 4-4\"/>"
        case .listFlat: "<path d=\"M3 5h.01\"/><path d=\"M3 12h.01\"/><path d=\"M3 19h.01\"/><path d=\"M8 5h13\"/><path d=\"M8 12h13\"/><path d=\"M8 19h13\"/>"
        case .listTree: "<path d=\"M8 5h13\"/><path d=\"M13 12h8\"/><path d=\"M13 19h8\"/><path d=\"M3 10a2 2 0 0 0 2 2h3\"/><path d=\"M3 5v12a2 2 0 0 0 2 2h3\"/>"
        case .lock: "<rect width=\"18\" height=\"11\" x=\"3\" y=\"11\" rx=\"2\" ry=\"2\"/><path d=\"M7 11V7a5 5 0 0 1 10 0v4\"/>"
        case .merge: "<circle cx=\"18\" cy=\"18\" r=\"3\"/><circle cx=\"6\" cy=\"6\" r=\"3\"/><path d=\"M6 21V9a9 9 0 0 0 9 9\"/>"
        case .messageCircle: "<path d=\"M2.992 16.342a2 2 0 0 1 .094 1.167l-1.065 3.29a1 1 0 0 0 1.236 1.168l3.413-.998a2 2 0 0 1 1.099.092 10 10 0 1 0-4.777-4.719\"/>"
        case .mic: "<path d=\"M12 19v3\"/><path d=\"M19 10v2a7 7 0 0 1-14 0v-2\"/><rect x=\"9\" y=\"2\" width=\"6\" height=\"13\" rx=\"3\"/>"
        case .minus: "<path d=\"M5 12h14\"/>"
        case .monitor: "<rect width=\"20\" height=\"14\" x=\"2\" y=\"3\" rx=\"2\"/><line x1=\"8\" x2=\"16\" y1=\"21\" y2=\"21\"/><line x1=\"12\" x2=\"12\" y1=\"17\" y2=\"21\"/>"
        case .monitorSmartphone: "<path d=\"M18 8V6a2 2 0 0 0-2-2H4a2 2 0 0 0-2 2v7a2 2 0 0 0 2 2h8\"/><path d=\"M10 19v-3.96 3.15\"/><path d=\"M7 19h5\"/><rect width=\"6\" height=\"10\" x=\"16\" y=\"12\" rx=\"2\"/>"
        case .ellipsis: "<circle cx=\"12\" cy=\"12\" r=\"1\"/><circle cx=\"19\" cy=\"12\" r=\"1\"/><circle cx=\"5\" cy=\"12\" r=\"1\"/>"
        case .package: "<path d=\"M11 21.73a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73z\"/><path d=\"M12 22V12\"/><polyline points=\"3.29 7 12 12 20.71 7\"/><path d=\"m7.5 4.27 9 5.15\"/>"
        case .pencil: "<path d=\"M21.174 6.812a1 1 0 0 0-3.986-3.987L3.842 16.174a2 2 0 0 0-.5.83l-1.321 4.352a.5.5 0 0 0 .623.622l4.353-1.32a2 2 0 0 0 .83-.497z\"/><path d=\"m15 5 4 4\"/>"
        case .phone: "<rect width=\"14\" height=\"20\" x=\"5\" y=\"2\" rx=\"2\" ry=\"2\"/><path d=\"M12 18h.01\"/>"
        case .pin: "<path d=\"M12 17v5\"/><path d=\"M9 10.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24V16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V7a1 1 0 0 1 1-1 2 2 0 0 0 0-4H8a2 2 0 0 0 0 4 1 1 0 0 1 1 1z\"/>"
        case .play: "<path d=\"M5 5a2 2 0 0 1 3.008-1.728l11.997 6.998a2 2 0 0 1 .003 3.458l-12 7A2 2 0 0 1 5 19z\"/>"
        case .plus: "<path d=\"M5 12h14\"/><path d=\"M12 5v14\"/>"
        case .pointer: "<path d=\"M22 14a8 8 0 0 1-8 8\"/><path d=\"M18 11v-1a2 2 0 0 0-2-2a2 2 0 0 0-2 2\"/><path d=\"M14 10V9a2 2 0 0 0-2-2a2 2 0 0 0-2 2v1\"/><path d=\"M10 9.5V4a2 2 0 0 0-2-2a2 2 0 0 0-2 2v10\"/><path d=\"M18 11a2 2 0 1 1 4 0v3a8 8 0 0 1-8 8h-2c-2.8 0-4.5-.86-5.99-2.34l-3.6-3.6a2 2 0 0 1 2.83-2.82L7 15\"/>"
        case .prompt: "<path d=\"M12 19h8\"/><path d=\"m4 17 6-6-6-6\"/>"
        case .pullRequest: "<circle cx=\"18\" cy=\"18\" r=\"3\"/><circle cx=\"6\" cy=\"6\" r=\"3\"/><path d=\"M13 6h3a2 2 0 0 1 2 2v7\"/><line x1=\"6\" x2=\"6\" y1=\"9\" y2=\"21\"/>"
        case .pullRequestClosed: "<path d=\"m15.5 3.5 5 5\"/><path d=\"m15.5 8.5 5-5\"/><path d=\"M18 11.62V15\"/><path d=\"M6 9v12\"/><circle cx=\"18\" cy=\"18\" r=\"3\"/><circle cx=\"6\" cy=\"6\" r=\"3\"/>"
        case .pullRequestDraft: "<circle cx=\"18\" cy=\"18\" r=\"3\"/><circle cx=\"6\" cy=\"6\" r=\"3\"/><path d=\"M18 6V5\"/><path d=\"M18 11v-1\"/><line x1=\"6\" x2=\"6\" y1=\"9\" y2=\"21\"/>"
        case .reload: "<path d=\"M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8\"/><path d=\"M21 3v5h-5\"/><path d=\"M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16\"/><path d=\"M8 16H3v5\"/>"
        case .reply: "<path d=\"M20 18v-2a4 4 0 0 0-4-4H4\"/><path d=\"m9 17-5-5 5-5\"/>"
        case .rotateCw: "<path d=\"M21 12a9 9 0 1 1-9-9c2.52 0 4.93 1 6.74 2.74L21 8\"/><path d=\"M21 3v5h-5\"/>"
        case .runCancelled: "<circle cx=\"12\" cy=\"12\" r=\"10\"/><path d=\"M4.929 4.929 19.07 19.071\"/>"
        case .runSkipped: "<circle cx=\"12\" cy=\"12\" r=\"10\"/><path d=\"M8 12h8\"/>"
        case .scanQrCode: "<path d=\"M17 12v4a1 1 0 0 1-1 1h-4\"/><path d=\"M17 3h2a2 2 0 0 1 2 2v2\"/><path d=\"M17 8V7\"/><path d=\"M21 17v2a2 2 0 0 1-2 2h-2\"/><path d=\"M3 7V5a2 2 0 0 1 2-2h2\"/><path d=\"M7 17h.01\"/><path d=\"M7 21H5a2 2 0 0 1-2-2v-2\"/><rect x=\"7\" y=\"7\" width=\"5\" height=\"5\" rx=\"1\"/>"
        case .search: "<path d=\"m21 21-4.34-4.34\"/><circle cx=\"11\" cy=\"11\" r=\"8\"/>"
        case .send: "<path d=\"M14.536 21.686a.5.5 0 0 0 .937-.024l6.5-19a.496.496 0 0 0-.635-.635l-19 6.5a.5.5 0 0 0-.024.937l7.93 3.18a2 2 0 0 1 1.112 1.11z\"/><path d=\"m21.854 2.147-10.94 10.939\"/>"
        case .server: "<rect width=\"20\" height=\"8\" x=\"2\" y=\"2\" rx=\"2\" ry=\"2\"/><rect width=\"20\" height=\"8\" x=\"2\" y=\"14\" rx=\"2\" ry=\"2\"/><line x1=\"6\" x2=\"6.01\" y1=\"6\" y2=\"6\"/><line x1=\"6\" x2=\"6.01\" y1=\"18\" y2=\"18\"/>"
        case .sessions: "<path d=\"M2.992 16.342a2 2 0 0 1 .094 1.167l-1.065 3.29a1 1 0 0 0 1.236 1.168l3.413-.998a2 2 0 0 1 1.099.092 10 10 0 1 0-4.777-4.719\"/><path d=\"M8 12h.01\"/><path d=\"M12 12h.01\"/><path d=\"M16 12h.01\"/>"
        case .settings: "<path d=\"M9.671 4.136a2.34 2.34 0 0 1 4.659 0 2.34 2.34 0 0 0 3.319 1.915 2.34 2.34 0 0 1 2.33 4.033 2.34 2.34 0 0 0 0 3.831 2.34 2.34 0 0 1-2.33 4.033 2.34 2.34 0 0 0-3.319 1.915 2.34 2.34 0 0 1-4.659 0 2.34 2.34 0 0 0-3.32-1.915 2.34 2.34 0 0 1-2.33-4.033 2.34 2.34 0 0 0 0-3.831A2.34 2.34 0 0 1 6.35 6.051a2.34 2.34 0 0 0 3.319-1.915\"/><circle cx=\"12\" cy=\"12\" r=\"3\"/>"
        case .shield: "<path d=\"M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z\"/>"
        case .shieldAlert: "<path d=\"M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z\"/><path d=\"M12 8v4\"/><path d=\"M12 16h.01\"/>"
        case .sidebar: "<rect width=\"18\" height=\"18\" x=\"3\" y=\"3\" rx=\"2\"/><path d=\"M9 3v18\"/>"
        case .sidebarCollapse: "<rect width=\"18\" height=\"18\" x=\"3\" y=\"3\" rx=\"2\"/><path d=\"M9 3v18\"/><path d=\"m16 15-3-3 3-3\"/>"
        case .sidebarExpand: "<rect width=\"18\" height=\"18\" x=\"3\" y=\"3\" rx=\"2\"/><path d=\"M9 3v18\"/><path d=\"m14 9 3 3-3 3\"/>"
        case .sliders: "<path d=\"M10 5H3\"/><path d=\"M12 19H3\"/><path d=\"M14 3v4\"/><path d=\"M16 17v4\"/><path d=\"M21 12h-9\"/><path d=\"M21 19h-5\"/><path d=\"M21 5h-7\"/><path d=\"M8 10v4\"/><path d=\"M8 12H3\"/>"
        case .sparkle: "<path d=\"M11.017 2.814a1 1 0 0 1 1.966 0l1.051 5.558a2 2 0 0 0 1.594 1.594l5.558 1.051a1 1 0 0 1 0 1.966l-5.558 1.051a2 2 0 0 0-1.594 1.594l-1.051 5.558a1 1 0 0 1-1.966 0l-1.051-5.558a2 2 0 0 0-1.594-1.594l-5.558-1.051a1 1 0 0 1 0-1.966l5.558-1.051a2 2 0 0 0 1.594-1.594z\"/>"
        case .splitDown: "<rect width=\"18\" height=\"18\" x=\"3\" y=\"3\" rx=\"2\"/><path d=\"M3 12h18\"/>"
        case .splitRight: "<rect width=\"18\" height=\"18\" x=\"3\" y=\"3\" rx=\"2\"/><path d=\"M12 3v18\"/>"
        case .star: "<path d=\"M11.525 2.295a.53.53 0 0 1 .95 0l2.31 4.679a2.123 2.123 0 0 0 1.595 1.16l5.166.756a.53.53 0 0 1 .294.904l-3.736 3.638a2.123 2.123 0 0 0-.611 1.878l.882 5.14a.53.53 0 0 1-.771.56l-4.618-2.428a2.122 2.122 0 0 0-1.973 0L6.396 21.01a.53.53 0 0 1-.77-.56l.881-5.139a2.122 2.122 0 0 0-.611-1.879L2.16 9.795a.53.53 0 0 1 .294-.906l5.165-.755a2.122 2.122 0 0 0 1.597-1.16z\"/>"
        case .stop: "<rect width=\"18\" height=\"18\" x=\"3\" y=\"3\" rx=\"2\"/>"
        case .terminal: "<path d=\"m7 11 2-2-2-2\"/><path d=\"M11 13h4\"/><rect width=\"18\" height=\"18\" x=\"3\" y=\"3\" rx=\"2\" ry=\"2\"/>"
        case .textWrap: "<path d=\"m16 16-3 3 3 3\"/><path d=\"M3 12h14.5a1 1 0 0 1 0 7H13\"/><path d=\"M3 19h6\"/><path d=\"M3 5h18\"/>"
        case .trash: "<path d=\"M10 11v6\"/><path d=\"M14 11v6\"/><path d=\"M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6\"/><path d=\"M3 6h18\"/><path d=\"M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2\"/>"
        case .unfold: "<path d=\"m7 15 5 5 5-5\"/><path d=\"m7 9 5-5 5 5\"/>"
        case .unlink: "<path d=\"m18.84 12.25 1.72-1.71h-.02a5.004 5.004 0 0 0-.12-7.07 5.006 5.006 0 0 0-6.95 0l-1.72 1.71\"/><path d=\"m5.17 11.75-1.71 1.71a5.004 5.004 0 0 0 .12 7.07 5.006 5.006 0 0 0 6.95 0l1.71-1.71\"/><line x1=\"8\" x2=\"8\" y1=\"2\" y2=\"5\"/><line x1=\"2\" x2=\"5\" y1=\"8\" y2=\"8\"/><line x1=\"16\" x2=\"16\" y1=\"19\" y2=\"22\"/><line x1=\"19\" x2=\"22\" y1=\"16\" y2=\"16\"/>"
        case .upload: "<path d=\"M12 3v12\"/><path d=\"m17 8-5-5-5 5\"/><path d=\"M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4\"/>"
        case .volume: "<path d=\"M11 4.702a.705.705 0 0 0-1.203-.498L6.413 7.587A1.4 1.4 0 0 1 5.416 8H3a1 1 0 0 0-1 1v6a1 1 0 0 0 1 1h2.416a1.4 1.4 0 0 1 .997.413l3.383 3.384A.705.705 0 0 0 11 19.298z\"/><path d=\"M16 9a5 5 0 0 1 0 6\"/><path d=\"M19.364 18.364a9 9 0 0 0 0-12.728\"/>"
        case .triangleAlert: "<path d=\"m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3\"/><path d=\"M12 9v4\"/><path d=\"M12 17h.01\"/>"
        case .wifi: "<path d=\"M12 20h.01\"/><path d=\"M2 8.82a15 15 0 0 1 20 0\"/><path d=\"M5 12.859a10 10 0 0 1 14 0\"/><path d=\"M8.5 16.429a5 5 0 0 1 7 0\"/>"
        case .worktree: "<circle cx=\"12\" cy=\"18\" r=\"3\"/><circle cx=\"6\" cy=\"6\" r=\"3\"/><circle cx=\"18\" cy=\"6\" r=\"3\"/><path d=\"M18 9v2c0 .6-.4 1-1 1H7c-.6 0-1-.4-1-1V9\"/><path d=\"M12 12v3\"/>"
        case .close: "<path d=\"M18 6 6 18\"/><path d=\"m6 6 12 12\"/>"
        }
    }
}
