"""Typecheck AnywhereKit and the App, Widgets and NotificationService targets as iOS code, without an iOS SDK.

Mac Catalyst compiles with os(iOS) true, and the Command Line Tools' macOS SDK ships its iOSSupport frameworks
(UIKit, SwiftUI for iOS, WidgetKit, AVFoundation, AppIntents, UserNotifications). That SDK is iOS 17.2-era, so:
- ActivityKit isn't there: `stubs/ActivityKit` declares what the app uses, copied from developer.apple.com.
- iOS 18/26 API and WidgetKit's Live Activity types aren't there: `stubs/gaps` declares those in use.
- Its SwiftUI interface doesn't build with Swift 6.2 (one os_log interpolation), so a patched copy is used, and the
  toolchain's own stdlib interfaces are retargeted to Catalyst so 6.2 macros (Observable) and typed throws resolve.
- Its SwiftUI predates View being @MainActor, so isolation diagnostics can differ from Xcode's.

Each module emits a body-skipped .swiftmodule for the next, then typechecks fully. Exits 1 on any error.

Usage: python3 -I typecheck.py <scratch path of a finished `swift build`>
"""
import glob, json, os, re, shutil, subprocess, sys

HERE = os.path.dirname(os.path.abspath(__file__))
IOS = os.path.dirname(os.path.dirname(HERE))
KIT = IOS + "/Packages/AnywhereKit"
SDK = "/Library/Developer/CommandLineTools/SDKs/MacOSX14.2.sdk"
IOS_FRAMEWORKS = SDK + "/System/iOSSupport/System/Library/Frameworks"
TARGET = "arm64-apple-ios17.2-macabi"

scratch = os.path.abspath(sys.argv[1])
work = scratch + "/ios-typecheck"
mods = work + "/modules"
os.makedirs(mods, exist_ok=True)
swiftc = shutil.which("swiftc")
checkouts = scratch + "/checkouts"


def files(d):
    return sorted(glob.glob(d + "/**/*.swift", recursive=True))


def patched_swiftui():
    """The iOSSupport SwiftUI with the one interpolation Swift 6.2 rejects removed."""
    dst = work + "/frameworks/SwiftUI.framework"
    if not os.path.exists(dst):
        shutil.copytree(IOS_FRAMEWORKS + "/SwiftUI.framework", dst, symlinks=True)
        iface = dst + "/Modules/SwiftUI.swiftmodule/arm64e-apple-ios-macabi.swiftinterface"
        text = open(iface).read().replace(r"Accessing Environment<\(Value.self)>", "Accessing Environment<Value>")
        os.chmod(iface, 0o644)
        open(iface, "w").write(text)
    return work + "/frameworks"


def catalyst_stdlib():
    """The toolchain's macOS stdlib interfaces, retargeted to Catalyst."""
    info = json.loads(subprocess.run([swiftc, "-print-target-info"], capture_output=True, text=True, check=True).stdout)
    lib = info["paths"]["runtimeResourcePath"] + "/macosx"
    dst = work + "/stdlib"
    if not os.path.exists(dst):
        for mod in glob.glob(lib + "/*.swiftmodule"):
            iface = mod + "/arm64-apple-macos.swiftinterface"
            if os.path.exists(iface):
                out = dst + "/" + os.path.basename(mod)
                os.makedirs(out)
                text = re.sub(r"-target arm64-apple-macosx?[0-9.]*", "-target " + TARGET, open(iface).read(), count=1)
                open(out + "/arm64-apple-ios-macabi.swiftinterface", "w").write(text)
    return dst


def package_targets():
    """AnywhereKit's library targets in dependency order."""
    desc = json.loads(subprocess.run(["swift", "package", "--scratch-path", scratch, "describe", "--type", "json"], cwd=KIT,
                                     capture_output=True, text=True, check=True).stdout)
    libs = {t["name"]: t.get("target_dependencies", []) for t in desc["targets"] if t["type"] == "library"}
    order, seen = [], set()

    def visit(name):
        if name in seen:
            return
        seen.add(name)
        for dep in libs[name]:
            visit(dep)
        order.append(name)

    for name in sorted(libs):
        visit(name)
    return order


common = [
    swiftc, "-swift-version", "6", "-target", TARGET, "-Xfrontend", "-disable-availability-checking", "-sdk", SDK,
    "-I", catalyst_stdlib(), "-module-cache-path", work + "/module-cache",
    "-Fsystem", patched_swiftui(), "-Fsystem", IOS_FRAMEWORKS, "-I", SDK + "/System/iOSSupport/usr/lib/swift", "-I", mods,
    "-Xcc", "-I" + checkouts + "/swift-cmark/src/include", "-Xcc", "-I" + checkouts + "/swift-cmark/extensions/include",
    "-parse-as-library",
]

gaps = {
    "DesignSystem": ["Glass26.swift"],
    "FeatureChat": ["Photos.swift", "ScrollAnchor18.swift"],
    "LiveActivityUI": ["LiveActivityWidgetKit.swift"],
}

modules = [("ActivityKit", files(HERE + "/stubs/ActivityKit"), ["-swift-version", "5"]),
           ("SwiftTerm", files(checkouts + "/SwiftTerm/Sources/SwiftTerm") + [HERE + "/stubs/SwiftTermGenerated.swift"], ["-swift-version", "5"])]
modules += [(name, files(KIT + "/Sources/" + name) + [HERE + "/stubs/gaps/" + g for g in gaps.get(name, [])], [])
            for name in package_targets()]
apps = [("Anywhere", files(IOS + "/App")), ("AnywhereWidgets", files(IOS + "/Widgets")), ("AnywhereNotify", files(IOS + "/NotificationService"))]

errors = 0
for name, sources, flags in modules + [(n, s, []) for n, s in apps]:
    ours = name not in ("ActivityKit", "SwiftTerm")
    cmd = common + flags + ["-module-name", name] + sources
    if name not in dict(apps):
        emit = subprocess.run(cmd + ["-emit-module", "-emit-module-path", mods + "/" + name + ".swiftmodule",
                                     "-Xfrontend", "-experimental-skip-all-function-bodies"], capture_output=True, text=True)
        if emit.returncode != 0 and not ours:
            sys.exit(emit.stderr)
    check = subprocess.run(cmd + ["-typecheck"], capture_output=True, text=True)
    found = [l.replace(IOS + "/", "") for l in check.stderr.splitlines() if re.search(r": (error|warning):", l)]
    shown = found if ours else [l for l in found if ": error:" in l]
    errors += sum(": error:" in l for l in found)
    print(("ok   " if check.returncode == 0 else "FAIL ") + name)
    for line in dict.fromkeys(shown):
        print("     " + line)
sys.exit(1 if errors else 0)
