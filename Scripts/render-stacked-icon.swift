import AppKit
import CoreGraphics
import Foundation

let root = URL(fileURLWithPath: FileManager.default.currentDirectoryPath)
let outputURL = root.appendingPathComponent("icon/typetext-fresh.png")

let size = 1254
let canvas = CGRect(x: 0, y: 0, width: size, height: size)

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
    fputs("Could not create icon context\n", stderr)
    exit(1)
}

context.clear(canvas)
context.setShouldAntialias(true)
context.interpolationQuality = .high

func color(_ hex: UInt32, alpha: CGFloat = 1) -> CGColor {
    let red = CGFloat((hex >> 16) & 0xff) / 255
    let green = CGFloat((hex >> 8) & 0xff) / 255
    let blue = CGFloat(hex & 0xff) / 255
    return CGColor(srgbRed: red, green: green, blue: blue, alpha: alpha)
}

func fillRoundedRect(
    _ rect: CGRect,
    radius: CGFloat,
    colors: [CGColor],
    start: CGPoint = CGPoint(x: 0, y: 0),
    end: CGPoint = CGPoint(x: 1, y: 1),
    shadow: (offset: CGSize, blur: CGFloat, color: CGColor)? = nil,
    stroke: CGColor? = nil,
    strokeWidth: CGFloat = 0
) {
    let path = CGPath(roundedRect: rect, cornerWidth: radius, cornerHeight: radius, transform: nil)
    context.saveGState()
    if let shadow {
        context.setShadow(offset: shadow.offset, blur: shadow.blur, color: shadow.color)
    }
    context.addPath(path)
    context.clip()
    let gradient = CGGradient(colorsSpace: colorSpace, colors: colors as CFArray, locations: nil)!
    context.drawLinearGradient(
        gradient,
        start: CGPoint(x: rect.minX + rect.width * start.x, y: rect.minY + rect.height * start.y),
        end: CGPoint(x: rect.minX + rect.width * end.x, y: rect.minY + rect.height * end.y),
        options: []
    )
    context.restoreGState()

    if let stroke {
        context.saveGState()
        context.addPath(path)
        context.setStrokeColor(stroke)
        context.setLineWidth(strokeWidth)
        context.strokePath()
        context.restoreGState()
    }
}

func drawGloss(_ rect: CGRect, radius: CGFloat) {
    let path = CGPath(roundedRect: rect.insetBy(dx: 12, dy: 12), cornerWidth: radius - 10, cornerHeight: radius - 10, transform: nil)
    context.saveGState()
    context.addPath(path)
    context.clip()
    let gradient = CGGradient(
        colorsSpace: colorSpace,
        colors: [
            color(0xffffff, alpha: 0.30),
            color(0xffffff, alpha: 0.08),
            color(0xffffff, alpha: 0.0),
        ] as CFArray,
        locations: [0.0, 0.42, 1.0]
    )!
    context.drawLinearGradient(
        gradient,
        start: CGPoint(x: rect.minX, y: rect.maxY),
        end: CGPoint(x: rect.minX, y: rect.midY),
        options: []
    )
    context.restoreGState()
}

func drawSymbol(_ text: String, rect: CGRect, fontSize: CGFloat, weight: NSFont.Weight = .heavy) {
    let paragraph = NSMutableParagraphStyle()
    paragraph.alignment = .center
    let attrs: [NSAttributedString.Key: Any] = [
        .font: NSFont.monospacedSystemFont(ofSize: fontSize, weight: weight),
        .foregroundColor: NSColor.white,
        .paragraphStyle: paragraph,
        .shadow: {
            let shadow = NSShadow()
            shadow.shadowOffset = CGSize(width: 0, height: -4)
            shadow.shadowBlurRadius = 7
            shadow.shadowColor = NSColor.black.withAlphaComponent(0.36)
            return shadow
        }(),
    ]
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(cgContext: context, flipped: false)
    NSString(string: text).draw(with: rect, options: [.usesLineFragmentOrigin, .usesFontLeading], attributes: attrs)
    NSGraphicsContext.restoreGraphicsState()
}

func drawLine(_ rect: CGRect) {
    fillRoundedRect(
        rect,
        radius: rect.height / 2,
        colors: [color(0xffffff), color(0xeef5ff)],
        shadow: (CGSize(width: 0, height: -3), 5, color(0x000000, alpha: 0.30))
    )
}

