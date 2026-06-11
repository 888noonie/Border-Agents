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
pub const SURFACE_W: u32 = 400;
pub const SURFACE_H: u32 = 400;

pub const HEAD_CX: f32 = 104.0;
pub const HEAD_CY: f32 = 110.0;
pub const HEAD_R: f32 = 64.0;

// The drawn bubble auto-sizes its height to the text (see `paint`); this const fixes
// its origin/width and a max height that the input region covers (≈6 lines @ 16px).
pub const BUBBLE: Rect = Rect { x: 184.0, y: 34.0, w: 200.0, h: 172.0 };
pub const MENU: Rect = Rect { x: 38.0, y: 196.0, w: 244.0, h: 150.0 };
pub const MENU_ITEM_H: f32 = 44.0;
// The old "Say hello" became the input box (drawn separately); only action buttons
// remain here, laid out below the input box.
pub const MENU_ITEMS: [&str; 1] = ["Cycle mood"];
const INPUT_H: f32 = 46.0;

/// The on-body text input, at the top of the open menu. Clicking it focuses typing.
pub fn input_box_rect() -> Rect {
    Rect { x: MENU.x + 12.0, y: MENU.y + 12.0, w: MENU.w - 24.0, h: INPUT_H }
}

/// Radius of the tucked "bump" — the minimized half-disc the buddy shows when parked
/// flush against a screen edge. Smaller than the head so it frees screen space.
pub const BUMP_R: f32 = 34.0;

/// Which screen edge a tucked buddy is parked against. (Render-side mirror of the
/// presence protocol's edge; `main.rs` maps `presence::Edge` onto it.)
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BumpEdge {
    Top,
    Right,
    Bottom,
    Left,
}

/// Centre of the bump's full circle — sits ON the surface edge so only the on-surface
/// half shows (the other half clips against the pixmap bounds = the "split in half").
fn bump_center(edge: BumpEdge) -> (f32, f32) {
    match edge {
        BumpEdge::Left => (0.0, HEAD_CY),
        BumpEdge::Right => (SURFACE_W as f32, HEAD_CY),
        BumpEdge::Top => (HEAD_CX, 0.0),
        BumpEdge::Bottom => (HEAD_CX, SURFACE_H as f32),
    }
}

/// Bounding box of the *visible* (on-surface) half of the bump — used for the input
/// region and for keeping the bump on-screen when tucked.
pub fn bump_rect(edge: BumpEdge) -> Rect {
    let (cx, cy) = bump_center(edge);
    match edge {
        BumpEdge::Left => Rect { x: 0.0, y: cy - BUMP_R, w: BUMP_R, h: BUMP_R * 2.0 },
        BumpEdge::Right => Rect { x: cx - BUMP_R, y: cy - BUMP_R, w: BUMP_R, h: BUMP_R * 2.0 },
        BumpEdge::Top => Rect { x: cx - BUMP_R, y: 0.0, w: BUMP_R * 2.0, h: BUMP_R },
        BumpEdge::Bottom => Rect { x: cx - BUMP_R, y: cy - BUMP_R, w: BUMP_R * 2.0, h: BUMP_R },
    }
}

