// Renders the app icon: two overlapping rings on a dark squircle. Usage: swift scripts/icon.swift out.png
import CoreGraphics
import Foundation
import ImageIO
import UniformTypeIdentifiers

let W = 1024
let cs = CGColorSpace(name: CGColorSpace.sRGB)!
let ctx = CGContext(data: nil, width: W, height: W, bitsPerComponent: 8, bytesPerRow: 0, space: cs,
                    bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
func c(_ hex: UInt32, _ a: CGFloat = 1) -> CGColor {
  CGColor(srgbRed: CGFloat((hex >> 16) & 255) / 255, green: CGFloat((hex >> 8) & 255) / 255, blue: CGFloat(hex & 255) / 255, alpha: a)
}
ctx.translateBy(x: 0, y: CGFloat(W)); ctx.scaleBy(x: 1, y: -1)

// macOS icon grid: 824×824 rounded square, subtle shadow, flat dark fill.
let body = CGPath(roundedRect: CGRect(x: 100, y: 100, width: 824, height: 824), cornerWidth: 186, cornerHeight: 186, transform: nil)
ctx.saveGState()
ctx.setShadow(offset: CGSize(width: 0, height: 12), blur: 28, color: c(0x000000, 0.35))
ctx.addPath(body); ctx.setFillColor(c(0x17171f)); ctx.fillPath()
ctx.restoreGState()
// hairline inner border
ctx.addPath(CGPath(roundedRect: CGRect(x: 101.5, y: 101.5, width: 821, height: 821), cornerWidth: 185, cornerHeight: 185, transform: nil))
ctx.setStrokeColor(c(0xffffff, 0.08)); ctx.setLineWidth(3); ctx.strokePath()

// Two rings: "peer" (white) and "lens" (accent).
let r: CGFloat = 196, y: CGFloat = 512, dx: CGFloat = 118
ctx.setLineWidth(46)
ctx.setStrokeColor(c(0xe7e8f1))
ctx.addEllipse(in: CGRect(x: 512 - dx - r, y: y - r, width: 2 * r, height: 2 * r)); ctx.strokePath()
ctx.setStrokeColor(c(0x8d9bff))
ctx.addEllipse(in: CGRect(x: 512 + dx - r, y: y - r, width: 2 * r, height: 2 * r)); ctx.strokePath()

let img = ctx.makeImage()!
let dest = CGImageDestinationCreateWithURL(URL(fileURLWithPath: CommandLine.arguments[1]) as CFURL, UTType.png.identifier as CFString, 1, nil)!
CGImageDestinationAddImage(dest, img, nil)
CGImageDestinationFinalize(dest)
