//! Software renderer for the desktop presence body (step 3 — animated body).
//!
//! Pure-Rust, no GPU: tiny-skia draws the antialiased buddy + speech bubble +
//! menu card into an RGBA pixmap, fontdue rasterizes the bubble/menu text, and we
//! convert to premultiplied BGRA for the `wl_shm` Argb8888 buffer. The static
//! sprite from the step-2 spike is replaced with a time-driven face: an idle bob,
//! periodic blink, and an emotion that changes the eyes and mouth.

use fontdue::Font;
use tiny_skia::{
    Color, FillRule, Mask, Paint, PathBuilder, Pixmap, Shader, Stroke, Transform,
};

// --- surface layout (also used by main.rs to build input regions) --------------
pub const SURFACE_W: u32 = 320;
pub const SURFACE_H: u32 = 360;

pub const HEAD_CX: f32 = 104.0;
pub const HEAD_CY: f32 = 110.0;
pub const HEAD_R: f32 = 64.0;

pub const BUBBLE: Rect = Rect { x: 176.0, y: 60.0, w: 134.0, h: 104.0 };
pub const MENU: Rect = Rect { x: 38.0, y: 214.0, w: 168.0, h: 122.0 };
pub const MENU_ITEM_H: f32 = 44.0;
pub const MENU_ITEMS: [&str; 2] = ["Say hello", "Cycle mood"];

#[derive(Clone, Copy)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn contains(&self, px: f64, py: f64) -> bool {
        let (px, py) = (px as f32, py as f32);
        px >= self.x && px <= self.x + self.w && py >= self.y && py <= self.y + self.h
    }
    pub fn as_i32(&self) -> (i32, i32, i32, i32) {
        (self.x as i32, self.y as i32, self.w as i32, self.h as i32)
    }
}

pub fn head_rect() -> Rect {
    Rect { x: HEAD_CX - HEAD_R, y: HEAD_CY - HEAD_R, w: HEAD_R * 2.0, h: HEAD_R * 2.0 }
}

pub fn menu_item_rect(index: usize) -> Rect {
    Rect {
        x: MENU.x + 10.0,
        y: MENU.y + 12.0 + index as f32 * MENU_ITEM_H,
        w: MENU.w - 20.0,
        h: MENU_ITEM_H - 8.0,
    }
}

pub fn point_in_head(px: f64, py: f64) -> bool {
    let dx = px as f32 - HEAD_CX;
    let dy = py as f32 - HEAD_CY;
    dx * dx + dy * dy <= HEAD_R * HEAD_R
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Emotion {
    Neutral,
    Happy,
    Thinking,
    Curious,
    Alert,
    Sleepy,
}

impl Emotion {
    pub const CYCLE: [Emotion; 6] = [
        Emotion::Neutral,
        Emotion::Happy,
        Emotion::Thinking,
        Emotion::Curious,
        Emotion::Alert,
        Emotion::Sleepy,
    ];

    /// Parse the wire value used by the presence protocol's `express` event.
    /// (Seam for step 4 — driving the body from presence events over the socket.)
    #[allow(dead_code)]
    pub fn from_wire(value: &str) -> Option<Emotion> {
        Some(match value {
            "neutral" => Emotion::Neutral,
            "happy" => Emotion::Happy,
            "thinking" => Emotion::Thinking,
            "curious" => Emotion::Curious,
            "alert" => Emotion::Alert,
            "sleepy" => Emotion::Sleepy,
            _ => return None,
        })
    }

    fn face(self) -> Face {
        match self {
            Emotion::Neutral => Face { eye_open: 1.0, pupil_dy: 0.0, mouth: Mouth::Smile(0.18) },
            Emotion::Happy => Face { eye_open: 0.9, pupil_dy: 0.0, mouth: Mouth::Smile(0.55) },
            Emotion::Thinking => Face { eye_open: 0.8, pupil_dy: -0.5, mouth: Mouth::Flat },
            Emotion::Curious => Face { eye_open: 1.25, pupil_dy: -0.2, mouth: Mouth::Open(0.6) },
            Emotion::Alert => Face { eye_open: 1.4, pupil_dy: 0.0, mouth: Mouth::Open(1.0) },
            Emotion::Sleepy => Face { eye_open: 0.28, pupil_dy: 0.3, mouth: Mouth::Smile(0.08) },
        }
    }
}

struct Face {
    eye_open: f32,
    pupil_dy: f32,
    mouth: Mouth,
}

enum Mouth {
    Smile(f32),
    Open(f32),
    Flat,
}

pub struct BodyView<'a> {
    /// Seconds since start — drives bob and blink.
    pub t: f32,
    pub emotion: Emotion,
    pub speech: Option<&'a str>,
    pub menu_open: bool,
}

