import AppKit
import CoreGraphics
import Foundation

let root = URL(fileURLWithPath: FileManager.default.currentDirectoryPath)
let iconDir = root.appendingPathComponent("icon", isDirectory: true)
let sourceURL = iconDir.appendingPathComponent("typetext-fresh.png")
let appIconURL = iconDir.appendingPathComponent("typetext-appicon.png")
let iconsetURL = iconDir.appendingPathComponent("TypeText.iconset", isDirectory: true)
let icnsURL = iconDir.appendingPathComponent("TypeText.icns")
let icoURL = iconDir.appendingPathComponent("TypeText.ico")

guard
    let sourceImage = NSImage(contentsOf: sourceURL),
    let sourceCg = sourceImage.cgImage(forProposedRect: nil, context: nil, hints: nil)
else {
    fputs("Could not read \(sourceURL.path)\n", stderr)
    exit(1)
}

let canvasSize = 1024
let iconSize = 900
let iconOrigin = (canvasSize - iconSize) / 2
let radius: CGFloat = 185

guard
    let colorSpace = CGColorSpace(name: CGColorSpace.sRGB),
    let context = CGContext(
        data: nil,
        width: canvasSize,
        height: canvasSize,
        bitsPerComponent: 8,
        bytesPerRow: 0,
        space: colorSpace,
        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
    )
else {
    fputs("Could not create icon context\n", stderr)
    exit(1)
}

context.clear(CGRect(x: 0, y: 0, width: canvasSize, height: canvasSize))
context.saveGState()

let roundedRect = CGRect(x: iconOrigin, y: iconOrigin, width: iconSize, height: iconSize)
let roundedPath = CGPath(roundedRect: roundedRect, cornerWidth: radius, cornerHeight: radius, transform: nil)
context.addPath(roundedPath)
context.clip()

let sourceWidth = CGFloat(sourceCg.width)
let sourceHeight = CGFloat(sourceCg.height)
let cropInset = min(sourceWidth, sourceHeight) * 0.075
let sourceCrop = CGRect(
    x: cropInset,
    y: cropInset,
    width: sourceWidth - cropInset * 2,
    height: sourceHeight - cropInset * 2
)

if let cropped = sourceCg.cropping(to: sourceCrop) {
    let cropAspect = CGFloat(cropped.width) / CGFloat(cropped.height)
    let targetAspect = CGFloat(iconSize) / CGFloat(iconSize)
    var drawRect = roundedRect

    if cropAspect > targetAspect {
        let scaledWidth = CGFloat(iconSize) * cropAspect
        drawRect = CGRect(
            x: CGFloat(iconOrigin) - (scaledWidth - CGFloat(iconSize)) / 2,
            y: CGFloat(iconOrigin),
            width: scaledWidth,
            height: CGFloat(iconSize)
        )
    } else {
        let scaledHeight = CGFloat(iconSize) / cropAspect
        drawRect = CGRect(
            x: CGFloat(iconOrigin),
            y: CGFloat(iconOrigin) - (scaledHeight - CGFloat(iconSize)) / 2,
            width: CGFloat(iconSize),
            height: scaledHeight
        )
    }

    context.draw(cropped, in: drawRect)
}

context.restoreGState()

guard let iconCg = context.makeImage() else {
    fputs("Could not render icon image\n", stderr)
    exit(1)
}

func writePNG(_ image: CGImage, to url: URL) throws {
    let bitmap = NSBitmapImageRep(cgImage: image)
    guard let data = bitmap.representation(using: .png, properties: [:]) else {
        throw NSError(domain: "TypeTextIcon", code: 1, userInfo: [NSLocalizedDescriptionKey: "Could not encode PNG"])
    }
    try data.write(to: url, options: .atomic)
}

func resized(_ image: CGImage, to size: Int) -> CGImage? {
    guard
        let colorSpace = CGColorSpace(name: CGColorSpace.sRGB),
        let context = CGContext(
            data: nil,
            width: size,
            height: size,
            bitsPerComponent: 8,
            bytesPerRow: 0,
            space: colorSpace,
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
        )
    else {
        return nil
    }

    context.interpolationQuality = .high
    context.clear(CGRect(x: 0, y: 0, width: size, height: size))
    context.draw(image, in: CGRect(x: 0, y: 0, width: size, height: size))
    return context.makeImage()
}

try writePNG(iconCg, to: appIconURL)

try? FileManager.default.removeItem(at: iconsetURL)
try FileManager.default.createDirectory(at: iconsetURL, withIntermediateDirectories: true)

