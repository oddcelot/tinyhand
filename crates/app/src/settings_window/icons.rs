//! Small icons standing in for the SF Symbols the Swift window uses. Each one is a
//! one-colour SVG drawn with `svg().data(..)`, so it takes the element's text colour.

use std::sync::Arc;

use gpui::{Hsla, ImageFormat, IntoElement, Styled, px, svg};

macro_rules! icon {
    ($name:ident, $view:literal, $body:literal) => {
        pub const $name: &str = concat!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"",
            $view,
            "\">",
            $body,
            "</svg>"
        );
    };
}

// Sidebar tiles.
icon!(GEAR, "0 0 24 24", r#"<path fill-rule="evenodd" d="M19.45 9.69 L22.00 9.97 L22.00 14.03 L19.45 14.31 L18.90 15.64 L20.50 17.64 L17.64 20.50 L15.64 18.90 L14.31 19.45 L14.03 22.00 L9.97 22.00 L9.69 19.45 L8.36 18.90 L6.36 20.50 L3.50 17.64 L5.10 15.64 L4.55 14.31 L2.00 14.03 L2.00 9.97 L4.55 9.69 L5.10 8.36 L3.50 6.36 L6.36 3.50 L8.36 5.10 L9.69 4.55 L9.97 2.00 L14.03 2.00 L14.31 4.55 L15.64 5.10 L17.64 3.50 L20.50 6.36 L18.90 8.36Z M15.3 12a3.3 3.3 0 1 0 -6.6 0a3.3 3.3 0 1 0 6.6 0Z"/>"#);
icon!(HAND, "1 1 30 30", r#"<path d="M14.448 11.218a2.09 2.09 0 014.18 0v1.418c0 .577.89.565.89.008v-.806a2.09 2.09 0 014.18 0v1.354c0 .544.945.523.945.015v-.434A1.678 1.678 0 0128 12.772v4.101c0 3.489-1.06 6.102-2.062 7.514-.607.855-.525 2.715-.525 4.612H22.78c-2.414 0-2.391-1.257-3.267-1.257S18.637 29 16.476 29h-3.673s-.123-1.378-.669-2.25c-.546-.872-2.793-1.804-4.1-3.547-1.719-2.292-1.856-4.448-4.68-7.357-.952-.981-.287-2.466 1.894-2.275 2.378.208 4.069 3.325 4.919 4.597.391.585.762.152.693-.158-.262-1.167-2.782-12.85-2.782-12.85a1.836 1.836 0 113.615-.647s1.547 7.036 1.734 7.779c.186.743 1.022.626 1.022-.03v-1.044z"/>"#);
icon!(SPARKLES, "0 0 24 24", r#"<path d="M14 4 Q15 12 22.5 13.5 Q15 15 14 23 Q13 15 5.5 13.5 Q13 12 14 4Z M6 1.5 Q6.5 5 9.5 5.8 Q6.5 6.6 6 10 Q5.5 6.6 2.5 5.8 Q5.5 5 6 1.5Z"/>"#);
icon!(PENCIL_TIP, "0 0 24 24", r#"<path fill-rule="evenodd" d="M12 1.8 L19.2 19.6 Q15.8 21.6 12 21.6 Q8.2 21.6 4.8 19.6Z M12 8.5 L15.8 18.4 Q14 19 12 19 Q10 19 8.2 18.4Z"/><path d="M12 1.8 L13.6 5.8 L10.4 5.8Z"/>"#);
icon!(SPEAKER_WAVE, "0 0 24 24", r#"<path d="M2.5 9 H6.5 L11.5 4.5 V19.5 L6.5 15 H2.5Z"/><path d="M15 8.6 A4.6 4.6 0 0 1 15 15.4 M18 5.6 A8.6 8.6 0 0 1 18 18.4" fill="none" stroke="black" stroke-width="2" stroke-linecap="round"/>"#);
icon!(INFO, "0 0 24 24", r#"<path fill-rule="evenodd" d="M22 12a10 10 0 1 1 -20 0a10 10 0 1 1 20 0Z M10.9 10.3 H13.1 V17.5 H10.9Z M13.35 7.2a1.35 1.35 0 1 1 -2.7 0a1.35 1.35 0 1 1 2.7 0Z"/>"#);

// Everything else.
icon!(WAVEFORM, "0 0 24 24", r#"<path d="M3 10.5v3 M6 8v8 M9 5v14 M12 9v6 M15 3.5v17 M18 8v8 M21 10.5v3" fill="none" stroke="black" stroke-width="1.7" stroke-linecap="round"/>"#);
icon!(SPEAKER_SLASH, "0 0 24 24", r#"<path d="M4 9 H8 L13 4.5 V19.5 L8 15 H4Z"/><path d="M3.5 3.5 L20.5 20.5" fill="none" stroke="black" stroke-width="2.4" stroke-linecap="round"/>"#);
icon!(CHEVRON_LEFT, "0 0 24 24", r#"<path d="M15 4.5 L7.5 12 L15 19.5" fill="none" stroke="black" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"/>"#);
icon!(CHEVRONS_UP_DOWN, "0 0 24 24", r#"<path d="M7.5 9.5 L12 5 L16.5 9.5 M7.5 14.5 L12 19 L16.5 14.5" fill="none" stroke="black" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round"/>"#);
icon!(PLUS, "0 0 24 24", r#"<path d="M12 4v16 M4 12h16" fill="none" stroke="black" stroke-width="1.8" stroke-linecap="round"/>"#);
icon!(XMARK, "0 0 24 24", r#"<path d="M5.5 5.5 L18.5 18.5 M18.5 5.5 L5.5 18.5" fill="none" stroke="black" stroke-width="4" stroke-linecap="round"/>"#);
icon!(ARROW_UP_RIGHT, "0 0 24 24", r#"<path d="M6 18 L18 6 M8.5 6 H18 V15.5" fill="none" stroke="black" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round"/>"#);
icon!(DOC_ON_DOC, "0 0 24 24", r#"<path d="M8.5 7.5 V5 a2 2 0 0 1 2-2 h6 l4 4 v9 a2 2 0 0 1 -2 2 h-2.5 M5.5 7.5 h6 l4 4 v8.5 a2 2 0 0 1 -2 2 h-8 a2 2 0 0 1 -2-2 v-10.5 a2 2 0 0 1 2-2Z" fill="none" stroke="black" stroke-width="1.9" stroke-linejoin="round"/>"#);
icon!(CHECKMARK, "0 0 24 24", r#"<path d="M4.5 12.5 L9.5 17.5 L19.5 6.5" fill="none" stroke="black" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"/>"#);
icon!(TRAY_DOWN, "0 0 24 24", r#"<path d="M12 3 V14 M7.5 9.5 L12 14 L16.5 9.5 M4.5 11 V18 a2.5 2.5 0 0 0 2.5 2.5 H17 a2.5 2.5 0 0 0 2.5 -2.5 V11" fill="none" stroke="black" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/>"#);
icon!(WARNING, "0 0 24 24", r#"<path fill-rule="evenodd" d="M10.3 3.6 a2 2 0 0 1 3.4 0 L21.7 18 a2 2 0 0 1 -1.7 3 H4 a2 2 0 0 1 -1.7 -3Z M11 8.5 H13 L12.7 14.5 H11.3Z M13.2 17.3a1.2 1.2 0 1 1 -2.4 0a1.2 1.2 0 1 1 2.4 0Z"/>"#);
icon!(WAND, "0 0 24 24", r#"<path d="M4 20 L15 9" fill="none" stroke="black" stroke-width="2.2" stroke-linecap="round"/><path d="M17 2.5 Q17.4 5.6 20.5 6 Q17.4 6.4 17 9.5 Q16.6 6.4 13.5 6 Q16.6 5.6 17 2.5Z M8 3 Q8.2 4.8 10 5 Q8.2 5.2 8 7 Q7.8 5.2 6 5 Q7.8 4.8 8 3Z M20 13 Q20.2 14.8 22 15 Q20.2 15.2 20 17 Q19.8 15.2 18 15 Q19.8 14.8 20 13Z"/>"#);
icon!(MINUS, "0 0 24 24", r#"<path d="M6 12 H18" fill="none" stroke="black" stroke-width="3.4" stroke-linecap="round"/>"#);

/// A one-colour icon.
pub fn icon(source: &'static str, size: f32, colour: Hsla) -> impl IntoElement {
    svg().data(source.as_bytes()).size(px(size)).flex_none().text_color(colour)
}

/// A full-colour SVG shown as an image (the laser dot, the rainbow ink swatch).
pub fn colour_svg(source: String) -> Arc<gpui::Image> {
    Arc::new(gpui::Image::from_bytes(ImageFormat::Svg, source.into_bytes()))
}

/// The laser dot as the toolbar draws it: a red glow, a red dot and a light centre.
pub fn laser_svg() -> String {
    r##"<svg xmlns="http://www.w3.org/2000/svg" width="128" height="128" viewBox="0 0 64 64"><defs><radialGradient id="g" cx="32" cy="32" r="18" gradientUnits="userSpaceOnUse"><stop offset="0" stop-color="#FF291F" stop-opacity="0.55"/><stop offset="1" stop-color="#FF291F" stop-opacity="0"/></radialGradient></defs><circle cx="32" cy="32" r="18" fill="url(#g)"/><circle cx="32" cy="32" r="7" fill="#FF291F"/><circle cx="32" cy="32" r="3" fill="#FFDBD1" fill-opacity="0.95"/></svg>"##.to_string()
}

/// The "Match the pointer" swatch: a colour wheel, as the Swift angular gradient.
pub fn rainbow_svg() -> String {
    let stops: [(f32, f32, f32); 6] = [(1.0, 0.0, 0.0), (1.0, 0.8, 0.0), (0.2, 0.78, 0.35), (0.0, 0.48, 1.0), (0.69, 0.32, 0.87), (1.0, 0.0, 0.0)];
    let mut out = String::from(r#"<svg xmlns="http://www.w3.org/2000/svg" width="80" height="80" viewBox="0 0 40 40">"#);
    let n = 60;
    for i in 0..n {
        let t = i as f32 / n as f32;
        let seg = t * 5.0;
        let k = seg.floor() as usize;
        let f = seg - k as f32;
        let (a, b) = (stops[k], stops[(k + 1).min(5)]);
        let c = |x: f32, y: f32| ((x + (y - x) * f) * 255.0).round() as u8;
        let (r, g, bl) = (c(a.0, b.0), c(a.1, b.1), c(a.2, b.2));
        // Angles start at the right and go clockwise, like SwiftUI's AngularGradient.
        let a0 = t * std::f32::consts::TAU;
        let a1 = (i + 1) as f32 / n as f32 * std::f32::consts::TAU + 0.02;
        let (x0, y0) = (20.0 + 20.0 * a0.cos(), 20.0 + 20.0 * a0.sin());
        let (x1, y1) = (20.0 + 20.0 * a1.cos(), 20.0 + 20.0 * a1.sin());
        out.push_str(&format!(r##"<path d="M20 20 L{x0:.3} {y0:.3} A20 20 0 0 1 {x1:.3} {y1:.3}Z" fill="#{r:02X}{g:02X}{bl:02X}"/>"##));
    }
    out.push_str("</svg>");
    out
}
