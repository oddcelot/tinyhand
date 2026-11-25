import AppKit

enum TapEffectChoice: String, CaseIterable {
    case off, fairyDust, fairyDustSubtle

    var title: String {
        switch self {
        case .off: return "Off"
        case .fairyDust: return "Fairy dust"
        case .fairyDustSubtle: return "Fairy dust, subtle"
        }
    }
}

/// Short particle bursts played where the pointer taps. Each burst is its own emitter
/// layer that Core Animation runs by itself, so it keeps moving while the overlay idles.
final class TapEffect {
    let layer = CALayer()

    private static let burst: CFTimeInterval = 0.12
    init() {
        layer.masksToBounds = false
    }

    /// `unit` scales the burst with the screen and pointer size; 1 suits a 900 pt tall screen.
    func play(_ choice: TapEffectChoice, at point: CGPoint, unit: CGFloat) {
        switch choice {
        case .off:
            return
        case .fairyDust:
            emit(TapEffect.fairyDust(unit: unit, subtle: false), at: point, radius: 4 * unit)
        case .fairyDustSubtle:
            emit(TapEffect.fairyDust(unit: unit, subtle: true), at: point, radius: 3 * unit)
        }
    }

    func clear() {
        layer.sublayers?.forEach { $0.removeFromSuperlayer() }
    }

    private func emit(_ cells: [CAEmitterCell], at point: CGPoint, radius: CGFloat) {
        let emitter = CAEmitterLayer()
        emitter.emitterPosition = point
        emitter.emitterShape = .circle
        emitter.emitterSize = CGSize(width: radius * 2, height: radius * 2)
        emitter.emitterCells = cells
        // Without this a new emitter starts as if it had been running for a while.
        emitter.beginTime = layer.convertTime(CACurrentMediaTime(), from: nil)

        CATransaction.begin()
        CATransaction.setDisableActions(true)
        layer.addSublayer(emitter)
        CATransaction.commit()

        let longest = cells.map { Double($0.lifetime + $0.lifetimeRange) }.max() ?? 1
        DispatchQueue.main.asyncAfter(deadline: .now() + TapEffect.burst) {
            emitter.birthRate = 0
        }
        DispatchQueue.main.asyncAfter(deadline: .now() + TapEffect.burst + longest + 0.1) {
            emitter.removeFromSuperlayer()
        }
    }

    /// Jewel tones that stay visible on white and on dark.
    private static let hues: [(r: CGFloat, g: CGFloat, b: CGFloat)] = [
        (0.96, 0.30, 0.68),
        (0.60, 0.38, 0.98),
        (0.16, 0.62, 1.00),
        (0.10, 0.84, 0.70),
        (1.00, 0.70, 0.16),
    ]

    private static func tone(_ index: Int, _ alpha: CGFloat, lighten: CGFloat = 0) -> CGColor {
        let h = hues[index % hues.count]
        return CGColor(srgbRed: h.r + (1 - h.r) * lighten, green: h.g + (1 - h.g) * lighten,
                       blue: h.b + (1 - h.b) * lighten, alpha: alpha)
    }

    // One glass star and one glass bead per hue. The colour is painted into the sprite as a
    // gradient from that hue to the next, so the cells are left untinted: a tint would also
    // colour the white highlights that make the pieces look like glass.
    private static let stars = hues.indices.map { makeStar(hue: $0, pixels: 112) }
    private static let beads = hues.indices.map { makeBead(hue: $0, pixels: 56) }
    private static let ripple = makeRipple(pixels: 128)