pub struct Sprite {
    font: Option<Font>,
}

impl Sprite {
    pub fn new() -> Self {
        Sprite { font: load_font() }
    }

    /// Render the body into a premultiplied-BGRA `wl_shm` canvas of size `w`×`h`.
    ///
    /// The pixmap matches the actual buffer size (which the compositor may shrink
    /// as the surface slides off an edge) so the row stride always matches the
    /// canvas — otherwise copying fixed-width rows into a narrower buffer shears the
    /// image. Drawing uses fixed surface-local coordinates and simply clips.
    pub fn paint(&self, canvas: &mut [u8], w: u32, h: u32, view: &BodyView) {
        let Some(mut pixmap) = Pixmap::new(w, h) else {
            return;
        };

        let bob = (view.t * std::f32::consts::TAU / 3.6).sin() * 4.0;
        // A ~150ms blink every 4s.
        let blinking = (view.t % 4.0) > 3.85;
        let face = view.emotion.face();
        let eye_open = if blinking { 0.10 } else { face.eye_open };

        draw_head(&mut pixmap, bob);
        draw_eyes(&mut pixmap, bob, eye_open, face.pupil_dy);
        draw_mouth(&mut pixmap, bob, &face.mouth);

        if let Some(text) = view.speech {
            draw_round_rect(&mut pixmap, BUBBLE, Color::from_rgba8(247, 251, 255, 245));
            draw_bubble_tail(&mut pixmap, bob);
            if let Some(font) = &self.font {
                draw_wrapped_text(
                    &mut pixmap,
                    font,
                    text,
                    BUBBLE.x + 14.0,
                    BUBBLE.y + 26.0,
                    BUBBLE.w - 28.0,
                    15.0,
                    [16, 24, 44],
                    3,
                );
            }
        }

        if view.menu_open {
            draw_round_rect(&mut pixmap, MENU, Color::from_rgba8(12, 18, 40, 235));
            for (i, label) in MENU_ITEMS.iter().enumerate() {
                let item = menu_item_rect(i);
                draw_round_rect(&mut pixmap, item, Color::from_rgba8(47, 125, 255, 235));
                if let Some(font) = &self.font {
                    draw_wrapped_text(
                        &mut pixmap,
                        font,
                        label,
                        item.x + 14.0,
                        item.y + 24.0,
                        item.w - 20.0,
                        15.0,
                        [242, 251, 255],
                        1,
                    );
                }
            }
        }

        blit_premultiplied_bgra(pixmap.data(), canvas);
    }
}

fn draw_head(pixmap: &mut Pixmap, bob: f32) {
    let mut paint = Paint::default();
    paint.anti_alias = true;
    paint.shader = tiny_skia::LinearGradient::new(
        tiny_skia::Point::from_xy(HEAD_CX, HEAD_CY - HEAD_R + bob),
        tiny_skia::Point::from_xy(HEAD_CX, HEAD_CY + HEAD_R + bob),
        vec![
            tiny_skia::GradientStop::new(0.0, Color::from_rgba8(47, 125, 255, 255)),
            tiny_skia::GradientStop::new(0.62, Color::from_rgba8(17, 27, 52, 255)),
            tiny_skia::GradientStop::new(1.0, Color::from_rgba8(5, 9, 19, 255)),
        ],
        tiny_skia::SpreadMode::Pad,
        Transform::identity(),
    )
    .unwrap_or_else(|| Shader::SolidColor(Color::from_rgba8(47, 125, 255, 255)));

    if let Some(circle) = PathBuilder::from_circle(HEAD_CX, HEAD_CY + bob, HEAD_R) {
        pixmap.fill_path(&circle, &paint, FillRule::Winding, Transform::identity(), None);
    }
}

fn draw_eyes(pixmap: &mut Pixmap, bob: f32, eye_open: f32, pupil_dy: f32) {
    let cyan = solid(Color::from_rgba8(125, 249, 255, 255));
    let dark = solid(Color::from_rgba8(7, 16, 32, 255));
    for sign in [-1.0_f32, 1.0] {
        let ex = HEAD_CX + sign * 24.0;
        let ey = HEAD_CY - 8.0 + bob;
        // Eye = ellipse approximated by a scaled circle path.
        if let Some(eye) = ellipse_path(ex, ey, 13.0, 16.0 * eye_open) {
            pixmap.fill_path(&eye, &cyan, FillRule::Winding, Transform::identity(), None);
        }
        if eye_open > 0.35 {
            if let Some(pupil) = PathBuilder::from_circle(ex + sign * 2.0, ey + 6.0 * pupil_dy, 5.0) {
                pixmap.fill_path(&pupil, &dark, FillRule::Winding, Transform::identity(), None);
            }
        }
    }
}

