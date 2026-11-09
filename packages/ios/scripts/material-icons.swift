#!/usr/bin/env swift
/* Renders the desktop's Material Icon Theme SVGs to 48-pixel PNGs, since iOS can't draw an SVG file at runtime:
     swift scripts/material-icons.swift   (from packages/ios) */
import AppKit

let from = URL(fileURLWithPath: "../desktop/crates/theme/assets/material")
let to = URL(fileURLWithPath: "Packages/AnywhereKit/Sources/FileIcons/Resources/material")
let side = 48
try FileManager.default.createDirectory(at: to, withIntermediateDirectories: true)
var failed = 0
for file in try FileManager.default.contentsOfDirectory(at: from, includingPropertiesForKeys: nil) where file.pathExtension == "svg" {
    guard let image = NSImage(contentsOf: file),
        let rep = NSBitmapImageRep(
            bitmapDataPlanes: nil, pixelsWide: side, pixelsHigh: side, bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
            colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)
    else {
        print("couldn't read \(file.lastPathComponent)")
        failed += 1
        continue
    }
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
    image.draw(in: NSRect(x: 0, y: 0, width: side, height: side))
    NSGraphicsContext.restoreGraphicsState()
    let png = rep.representation(using: .png, properties: [:])!
    try png.write(to: to.appendingPathComponent(file.deletingPathExtension().lastPathComponent + ".png"))
}
for name in ["icons.json", "LICENSE"] {
    try? FileManager.default.removeItem(at: to.appendingPathComponent(name))
    try FileManager.default.copyItem(at: from.appendingPathComponent(name), to: to.appendingPathComponent(name))
}
print("done, \(failed) failed")