    // Cells here must not have sub-emitters (`emitterCells`): with several such cells in one
    // layer Core Animation draws every particle as a solid square.
    private static func fairyDust(unit: CGFloat, subtle: Bool) -> [CAEmitterCell] {
        var cells: [CAEmitterCell] = []

        // A ring of light that opens from the fingertip.
        let ring = CAEmitterCell()
        ring.contents = ripple
        ring.contentsScale = 2
        ring.birthRate = Float(1 / burst)
        ring.lifetime = 0.5
        ring.scale = 0.15 * unit
        ring.scaleSpeed = 2.4 * unit
        ring.alphaSpeed = -2.0
        if !subtle {
            cells.append(ring)
        }

        for hue in hues.indices {
            // Glass stars that fly out and tumble, so their gradients turn in the light.
            let star = CAEmitterCell()
            star.contents = stars[hue]
            star.contentsScale = 2
            star.birthRate = Float(3 / burst)
            star.lifetime = 1.1
            star.lifetimeRange = 0.4
            star.velocity = 120 * unit
            star.velocityRange = 75 * unit
            star.emissionRange = 2 * .pi
            star.yAcceleration = -120 * unit
            star.scale = 0.42 * unit
            star.scaleRange = 0.2 * unit
            star.scaleSpeed = -0.24 * unit
            star.spinRange = 4
            star.alphaSpeed = -0.6
            cells.append(star)

            // Small glass beads thrown out with the stars.
            let dust = CAEmitterCell()
            dust.contents = beads[hue]
            dust.contentsScale = 2
            dust.birthRate = Float(10 / burst)
            dust.lifetime = 0.85
            dust.lifetimeRange = 0.4
            dust.velocity = 100 * unit
            dust.velocityRange = 90 * unit
            dust.emissionRange = 2 * .pi
            dust.yAcceleration = -130 * unit
            dust.scale = 0.2 * unit
            dust.scaleRange = 0.1 * unit
            dust.scaleSpeed = -0.1 * unit
            dust.spinRange = 3
            dust.alphaSpeed = -0.8
            cells.append(dust)

            // Bigger beads that hang in the air like bubbles after the stars have gone.
            let bubble = CAEmitterCell()
            bubble.contents = beads[hue]
            bubble.contentsScale = 2
            bubble.birthRate = Float(4 / burst)
            bubble.lifetime = 1.4
            bubble.lifetimeRange = 0.6
            bubble.velocity = 46 * unit
            bubble.velocityRange = 44 * unit
            bubble.emissionRange = 2 * .pi
            bubble.yAcceleration = -30 * unit
            bubble.scale = 0.42 * unit
            bubble.scaleRange = 0.22 * unit
            bubble.scaleSpeed = -0.12 * unit
            bubble.spinRange = 2
            bubble.alphaSpeed = -0.55
            cells.append(bubble)
        }

        if subtle {
            // The same pieces, but a handful of them: smaller, fainter, staying near the
            // fingertip and gone sooner.
            for cell in cells {
                cell.birthRate *= 0.3
                cell.velocity *= 0.55
                cell.velocityRange *= 0.55
                cell.yAcceleration *= 0.55
                cell.scale *= 0.72
                cell.scaleRange *= 0.72
                cell.scaleSpeed *= 0.72
                cell.lifetime *= 0.75
                cell.lifetimeRange *= 0.75
                cell.alphaSpeed = -0.7 / (cell.lifetime + cell.lifetimeRange)
                cell.color = CGColor(gray: 1, alpha: 0.7)
            }
        }
        return cells
    }

    private static func bitmap(pixels: Int, draw: (CGContext, CGFloat) -> Void) -> CGImage? {
        guard let ctx = CGContext(
            data: nil, width: pixels, height: pixels, bitsPerComponent: 8, bytesPerRow: 0,
            space: CGColorSpace(name: CGColorSpace.sRGB) ?? CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
        ) else {
            return nil
        }
        draw(ctx, CGFloat(pixels))
        return ctx.makeImage()
    }

    private static func gradient(_ stops: [(CGFloat, CGColor)]) -> CGGradient? {
        CGGradient(colorsSpace: CGColorSpace(name: CGColorSpace.sRGB),
                   colors: stops.map(\.1) as CFArray, locations: stops.map(\.0))
    }

    private static func starPath(center c: CGPoint, radius r: CGFloat, pinch: CGFloat) -> CGPath {
        let tips = [CGPoint(x: c.x, y: c.y + r), CGPoint(x: c.x + r, y: c.y),
                    CGPoint(x: c.x, y: c.y - r), CGPoint(x: c.x - r, y: c.y)]
        let path = CGMutablePath()
        path.move(to: tips[3])
        for (i, tip) in tips.enumerated() {
            let from = tips[(i + 3) % 4]
            let mid = CGPoint(x: (from.x + tip.x) / 2, y: (from.y + tip.y) / 2)
            let control = CGPoint(x: c.x + (mid.x - c.x) * pinch, y: c.y + (mid.y - c.y) * pinch)
            path.addQuadCurve(to: tip, control: control)
        }
        path.closeSubpath()
        return path
    }