fn draw_mouth(pixmap: &mut Pixmap, bob: f32, mouth: &Mouth) {
    let mut stroke = Stroke::default();
    stroke.width = 4.0;
    stroke.line_cap = tiny_skia::LineCap::Round;
    let ink = solid(Color::from_rgba8(7, 16, 32, 255));
    let my = HEAD_CY + 30.0 + bob;

    match *mouth {
        Mouth::Smile(amount) => {
            let mut pb = PathBuilder::new();
            pb.move_to(HEAD_CX - 22.0, my);
            pb.quad_to(HEAD_CX, my + 22.0 * amount, HEAD_CX + 22.0, my);
            if let Some(path) = pb.finish() {
                pixmap.stroke_path(&path, &ink, &stroke, Transform::identity(), None);
            }
        }
        Mouth::Flat => {
            let mut pb = PathBuilder::new();
            pb.move_to(HEAD_CX - 16.0, my + 4.0);
            pb.line_to(HEAD_CX + 16.0, my + 4.0);
            if let Some(path) = pb.finish() {
                pixmap.stroke_path(&path, &ink, &stroke, Transform::identity(), None);
            }
        }
        Mouth::Open(amount) => {
            let cx = HEAD_CX;
            let cy = my + 4.0;
            let rx = 11.0;
            let ry = 7.0 + 7.0 * amount;

            let Some(cavity) = ellipse_path(cx, cy, rx, ry) else { return };
            // The open mouth interior.
            pixmap.fill_path(&cavity, &ink, FillRule::Winding, Transform::identity(), None);

            // Clip the teeth + tongue to the cavity so they're framed by the lips and
            // never spill past them (the ellipse edge rounds off their corners).
            let clip = Mask::new(pixmap.width(), pixmap.height()).map(|mut m| {
                m.fill_path(&cavity, FillRule::Winding, true, Transform::identity());
                m
            });
            let clip = clip.as_ref();

            // Tongue — a pink mound in the lower half of the mouth.
            let tongue = solid(Color::from_rgba8(232, 92, 110, 255));
            if let Some(t) = ellipse_path(cx, cy + ry * 0.42, rx * 0.72, ry * 0.62) {
                pixmap.fill_path(&t, &tongue, FillRule::Winding, Transform::identity(), clip);
            }

            // Upper teeth — a white band across the top, with thin gaps suggesting
            // individual teeth.
            let white = solid(Color::from_rgba8(248, 250, 252, 255));
            let teeth_h = (ry * 0.55).min(7.0);
            let top = cy - ry;
            let mut band = PathBuilder::new();
            band.move_to(cx - rx, top);
            band.line_to(cx + rx, top);
            band.line_to(cx + rx, top + teeth_h);
            band.line_to(cx - rx, top + teeth_h);
            band.close();
            if let Some(path) = band.finish() {
                pixmap.fill_path(&path, &white, FillRule::Winding, Transform::identity(), clip);
            }

            let mut sep = Stroke::default();
            sep.width = 1.5;
            for dx in [-rx * 0.45, 0.0, rx * 0.45] {
                let mut pb = PathBuilder::new();
                pb.move_to(cx + dx, top);
                pb.line_to(cx + dx, top + teeth_h);
                if let Some(p) = pb.finish() {
                    pixmap.stroke_path(&p, &ink, &sep, Transform::identity(), clip);
                }
            }
        }
    }
}