let iconsetEntries: [(String, Int)] = [
    ("icon_16x16.png", 16),
    ("icon_16x16@2x.png", 32),
    ("icon_32x32.png", 32),
    ("icon_32x32@2x.png", 64),
    ("icon_128x128.png", 128),
    ("icon_128x128@2x.png", 256),
    ("icon_256x256.png", 256),
    ("icon_256x256@2x.png", 512),
    ("icon_512x512.png", 512),
    ("icon_512x512@2x.png", 1024),
]

for (filename, size) in iconsetEntries {
    guard let output = resized(iconCg, to: size) else {
        fputs("Could not resize icon to \(size)x\(size)\n", stderr)
        exit(1)
    }
    try writePNG(output, to: iconsetURL.appendingPathComponent(filename))
}

func appendBigEndianUInt32(_ value: UInt32, to data: inout Data) {
    var bigEndian = value.bigEndian
    withUnsafeBytes(of: &bigEndian) { data.append(contentsOf: $0) }
}

func appendIcnsChunk(type: String, pngURL: URL, to data: inout Data) throws {
    let png = try Data(contentsOf: pngURL)
    guard let typeData = type.data(using: .ascii), typeData.count == 4 else {
        throw NSError(domain: "TypeTextIcon", code: 2, userInfo: [NSLocalizedDescriptionKey: "Invalid ICNS type \(type)"])
    }

    data.append(typeData)
    appendBigEndianUInt32(UInt32(png.count + 8), to: &data)
    data.append(png)
}

let icnsChunks: [(String, String)] = [
    ("icp4", "icon_16x16.png"),
    ("icp5", "icon_32x32.png"),
    ("icp6", "icon_32x32@2x.png"),
    ("ic07", "icon_128x128.png"),
    ("ic08", "icon_256x256.png"),
    ("ic09", "icon_512x512.png"),
    ("ic10", "icon_512x512@2x.png"),
]

var body = Data()
for (type, filename) in icnsChunks {
    try appendIcnsChunk(type: type, pngURL: iconsetURL.appendingPathComponent(filename), to: &body)
}

var icns = Data()
icns.append("icns".data(using: .ascii)!)
appendBigEndianUInt32(UInt32(body.count + 8), to: &icns)
icns.append(body)
try icns.write(to: icnsURL, options: .atomic)

func appendLittleEndianUInt16(_ value: UInt16, to data: inout Data) {
    var littleEndian = value.littleEndian
    withUnsafeBytes(of: &littleEndian) { data.append(contentsOf: $0) }
}

func appendLittleEndianUInt32(_ value: UInt32, to data: inout Data) {
    var littleEndian = value.littleEndian
    withUnsafeBytes(of: &littleEndian) { data.append(contentsOf: $0) }
}

let icoSizes = [16, 24, 32, 48, 64, 128, 256]
var icoImages: [(size: Int, png: Data)] = []
for size in icoSizes {
    guard let output = resized(iconCg, to: size) else {
        fputs("Could not resize Windows icon to \(size)x\(size)\n", stderr)
        exit(1)
    }

    let bitmap = NSBitmapImageRep(cgImage: output)
    guard let png = bitmap.representation(using: .png, properties: [:]) else {
        fputs("Could not encode Windows icon PNG at \(size)x\(size)\n", stderr)
        exit(1)
    }
    icoImages.append((size, png))
}

var ico = Data()
appendLittleEndianUInt16(0, to: &ico)
appendLittleEndianUInt16(1, to: &ico)
appendLittleEndianUInt16(UInt16(icoImages.count), to: &ico)

let directorySize = 6 + icoImages.count * 16
var imageOffset = directorySize
var imageBody = Data()

for image in icoImages {
    ico.append(UInt8(image.size == 256 ? 0 : image.size))
    ico.append(UInt8(image.size == 256 ? 0 : image.size))
    ico.append(0)
    ico.append(0)
    appendLittleEndianUInt16(1, to: &ico)
    appendLittleEndianUInt16(32, to: &ico)
    appendLittleEndianUInt32(UInt32(image.png.count), to: &ico)
    appendLittleEndianUInt32(UInt32(imageOffset), to: &ico)

    imageBody.append(image.png)
    imageOffset += image.png.count
}

ico.append(imageBody)
try ico.write(to: icoURL, options: .atomic)

print("Generated \(appIconURL.path)")
print("Generated \(iconsetURL.path)")
print("Generated \(icnsURL.path)")
print("Generated \(icoURL.path)")