    /// A four-point star cut from tinted glass: see-through body, denser coloured edge,
    /// a pale rim and a white glint.
    private static func makeStar(hue: Int, pixels: Int) -> CGImage? {
        bitmap(pixels: pixels) { ctx, size in
            let c = CGPoint(x: size / 2, y: size / 2)
            let r = size / 2 - 3
            let path = starPath(center: c, radius: r, pinch: 0.16)

            ctx.saveGState()
            ctx.addPath(path)
            ctx.clip()
            // Body: one hue melting into the next across the piece, thinner in the middle.
            if let body = gradient([(0, tone(hue, 0.8)), (0.5, tone(hue + 1, 0.42, lighten: 0.25)), (1, tone(hue + 2, 0.8))]) {
                ctx.drawLinearGradient(body, start: CGPoint(x: c.x - r, y: c.y + r), end: CGPoint(x: c.x + r, y: c.y - r), options: [])
            }
            // Light pooling in the upper left, as on a curved surface.
            if let sheen = gradient([(0, CGColor(gray: 1, alpha: 0.75)), (1, CGColor(gray: 1, alpha: 0))]) {
                let at = CGPoint(x: c.x - r * 0.16, y: c.y + r * 0.2)
                ctx.drawRadialGradient(sheen, startCenter: at, startRadius: 0, endCenter: at, endRadius: r * 0.5, options: [])
            }
            ctx.restoreGState()

            ctx.addPath(path)
            ctx.setStrokeColor(tone(hue + 1, 0.9, lighten: 0.55))
            ctx.setLineWidth(size * 0.022)
            ctx.setLineJoin(.round)
            ctx.strokePath()

            ctx.addPath(starPath(center: CGPoint(x: c.x - r * 0.08, y: c.y + r * 0.1), radius: r * 0.3, pinch: 0.1))
            ctx.setFillColor(CGColor(gray: 1, alpha: 0.95))
            ctx.fillPath()
        }
    }

    /// A glass bead: clear in the middle, coloured towards the edge, with a highlight and
    /// a patch of light coming through on the far side.
    private static func makeBead(hue: Int, pixels: Int) -> CGImage? {
        bitmap(pixels: pixels) { ctx, size in
            let c = CGPoint(x: size / 2, y: size / 2)
            let r = size / 2 - 2
            ctx.saveGState()
            ctx.addEllipse(in: CGRect(x: c.x - r, y: c.y - r, width: 2 * r, height: 2 * r))
            ctx.clip()
            if let body = gradient([(0, tone(hue, 0.14, lighten: 0.5)), (0.6, tone(hue, 0.36)), (0.88, tone(hue + 1, 0.82)), (1, tone(hue + 1, 0.95, lighten: 0.3))]) {
                ctx.drawRadialGradient(body, startCenter: c, startRadius: 0, endCenter: c, endRadius: r, options: [])
            }
            if let through = gradient([(0, tone(hue + 2, 0.7, lighten: 0.5)), (1, tone(hue + 2, 0, lighten: 0.5))]) {
                let at = CGPoint(x: c.x + r * 0.36, y: c.y - r * 0.4)
                ctx.drawRadialGradient(through, startCenter: at, startRadius: 0, endCenter: at, endRadius: r * 0.55, options: [])
            }
            if let glint = gradient([(0, CGColor(gray: 1, alpha: 1)), (0.5, CGColor(gray: 1, alpha: 0.7)), (1, CGColor(gray: 1, alpha: 0))]) {
                let at = CGPoint(x: c.x - r * 0.36, y: c.y + r * 0.4)
                ctx.drawRadialGradient(glint, startCenter: at, startRadius: 0, endCenter: at, endRadius: r * 0.36, options: [])
            }
            ctx.restoreGState()
        }
    }

    /// A thin iridescent ring, like the edge of a soap bubble.
    private static func makeRipple(pixels: Int) -> CGImage? {
        bitmap(pixels: pixels) { ctx, size in
            let c = CGPoint(x: size / 2, y: size / 2)
            let r = size / 2 - 1
            if let ring = gradient([(0, tone(1, 0, lighten: 0.6)), (0.62, tone(1, 0.05, lighten: 0.6)), (0.8, tone(2, 0.5, lighten: 0.35)),
                                    (0.9, tone(0, 0.6, lighten: 0.45)), (1, tone(4, 0, lighten: 0.4))]) {
                ctx.drawRadialGradient(ring, startCenter: c, startRadius: 0, endCenter: c, endRadius: r, options: [])
            }
        }
    }
}