func drawCursor() {
    let transform = CGAffineTransform(translationX: 875, y: 275).rotated(by: -0.18)
    let cursor = CGMutablePath()
    cursor.move(to: CGPoint(x: 0, y: 440), transform: transform)
    cursor.addLine(to: CGPoint(x: 255, y: 170), transform: transform)
    cursor.addLine(to: CGPoint(x: 142, y: 151), transform: transform)
    cursor.addLine(to: CGPoint(x: 240, y: 20), transform: transform)
    cursor.addLine(to: CGPoint(x: 170, y: -31), transform: transform)
    cursor.addLine(to: CGPoint(x: 74, y: 99), transform: transform)
    cursor.addLine(to: CGPoint(x: 5, y: 2), transform: transform)
    cursor.closeSubpath()

    context.saveGState()
    context.setShadow(offset: CGSize(width: 26, height: -30), blur: 34, color: color(0x000000, alpha: 0.42))
    context.addPath(cursor)
    context.setFillColor(color(0xf9fbff))
    context.fillPath()
    context.restoreGState()

    context.saveGState()
    context.addPath(cursor)
    context.setStrokeColor(color(0xd8deef))
    context.setLineWidth(11)
    context.setLineJoin(.round)
    context.strokePath()
    context.restoreGState()
}

let base = canvas
fillRoundedRect(
    base,
    radius: 0,
    colors: [color(0x196cff), color(0x8547ff), color(0xe44adb)],
    start: CGPoint(x: 0.15, y: 0.92),
    end: CGPoint(x: 0.94, y: 0.04)
)

let document = CGRect(x: 620, y: 210, width: 570, height: 630)
fillRoundedRect(
    document,
    radius: 30,
    colors: [color(0xf8fbff), color(0xdce5f9)],
    start: CGPoint(x: 0.2, y: 1),
    end: CGPoint(x: 0.9, y: 0),
    shadow: (CGSize(width: 24, height: -18), 28, color(0x000000, alpha: 0.24)),
    stroke: color(0xc9d5ec),
    strokeWidth: 3
)

for y in [695, 600, 505, 410, 315] as [CGFloat] {
    fillRoundedRect(
        CGRect(x: 690, y: y, width: y == 695 ? 420 : 470, height: 36),
        radius: 10,
        colors: [color(0xcdd8ef), color(0xb8c5df)],
        shadow: nil
    )
}

let orange = CGRect(x: 92, y: 130, width: 725, height: 325)
fillRoundedRect(
    orange,
    radius: 28,
    colors: [color(0xff8e00), color(0xff2b08)],
    start: CGPoint(x: 0.15, y: 1),
    end: CGPoint(x: 0.9, y: 0),
    shadow: (CGSize(width: 16, height: -18), 24, color(0x000000, alpha: 0.34)),
    stroke: color(0xffab1d, alpha: 0.72),
    strokeWidth: 6
)
drawGloss(orange, radius: 28)
drawSymbol("{  }", rect: orange.insetBy(dx: 86, dy: 62), fontSize: 180)

let green = CGRect(x: 150, y: 440, width: 805, height: 310)
fillRoundedRect(
    green,
    radius: 28,
    colors: [color(0x4bda22), color(0x0b861b)],
    start: CGPoint(x: 0.12, y: 1),
    end: CGPoint(x: 0.88, y: 0),
    shadow: (CGSize(width: 16, height: -18), 24, color(0x000000, alpha: 0.34)),
    stroke: color(0x54f238, alpha: 0.64),
    strokeWidth: 6
)
drawGloss(green, radius: 28)
drawLine(CGRect(x: 270, y: 620, width: 485, height: 32))
drawLine(CGRect(x: 270, y: 555, width: 370, height: 32))
drawLine(CGRect(x: 270, y: 492, width: 235, height: 32))

let blue = CGRect(x: 92, y: 700, width: 720, height: 325)
fillRoundedRect(
    blue,
    radius: 28,
    colors: [color(0x4196ff), color(0x0036c9)],
    start: CGPoint(x: 0.1, y: 1),
    end: CGPoint(x: 0.85, y: 0),
    shadow: (CGSize(width: 14, height: -18), 28, color(0x000000, alpha: 0.42)),
    stroke: color(0x66b7ff, alpha: 0.62),
    strokeWidth: 6
)
drawGloss(blue, radius: 28)
drawSymbol("</>", rect: blue.insetBy(dx: 95, dy: 55), fontSize: 165)

drawCursor()

guard let image = context.makeImage() else {
    fputs("Could not render icon image\n", stderr)
    exit(1)
}

let bitmap = NSBitmapImageRep(cgImage: image)
guard let data = bitmap.representation(using: .png, properties: [:]) else {
    fputs("Could not encode PNG\n", stderr)
    exit(1)
}

try data.write(to: outputURL, options: .atomic)
print("Generated \(outputURL.path)")