pub fn point_in_bump(edge: BumpEdge, px: f64, py: f64) -> bool {
    let (cx, cy) = bump_center(edge);
    let dx = px as f32 - cx;
    let dy = py as f32 - cy;
    dx * dx + dy * dy <= BUMP_R * BUMP_R
}

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
    // Action buttons start below the input box (input top pad + INPUT_H + gap).
    let top = MENU.y + 12.0 + INPUT_H + 12.0;
    Rect {
        x: MENU.x + 12.0,
        y: top + index as f32 * MENU_ITEM_H,
        w: MENU.w - 24.0,
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
    /// When `Some`, the buddy is tucked against this edge: draw the minimized bump
    /// instead of the full figure (and the body shows neither menu nor speech).
    pub tucked: Option<BumpEdge>,
    /// Current text in the on-body input box (shown when the menu is open).
    pub input_text: &'a str,
    /// Whether the input box has focus — brightens it and shows a blinking caret.
    pub input_focused: bool,
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

        // Tucked: draw only the minimized bump (no bob, no face, no menu/bubble) — a
        // dormant buddy hugging the edge, waiting to be summoned with a click.
        if let Some(edge) = view.tucked {
            draw_bump(&mut pixmap, edge);
            blit_premultiplied_bgra(pixmap.data(), canvas);
            return;
        }

        let bob = (view.t * std::f32::consts::TAU / 3.6).sin() * 4.0;
        // A ~150ms blink every 4s.
        let blinking = (view.t % 4.0) > 3.85;
        let face = view.emotion.face();
        let eye_open = if blinking { 0.10 } else { face.eye_open };

        draw_head(&mut pixmap, bob);
        draw_eyes(&mut pixmap, bob, eye_open, face.pupil_dy);
        draw_mouth(&mut pixmap, bob, &face.mouth);

        if let Some(text) = view.speech {
            if let Some(font) = &self.font {
                // Auto-size the bubble to the wrapped text so short lines get a small
                // card and longer messages grow downward (capped at 6 lines). Height
                // stays within BUBBLE.h so the input region keeps covering it.
                let pad_x = 16.0;
                let pad_top = 30.0;
                let px = 16.0;
                let line_h = px * 1.3;
                let lines = wrap(font, text, px, BUBBLE.w - pad_x * 2.0, 6);
                let body_h = pad_top + lines.len().max(1) as f32 * line_h + 12.0;
                let rect = Rect { x: BUBBLE.x, y: BUBBLE.y, w: BUBBLE.w, h: body_h };
                draw_round_rect(&mut pixmap, rect, Color::from_rgba8(247, 251, 255, 245));
                draw_bubble_tail(&mut pixmap, bob);
                let mut baseline = BUBBLE.y + pad_top;
                for line in &lines {
                    draw_line(&mut pixmap, font, line, BUBBLE.x + pad_x, baseline, px, [16, 24, 44]);
                    baseline += line_h;
                }
            }
        }

        if view.menu_open {
            draw_round_rect(&mut pixmap, MENU, Color::from_rgba8(12, 18, 40, 235));
            if let Some(font) = &self.font {
                draw_input(
                    &mut pixmap,
                    font,
                    input_box_rect(),
                    view.input_text,
                    view.input_focused,
                    view.t,
                );
            }
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

/// The tucked bump: a half-disc hugging the edge (the off-surface half clips away),
/// with a cyan "presence" dot toward the visible side so it reads as alive, not dead.
fn draw_bump(pixmap: &mut Pixmap, edge: BumpEdge) {
    let (cx, cy) = bump_center(edge);

    let outer = solid(Color::from_rgba8(47, 125, 255, 255));
    if let Some(c) = PathBuilder::from_circle(cx, cy, BUMP_R) {
        pixmap.fill_path(&c, &outer, FillRule::Winding, Transform::identity(), None);
    }
    let inner = solid(Color::from_rgba8(17, 27, 52, 255));
    if let Some(c) = PathBuilder::from_circle(cx, cy, BUMP_R - 7.0) {
        pixmap.fill_path(&c, &inner, FillRule::Winding, Transform::identity(), None);
    }

    // Nudge the dot toward the on-screen side of the bump.
    let (dx, dy) = match edge {
        BumpEdge::Left => (BUMP_R * 0.45, 0.0),
        BumpEdge::Right => (-BUMP_R * 0.45, 0.0),
        BumpEdge::Top => (0.0, BUMP_R * 0.45),
        BumpEdge::Bottom => (0.0, -BUMP_R * 0.45),
    };
    let cyan = solid(Color::from_rgba8(125, 249, 255, 255));
    if let Some(c) = PathBuilder::from_circle(cx + dx, cy + dy, 5.0) {
        pixmap.fill_path(&c, &cyan, FillRule::Winding, Transform::identity(), None);
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

// The on-body text input: a light field showing the typed text (or a placeholder),
// brighter when focused, with a blinking caret at the end of the text.
fn draw_input(pixmap: &mut Pixmap, font: &Font, rect: Rect, text: &str, focused: bool, t: f32) {
    let bg = if focused {
        Color::from_rgba8(255, 255, 255, 250)
    } else {
        Color::from_rgba8(226, 233, 243, 235)
    };
    draw_round_rect(pixmap, rect, bg);

    let px = 16.0;
    let baseline = rect.y + rect.h * 0.5 + px * 0.34;
    let pad = 12.0;
    let max_w = rect.w - pad * 2.0;

    let (shown, color) = if text.is_empty() && !focused {
        ("Type to me…".to_string(), [120, 130, 150])
    } else {
        // Show the tail so the latest characters stay visible as the field overflows.
        (clip_text_tail(font, text, px, max_w), [16, 24, 44])
    };
    draw_line(pixmap, font, &shown, rect.x + pad, baseline, px, color);

    // Blinking caret (~1.4 Hz) at the end of the visible text.
    if focused && ((t * 1.4) as i32) % 2 == 0 {
        let caret_x = rect.x + pad + measure(font, &shown, px) + 2.0;
        let caret = Rect { x: caret_x, y: rect.y + 10.0, w: 2.0, h: rect.h - 20.0 };
        draw_round_rect(pixmap, caret, Color::from_rgba8(47, 125, 255, 255));
    }
}

// Drop leading characters until the text fits `max_w`, so an overflowing input shows
// its end (where the caret is) rather than its beginning.
fn clip_text_tail(font: &Font, text: &str, px: f32, max_w: f32) -> String {
    if measure(font, text, px) <= max_w {
        return text.to_string();
    }
    let mut chars: Vec<char> = text.chars().collect();
    while chars.len() > 1 && measure(font, &chars.iter().collect::<String>(), px) > max_w {
        chars.remove(0);
    }
    chars.into_iter().collect()
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