fn draw_bubble_tail(pixmap: &mut Pixmap, _bob: f32) {
    let mut pb = PathBuilder::new();
    pb.move_to(BUBBLE.x, BUBBLE.y + 34.0);
    pb.line_to(BUBBLE.x - 16.0, BUBBLE.y + 48.0);
    pb.line_to(BUBBLE.x + 4.0, BUBBLE.y + 60.0);
    pb.close();
    if let Some(path) = pb.finish() {
        pixmap.fill_path(
            &path,
            &solid(Color::from_rgba8(247, 251, 255, 245)),
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

fn draw_round_rect(pixmap: &mut Pixmap, rect: Rect, color: Color) {
    let r = 16.0_f32.min(rect.w / 2.0).min(rect.h / 2.0);
    let (x, y, w, h) = (rect.x, rect.y, rect.w, rect.h);
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.quad_to(x + w, y, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.quad_to(x + w, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.quad_to(x, y + h, x, y + h - r);
    pb.line_to(x, y + r);
    pb.quad_to(x, y, x + r, y);
    pb.close();
    if let Some(path) = pb.finish() {
        pixmap.fill_path(&path, &solid(color), FillRule::Winding, Transform::identity(), None);
    }
}

fn ellipse_path(cx: f32, cy: f32, rx: f32, ry: f32) -> Option<tiny_skia::Path> {
    let circle = PathBuilder::from_circle(0.0, 0.0, 1.0)?;
    circle.transform(Transform::from_row(rx, 0.0, 0.0, ry.max(0.5), cx, cy))
}

fn solid(color: Color) -> Paint<'static> {
    let mut paint = Paint::default();
    paint.anti_alias = true;
    paint.shader = Shader::SolidColor(color);
    paint
}

// --- text ----------------------------------------------------------------------

fn load_font() -> Option<Font> {
    let candidates = [
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
    ];
    for path in candidates {
        if let Ok(bytes) = std::fs::read(path) {
            if let Ok(font) = Font::from_bytes(bytes, fontdue::FontSettings::default()) {
                return Some(font);
            }
        }
    }
    eprintln!("[bb-desktop-body] no system font found — bubble/menu text disabled");
    None
}

#[allow(clippy::too_many_arguments)]
fn draw_wrapped_text(
    pixmap: &mut Pixmap,
    font: &Font,
    text: &str,
    x: f32,
    baseline: f32,
    max_w: f32,
    px: f32,
    color: [u8; 3],
    max_lines: usize,
) {
    let lines = wrap(font, text, px, max_w, max_lines);
    let line_h = px * 1.3;
    for (i, line) in lines.iter().enumerate() {
        draw_line(pixmap, font, line, x, baseline + i as f32 * line_h, px, color);
    }
}

fn wrap(font: &Font, text: &str, px: f32, max_w: f32, max_lines: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let candidate = if current.is_empty() { word.to_string() } else { format!("{current} {word}") };
        if measure(font, &candidate, px) <= max_w || current.is_empty() {
            current = candidate;
        } else {
            lines.push(std::mem::take(&mut current));
            current = word.to_string();
            if lines.len() == max_lines {
                break;
            }
        }
    }
    if lines.len() < max_lines && !current.is_empty() {
        lines.push(current);
    }
    if lines.len() == max_lines {
        if let Some(last) = lines.last_mut() {
            if measure(font, last, px) > max_w {
                while measure(font, &format!("{last}…"), px) > max_w && last.pop().is_some() {}
                last.push('…');
            }
        }
    }
    lines
}

fn measure(font: &Font, text: &str, px: f32) -> f32 {
    text.chars().map(|ch| font.metrics(ch, px).advance_width).sum()
}

fn draw_line(pixmap: &mut Pixmap, font: &Font, text: &str, x: f32, baseline: f32, px: f32, color: [u8; 3]) {
    let w = pixmap.width() as i32;
    let h = pixmap.height() as i32;
    let data = pixmap.data_mut();
    let mut pen = x;
    for ch in text.chars() {
        let (metrics, bitmap) = font.rasterize(ch, px);
        let gx = pen + metrics.xmin as f32;
        let gy = baseline - (metrics.height as f32 + metrics.ymin as f32);
        for row in 0..metrics.height {
            for col in 0..metrics.width {
                let cov = bitmap[row * metrics.width + col] as f32 / 255.0;
                if cov <= 0.0 {
                    continue;
                }
                let px_x = (gx + col as f32) as i32;
                let px_y = (gy + row as f32) as i32;
                if px_x < 0 || px_y < 0 || px_x >= w || px_y >= h {
                    continue;
                }
                let idx = ((px_y * w + px_x) * 4) as usize;
                // Source-over in premultiplied RGBA (tiny-skia's pixel order).
                let inv = 1.0 - cov;
                data[idx] = (color[0] as f32 * cov + data[idx] as f32 * inv) as u8;
                data[idx + 1] = (color[1] as f32 * cov + data[idx + 1] as f32 * inv) as u8;
                data[idx + 2] = (color[2] as f32 * cov + data[idx + 2] as f32 * inv) as u8;
                data[idx + 3] = (255.0 * cov + data[idx + 3] as f32 * inv) as u8;
            }
        }
        pen += metrics.advance_width;
    }
}

/// tiny-skia stores premultiplied RGBA; `wl_shm` Argb8888 wants premultiplied
/// BGRA byte order. Swap R/B.
fn blit_premultiplied_bgra(rgba: &[u8], canvas: &mut [u8]) {
    for (src, dst) in rgba.chunks_exact(4).zip(canvas.chunks_exact_mut(4)) {
        dst[0] = src[2];
        dst[1] = src[1];
        dst[2] = src[0];
        dst[3] = src[3];
    }
}
