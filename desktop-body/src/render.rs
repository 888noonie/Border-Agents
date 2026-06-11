//! Software renderer for the desktop presence body — the clay figure.
//!
//! Pure-Rust, no GPU: tiny-skia draws an antialiased stop-motion-style buddy
//! (think Aardman's Morph: terracotta clay, big white eyes, slender stretchable
//! body) plus a speech bubble and an on-body chat input into an RGBA pixmap;
//! fontdue rasterizes text; we convert to premultiplied BGRA for `wl_shm`.
//!
//! Geometry is parameterized through [`Layout`] instead of fixed constants:
//! - `facing` flips the bubble + input to whichever side faces the screen
//!   centre, so UI never clips against the docked edge;
//! - `body_len` stretches the rounded-rect torso (the user resizes the figure
//!   by dragging its feet; the head stays the dock/drag handle).
//!
//! Limbs are drawn from [`FigurePose`] joint angles so future cues can animate
//! them (wave, point, carry) without touching the geometry code. The mouth is
//! drawn from a [`MouthSpec`], with a [`Viseme`] table modeled on stop-motion
//! clay phoneme charts — the seam for TTS lipsync later.

use fontdue::Font;
use tiny_skia::{
    Color, FillRule, Mask, Paint, PathBuilder, Pixmap, Shader, Stroke, Transform,
};

// --- figure geometry -------------------------------------------------------------

pub const SURFACE_W: u32 = 480;
/// Figure centreline. UI (bubble/input) flips to either side of this.
pub const FIG_CX: f32 = 240.0;
pub const HEAD_CY: f32 = 58.0;
pub const HEAD_R: f32 = 44.0;
/// Torso top — tucks up under the chin (Morph has no neck).
const TORSO_TOP: f32 = HEAD_CY + HEAD_R - 12.0;
/// Slender clay torso; the rounded rectangle the user asked for, 10px sides.
pub const TORSO_W: f32 = 54.0;
const TORSO_R: f32 = 10.0;
pub const BODY_LEN_MIN: f32 = 70.0;
pub const BODY_LEN_MAX: f32 = 300.0;
pub const BODY_LEN_DEFAULT: f32 = 140.0;
const LEG_H: f32 = 30.0;
const FOOT_H: f32 = 14.0;
const BOTTOM_PAD: f32 = 10.0;
/// The surface is never shorter than this, so the bubble + input always fit
/// beside the figure even when the body is squashed to its minimum.
const UI_MIN_H: f32 = 312.0;

const ARM_UPPER: f32 = 36.0;
const ARM_FORE: f32 = 32.0;
const ARM_W: f32 = 13.0;
const HAND_R: f32 = 9.0;
const LEG_W: f32 = 15.0;

// --- UI geometry -----------------------------------------------------------------

const BUBBLE_W: f32 = 172.0;
const BUBBLE_Y: f32 = 8.0;
const BUBBLE_MAX_LINES: usize = 6;
const INPUT_Y: f32 = 196.0;
const TEXT_PX: f32 = 16.0;
const LINE_H: f32 = TEXT_PX * 1.3;
pub const INPUT_MAX_LINES: usize = 3;
/// Gap between the figure and its UI column.
const UI_GAP: f32 = 16.0;

/// Default clay colour — Morph terracotta. Override per-buddy with `BB_COLOR`.
pub const CLAY_DEFAULT: [u8; 3] = [201, 109, 60];

/// Which side of the figure the UI (bubble + input) sits on. Computed by main.rs
/// from the buddy's screen position so the UI always faces the screen centre.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Facing {
    Left,
    Right,
}

/// Parameterized surface layout: everything whose position depends on the
/// stretchable body or the inward-facing flip. main.rs builds one per frame /
/// hit-test from its `facing` + `body_len` state.
#[derive(Clone, Copy)]
pub struct Layout {
    pub facing: Facing,
    pub body_len: f32,
}

impl Layout {
    pub fn initial() -> Layout {
        Layout { facing: Facing::Right, body_len: BODY_LEN_DEFAULT }
    }

    /// Bottom of the torso — where the hips/legs start.
    fn hips_y(&self) -> f32 {
        TORSO_TOP + self.body_len
    }

    /// Total surface height for the current stretch (figure or UI, whichever
    /// is taller).
    pub fn surface_h(&self) -> u32 {
        let figure = self.hips_y() + LEG_H + FOOT_H + BOTTOM_PAD;
        figure.max(UI_MIN_H) as u32
    }

    fn ui_x(&self, w: f32) -> f32 {
        match self.facing {
            Facing::Right => FIG_CX + HEAD_R + UI_GAP,
            Facing::Left => FIG_CX - HEAD_R - UI_GAP - w,
        }
    }

    /// Speech bubble at its maximum extent (drawing shrinks to the text; the
    /// input region uses this full rect).
    pub fn bubble_rect(&self) -> Rect {
        let h = 30.0 + BUBBLE_MAX_LINES as f32 * LINE_H + 12.0;
        Rect { x: self.ui_x(BUBBLE_W), y: BUBBLE_Y, w: BUBBLE_W, h }
    }

    /// Chat input box sized for `lines` lines of text.
    pub fn input_rect(&self, lines: usize) -> Rect {
        let lines = lines.clamp(1, INPUT_MAX_LINES) as f32;
        Rect { x: self.ui_x(BUBBLE_W), y: INPUT_Y, w: BUBBLE_W, h: 16.0 + lines * LINE_H }
    }

    /// The input box at its maximum height — what the input region covers, so
    /// growing while typing never races the region.
    pub fn input_region_rect(&self) -> Rect {
        self.input_rect(INPUT_MAX_LINES)
    }

    /// Grab zone over the legs/feet — dragging it vertically stretches the body.
    pub fn feet_rect(&self) -> Rect {
        Rect {
            x: FIG_CX - 56.0,
            y: self.hips_y() - 6.0,
            w: 112.0,
            h: LEG_H + FOOT_H + BOTTOM_PAD + 6.0,
        }
    }

    // --- tucked bump (minimized, parked flush against a screen edge) ---

    /// Centre of the bump's full circle — ON the surface edge so only the
    /// on-surface half shows (the off-surface half clips = the "split in half").
    fn bump_center(&self, edge: BumpEdge) -> (f32, f32) {
        let h = self.surface_h() as f32;
        match edge {
            BumpEdge::Left => (0.0, HEAD_CY),
            BumpEdge::Right => (SURFACE_W as f32, HEAD_CY),
            BumpEdge::Top => (FIG_CX, 0.0),
            BumpEdge::Bottom => (FIG_CX, h),
        }
    }

    /// Bounding box of the visible half of the bump — input region + on-screen
    /// clamping while tucked.
    pub fn bump_rect(&self, edge: BumpEdge) -> Rect {
        let (cx, cy) = self.bump_center(edge);
        match edge {
            BumpEdge::Left => Rect { x: 0.0, y: cy - BUMP_R, w: BUMP_R, h: BUMP_R * 2.0 },
            BumpEdge::Right => Rect { x: cx - BUMP_R, y: cy - BUMP_R, w: BUMP_R, h: BUMP_R * 2.0 },
            BumpEdge::Top => Rect { x: cx - BUMP_R, y: 0.0, w: BUMP_R * 2.0, h: BUMP_R },
            BumpEdge::Bottom => Rect { x: cx - BUMP_R, y: cy - BUMP_R, w: BUMP_R * 2.0, h: BUMP_R },
        }
    }

    pub fn point_in_bump(&self, edge: BumpEdge, px: f64, py: f64) -> bool {
        let (cx, cy) = self.bump_center(edge);
        let dx = px as f32 - cx;
        let dy = py as f32 - cy;
        dx * dx + dy * dy <= BUMP_R * BUMP_R
    }
}

/// Radius of the tucked "bump" — smaller than the head so it frees screen space.
pub const BUMP_R: f32 = 34.0;

/// Which screen edge a tucked buddy is parked against. (Render-side mirror of
/// the presence protocol's edge; `main.rs` maps `presence::Edge` onto it.)
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BumpEdge {
    Top,
    Right,
    Bottom,
    Left,
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

/// The head is the dock/drag handle — fixed regardless of stretch.
pub fn head_rect() -> Rect {
    Rect { x: FIG_CX - HEAD_R, y: HEAD_CY - HEAD_R, w: HEAD_R * 2.0, h: HEAD_R * 2.0 }
}

pub fn point_in_head(px: f64, py: f64) -> bool {
    let dx = px as f32 - FIG_CX;
    let dy = py as f32 - HEAD_CY;
    dx * dx + dy * dy <= HEAD_R * HEAD_R
}

// --- pose (the future-animation seam) ---------------------------------------------

/// Joint angles for one arm, in degrees from straight-down; positive swings the
/// limb away from the body. Future presence cues (wave, point, carry) animate
/// the figure by writing poses — the geometry below just follows the joints.
#[derive(Clone, Copy)]
pub struct ArmPose {
    pub shoulder: f32,
    pub elbow: f32,
}

#[derive(Clone, Copy)]
pub struct FigurePose {
    pub left_arm: ArmPose,
    pub right_arm: ArmPose,
    /// Whole-figure lean in degrees (reserved for walk/react animations).
    #[allow(dead_code)]
    pub lean: f32,
}

impl FigurePose {
    /// Gentle idle: arms slightly out, breathing sway.
    pub fn idle(t: f32) -> FigurePose {
        let sway = (t * std::f32::consts::TAU / 5.6).sin();
        FigurePose {
            left_arm: ArmPose { shoulder: 16.0 + sway * 4.0, elbow: 12.0 },
            right_arm: ArmPose { shoulder: 16.0 - sway * 4.0, elbow: 12.0 },
            lean: 0.0,
        }
    }
}

// --- face ---------------------------------------------------------------------------

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
    /// Parse the wire value used by the presence protocol's `express` event.
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
            // Curious = the small round 'O U W Q' clay mouth from the chart.
            Emotion::Curious => Face { eye_open: 1.25, pupil_dy: -0.2, mouth: Mouth::Spec(viseme_spec(Viseme::OUWQ)) },
            // Alert = the wide-open 'O U AGH' mouth — teeth and tongue showing.
            Emotion::Alert => Face { eye_open: 1.4, pupil_dy: 0.0, mouth: Mouth::Spec(viseme_spec(Viseme::Agh)) },
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
    Flat,
    Spec(MouthSpec),
}

/// Parameterized clay mouth: an open cavity with optional teeth bands and tongue.
/// Every shape on a stop-motion phoneme chart is some setting of these knobs.
#[derive(Clone, Copy)]
pub struct MouthSpec {
    /// Half-width of the mouth cavity.
    pub rx: f32,
    /// 0..=1 — how far open (drives cavity height).
    pub open: f32,
    pub teeth_top: bool,
    pub teeth_bottom: bool,
    pub tongue: bool,
}

/// Clay phoneme mouths, named for the letter groups on stop-motion mouth charts
/// ("A/I", "O U W Q", "C D E G K N R S", "TH/L", "F/V", "M B P", open "AGH").
/// This is the lipsync seam: a future TTS path maps phonemes → `Viseme` →
/// `Mouth::Spec(viseme_spec(v))` per frame, and the clay mouth talks.
#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Viseme {
    /// Closed, resting lips.
    Rest,
    /// Wide open smile-shape with upper teeth: 'A', 'I'.
    AI,
    /// Small tight ring: 'O', 'U', 'W', 'Q'.
    OUWQ,
    /// Mid-open, both teeth bands: 'C D E G K N R S'.
    Cdgknrs,
    /// Tongue visible behind upper teeth: 'TH', 'L'.
    ThL,
    /// Lower lip under upper teeth: 'F', 'V'.
    FV,
    /// Pressed-flat lips: 'M', 'B', 'P'.
    Mbp,
    /// Big open vowel: 'AGH'.
    Agh,
}

pub fn viseme_spec(v: Viseme) -> MouthSpec {
    match v {
        Viseme::Rest => MouthSpec { rx: 12.0, open: 0.06, teeth_top: false, teeth_bottom: false, tongue: false },
        Viseme::AI => MouthSpec { rx: 16.0, open: 0.55, teeth_top: true, teeth_bottom: true, tongue: false },
        Viseme::OUWQ => MouthSpec { rx: 7.0, open: 0.55, teeth_top: false, teeth_bottom: false, tongue: false },
        Viseme::Cdgknrs => MouthSpec { rx: 13.0, open: 0.45, teeth_top: true, teeth_bottom: true, tongue: false },
        Viseme::ThL => MouthSpec { rx: 12.0, open: 0.6, teeth_top: true, teeth_bottom: false, tongue: true },
        Viseme::FV => MouthSpec { rx: 13.0, open: 0.25, teeth_top: true, teeth_bottom: false, tongue: false },
        Viseme::Mbp => MouthSpec { rx: 11.0, open: 0.0, teeth_top: false, teeth_bottom: false, tongue: false },
        Viseme::Agh => MouthSpec { rx: 13.0, open: 1.0, teeth_top: true, teeth_bottom: false, tongue: true },
    }
}

// --- view + sprite ---------------------------------------------------------------

pub struct BodyView<'a> {
    /// Seconds since start — drives bob, blink, and limb sway.
    pub t: f32,
    pub emotion: Emotion,
    pub speech: Option<&'a str>,
    /// Whether the chat input is open (replaces the old menu).
    pub chat_open: bool,
    /// When `Some`, the buddy is tucked against this edge: draw the minimized
    /// bump instead of the full figure.
    pub tucked: Option<BumpEdge>,
    pub input_text: &'a str,
    pub input_focused: bool,
    pub layout: Layout,
    /// Clay colour (BB_COLOR) — every shade on the figure derives from this.
    pub color: [u8; 3],
}

pub struct Sprite {
    font: Option<Font>,
}

impl Sprite {
    pub fn new() -> Self {
        Sprite { font: load_font() }
    }

    /// Render the body into a premultiplied-BGRA `wl_shm` canvas of size `w`×`h`.
    /// The pixmap matches the actual buffer so the row stride always matches;
    /// drawing uses surface-local coordinates and simply clips.
    pub fn paint(&self, canvas: &mut [u8], w: u32, h: u32, view: &BodyView) {
        let Some(mut pixmap) = Pixmap::new(w, h) else {
            return;
        };

        // Tucked: only the minimized bump — a dormant buddy hugging the edge.
        if let Some(edge) = view.tucked {
            draw_bump(&mut pixmap, &view.layout, edge, view.color);
            blit_premultiplied_bgra(pixmap.data(), canvas);
            return;
        }

        let bob = (view.t * std::f32::consts::TAU / 3.6).sin() * 3.0;
        // A ~150ms blink every 4s.
        let blinking = (view.t % 4.0) > 3.85;
        let face = view.emotion.face();
        let eye_open = if blinking { 0.10 } else { face.eye_open };
        let pose = FigurePose::idle(view.t);

        draw_figure(&mut pixmap, &view.layout, view.color, bob, &pose);
        draw_eyes(&mut pixmap, bob, eye_open, face.pupil_dy);
        draw_mouth(&mut pixmap, bob, &face.mouth);

        if let Some(text) = view.speech {
            if let Some(font) = &self.font {
                draw_bubble(&mut pixmap, font, &view.layout, text);
            }
        }

        if view.chat_open {
            if let Some(font) = &self.font {
                draw_input(
                    &mut pixmap,
                    font,
                    &view.layout,
                    view.input_text,
                    view.input_focused,
                    view.t,
                );
            }
        }

        blit_premultiplied_bgra(pixmap.data(), canvas);
    }
}

// --- colour helpers -----------------------------------------------------------------

fn rgb(c: [u8; 3]) -> Color {
    Color::from_rgba8(c[0], c[1], c[2], 255)
}

/// Multiply toward black (f < 1.0 darkens).
fn shade(c: [u8; 3], f: f32) -> [u8; 3] {
    [
        (c[0] as f32 * f).clamp(0.0, 255.0) as u8,
        (c[1] as f32 * f).clamp(0.0, 255.0) as u8,
        (c[2] as f32 * f).clamp(0.0, 255.0) as u8,
    ]
}

/// Blend toward white (f in 0..=1 lightens).
fn lighten(c: [u8; 3], f: f32) -> [u8; 3] {
    [
        (c[0] as f32 + (255.0 - c[0] as f32) * f) as u8,
        (c[1] as f32 + (255.0 - c[1] as f32) * f) as u8,
        (c[2] as f32 + (255.0 - c[2] as f32) * f) as u8,
    ]
}

// --- figure drawing -----------------------------------------------------------------

fn draw_figure(pixmap: &mut Pixmap, layout: &Layout, color: [u8; 3], bob: f32, pose: &FigurePose) {
    let hips_y = layout.hips_y();
    let limb = solid(rgb(shade(color, 0.94)));

    // Legs + feet (drawn first so the torso bottom overlaps the hip joins).
    let mut leg_stroke = Stroke::default();
    leg_stroke.width = LEG_W;
    leg_stroke.line_cap = tiny_skia::LineCap::Round;
    for sign in [-1.0_f32, 1.0] {
        let leg_x = FIG_CX + sign * 13.0;
        let mut pb = PathBuilder::new();
        pb.move_to(leg_x, hips_y - 8.0);
        pb.line_to(leg_x, hips_y + LEG_H);
        if let Some(path) = pb.finish() {
            pixmap.stroke_path(&path, &limb, &leg_stroke, Transform::identity(), None);
        }
        // Foot: a clay slab pointing outward.
        if let Some(foot) = ellipse_path(leg_x + sign * 7.0, hips_y + LEG_H + 5.0, 17.0, 7.0) {
            pixmap.fill_path(&foot, &limb, FillRule::Winding, Transform::identity(), None);
        }
    }

    // Torso: the stretchable rounded rectangle (10px sides), vertical clay shading.
    let torso = Rect {
        x: FIG_CX - TORSO_W / 2.0,
        y: TORSO_TOP,
        w: TORSO_W,
        h: layout.body_len,
    };
    let mut torso_paint = Paint::default();
    torso_paint.anti_alias = true;
    torso_paint.shader = tiny_skia::LinearGradient::new(
        tiny_skia::Point::from_xy(FIG_CX, TORSO_TOP),
        tiny_skia::Point::from_xy(FIG_CX, hips_y),
        vec![
            tiny_skia::GradientStop::new(0.0, rgb(lighten(color, 0.10))),
            tiny_skia::GradientStop::new(1.0, rgb(shade(color, 0.78))),
        ],
        tiny_skia::SpreadMode::Pad,
        Transform::identity(),
    )
    .unwrap_or_else(|| Shader::SolidColor(rgb(color)));
    fill_round_rect(pixmap, torso, TORSO_R, &torso_paint);

    // Arms from joint angles (the future-animation seam).
    let mut arm_stroke = Stroke::default();
    arm_stroke.width = ARM_W;
    arm_stroke.line_cap = tiny_skia::LineCap::Round;
    arm_stroke.line_join = tiny_skia::LineJoin::Round;
    for (sign, arm) in [(-1.0_f32, &pose.left_arm), (1.0, &pose.right_arm)] {
        let shoulder = (FIG_CX + sign * (TORSO_W / 2.0 - 4.0), TORSO_TOP + 14.0);
        let a1 = arm.shoulder.to_radians();
        let elbow = (
            shoulder.0 + sign * a1.sin() * ARM_UPPER,
            shoulder.1 + a1.cos() * ARM_UPPER,
        );
        let a2 = (arm.shoulder + arm.elbow).to_radians();
        let hand = (elbow.0 + sign * a2.sin() * ARM_FORE, elbow.1 + a2.cos() * ARM_FORE);

        let mut pb = PathBuilder::new();
        pb.move_to(shoulder.0, shoulder.1);
        pb.line_to(elbow.0, elbow.1);
        pb.line_to(hand.0, hand.1);
        if let Some(path) = pb.finish() {
            pixmap.stroke_path(&path, &limb, &arm_stroke, Transform::identity(), None);
        }
        // Mitten hand — Morph's hands are proportionally big.
        if let Some(h) = PathBuilder::from_circle(hand.0, hand.1, HAND_R) {
            pixmap.fill_path(&h, &limb, FillRule::Winding, Transform::identity(), None);
        }
    }

    // Head last so it sits on the shoulders (no neck — clay).
    let mut head_paint = Paint::default();
    head_paint.anti_alias = true;
    head_paint.shader = tiny_skia::LinearGradient::new(
        tiny_skia::Point::from_xy(FIG_CX, HEAD_CY - HEAD_R + bob),
        tiny_skia::Point::from_xy(FIG_CX, HEAD_CY + HEAD_R + bob),
        vec![
            tiny_skia::GradientStop::new(0.0, rgb(lighten(color, 0.22))),
            tiny_skia::GradientStop::new(0.55, rgb(color)),
            tiny_skia::GradientStop::new(1.0, rgb(shade(color, 0.72))),
        ],
        tiny_skia::SpreadMode::Pad,
        Transform::identity(),
    )
    .unwrap_or_else(|| Shader::SolidColor(rgb(color)));
    if let Some(circle) = PathBuilder::from_circle(FIG_CX, HEAD_CY + bob, HEAD_R) {
        pixmap.fill_path(&circle, &head_paint, FillRule::Winding, Transform::identity(), None);
    }

    draw_clay_texture(pixmap, layout, color, bob);
}

/// Texture bonus: subtle thumb-smudge marks so the figure reads as worked clay,
/// not vector plastic. Deterministic (no per-frame shimmer).
fn draw_clay_texture(pixmap: &mut Pixmap, layout: &Layout, color: [u8; 3], bob: f32) {
    // Sheen highlight on the head's upper-left.
    let hl = Color::from_rgba8(255, 255, 255, 34);
    if let Some(e) = ellipse_path(FIG_CX - 14.0, HEAD_CY - 18.0 + bob, 16.0, 10.0) {
        pixmap.fill_path(&e, &solid(hl), FillRule::Winding, Transform::identity(), None);
    }

    // Smudges down the torso: alternating slightly-darker / slightly-lighter
    // streaks from a tiny LCG so they're stable frame to frame.
    let dark = shade(color, 0.85);
    let light = lighten(color, 0.12);
    let mut seed: u32 = 0x5EED_C1A7;
    let torso_x = FIG_CX - TORSO_W / 2.0;
    let n = ((layout.body_len / 26.0) as usize).clamp(3, 12);
    for i in 0..n {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let rx = 5.0 + (seed >> 8 & 0x7) as f32; // 5..12
        let ry = 2.0 + (seed >> 12 & 0x3) as f32; // 2..5
        let x = torso_x + 10.0 + (seed >> 16 & 0x1F) as f32 % (TORSO_W - 20.0);
        let y = TORSO_TOP + 16.0 + (i as f32 + 0.5) * (layout.body_len - 28.0) / n as f32;
        let c = if i % 2 == 0 { dark } else { light };
        let paint = solid(Color::from_rgba8(c[0], c[1], c[2], 14));
        if let Some(e) = ellipse_path(x, y, rx, ry) {
            pixmap.fill_path(&e, &paint, FillRule::Winding, Transform::identity(), None);
        }
    }
}

/// The tucked bump: a half-disc in the buddy's clay colour hugging the edge,
/// with a cyan "presence" dot toward the visible side so it reads as alive.
fn draw_bump(pixmap: &mut Pixmap, layout: &Layout, edge: BumpEdge, color: [u8; 3]) {
    let (cx, cy) = layout.bump_center(edge);

    let outer = solid(rgb(color));
    if let Some(c) = PathBuilder::from_circle(cx, cy, BUMP_R) {
        pixmap.fill_path(&c, &outer, FillRule::Winding, Transform::identity(), None);
    }
    let inner = solid(rgb(shade(color, 0.45)));
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

/// Big white stop-motion eyes with dark pupils — the Morph look.
fn draw_eyes(pixmap: &mut Pixmap, bob: f32, eye_open: f32, pupil_dy: f32) {
    let white = solid(Color::from_rgba8(250, 250, 248, 255));
    let dark = solid(Color::from_rgba8(28, 22, 18, 255));
    for sign in [-1.0_f32, 1.0] {
        let ex = FIG_CX + sign * 18.0;
        let ey = HEAD_CY - 10.0 + bob;
        if let Some(eye) = ellipse_path(ex, ey, 11.0, 14.0 * eye_open) {
            pixmap.fill_path(&eye, &white, FillRule::Winding, Transform::identity(), None);
        }
        if eye_open > 0.35 {
            if let Some(pupil) = PathBuilder::from_circle(ex + sign * 2.0, ey + 5.0 * pupil_dy, 4.5) {
                pixmap.fill_path(&pupil, &dark, FillRule::Winding, Transform::identity(), None);
            }
        }
    }
}

fn draw_mouth(pixmap: &mut Pixmap, bob: f32, mouth: &Mouth) {
    let mut stroke = Stroke::default();
    stroke.width = 4.0;
    stroke.line_cap = tiny_skia::LineCap::Round;
    let ink = solid(Color::from_rgba8(40, 24, 16, 255));
    let my = HEAD_CY + 22.0 + bob;

    match *mouth {
        Mouth::Smile(amount) => {
            let mut pb = PathBuilder::new();
            pb.move_to(FIG_CX - 20.0, my);
            pb.quad_to(FIG_CX, my + 20.0 * amount, FIG_CX + 20.0, my);
            if let Some(path) = pb.finish() {
                pixmap.stroke_path(&path, &ink, &stroke, Transform::identity(), None);
            }
        }
        Mouth::Flat => {
            let mut pb = PathBuilder::new();
            pb.move_to(FIG_CX - 14.0, my + 4.0);
            pb.line_to(FIG_CX + 14.0, my + 4.0);
            if let Some(path) = pb.finish() {
                pixmap.stroke_path(&path, &ink, &stroke, Transform::identity(), None);
            }
        }
        Mouth::Spec(spec) => draw_mouth_spec(pixmap, FIG_CX, my + 4.0, &spec),
    }
}

/// Draw a parameterized clay mouth: dark cavity, optional teeth bands clipped to
/// the cavity (the ellipse edge rounds their corners like pressed clay), tongue.
fn draw_mouth_spec(pixmap: &mut Pixmap, cx: f32, cy: f32, spec: &MouthSpec) {
    let ink = solid(Color::from_rgba8(40, 24, 16, 255));
    let rx = spec.rx;
    let ry = 3.0 + 11.0 * spec.open;

    if spec.open <= 0.05 {
        // Pressed-shut lips ('M', 'B', 'P'): just a firm line.
        let mut stroke = Stroke::default();
        stroke.width = 4.5;
        stroke.line_cap = tiny_skia::LineCap::Round;
        let mut pb = PathBuilder::new();
        pb.move_to(cx - rx, cy);
        pb.line_to(cx + rx, cy);
        if let Some(path) = pb.finish() {
            pixmap.stroke_path(&path, &ink, &stroke, Transform::identity(), None);
        }
        return;
    }

    let Some(cavity) = ellipse_path(cx, cy, rx, ry) else { return };
    pixmap.fill_path(&cavity, &ink, FillRule::Winding, Transform::identity(), None);

    let clip = Mask::new(pixmap.width(), pixmap.height()).map(|mut m| {
        m.fill_path(&cavity, FillRule::Winding, true, Transform::identity());
        m
    });
    let clip = clip.as_ref();

    if spec.tongue {
        let tongue = solid(Color::from_rgba8(232, 92, 110, 255));
        if let Some(t) = ellipse_path(cx, cy + ry * 0.42, rx * 0.72, ry * 0.62) {
            pixmap.fill_path(&t, &tongue, FillRule::Winding, Transform::identity(), clip);
        }
    }

    let white = solid(Color::from_rgba8(248, 250, 252, 255));
    let teeth_h = (ry * 0.55).min(7.0);
    let mut bands: Vec<f32> = Vec::new();
    if spec.teeth_top {
        bands.push(cy - ry);
    }
    if spec.teeth_bottom {
        bands.push(cy + ry - teeth_h);
    }
    for top in bands {
        let mut band = PathBuilder::new();
        band.move_to(cx - rx, top);
        band.line_to(cx + rx, top);
        band.line_to(cx + rx, top + teeth_h);
        band.line_to(cx - rx, top + teeth_h);
        band.close();
        if let Some(path) = band.finish() {
            pixmap.fill_path(&path, &white, FillRule::Winding, Transform::identity(), clip);
        }
        // Thin separators suggesting individual teeth.
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

// --- bubble + input ----------------------------------------------------------------

/// Speech bubble: auto-sizes its height to the wrapped text (≤6 lines) and faces
/// inward, with the tail pointing at the head.
fn draw_bubble(pixmap: &mut Pixmap, font: &Font, layout: &Layout, text: &str) {
    let max = layout.bubble_rect();
    let pad_x = 14.0;
    let pad_top = 28.0;
    let lines = wrap(font, text, TEXT_PX, max.w - pad_x * 2.0, BUBBLE_MAX_LINES);
    let h = pad_top + lines.len().max(1) as f32 * LINE_H + 12.0;
    let rect = Rect { x: max.x, y: max.y, w: max.w, h };

    let bg = Color::from_rgba8(247, 251, 255, 245);
    draw_round_rect(pixmap, rect, bg);

    // Tail on the head-facing edge.
    let (tail_x, dir) = match layout.facing {
        Facing::Right => (rect.x, -1.0_f32),
        Facing::Left => (rect.x + rect.w, 1.0),
    };
    let mut pb = PathBuilder::new();
    pb.move_to(tail_x, rect.y + 30.0);
    pb.line_to(tail_x + dir * 16.0, rect.y + 46.0);
    pb.line_to(tail_x, rect.y + 56.0);
    pb.close();
    if let Some(path) = pb.finish() {
        pixmap.fill_path(&path, &solid(bg), FillRule::Winding, Transform::identity(), None);
    }

    let mut baseline = rect.y + pad_top;
    for line in &lines {
        draw_line(pixmap, font, line, rect.x + pad_x, baseline, TEXT_PX, [16, 24, 44]);
        baseline += LINE_H;
    }
}

/// The on-body chat input: expands to fit the typed text (up to
/// `INPUT_MAX_LINES`, then scrolls), brighter when focused, blinking caret.
fn draw_input(pixmap: &mut Pixmap, font: &Font, layout: &Layout, text: &str, focused: bool, t: f32) {
    let pad = 12.0;
    let max_w = BUBBLE_W - pad * 2.0;

    // Wrap the whole text, then keep the tail — the newest words stay visible.
    let all = wrap(font, text, TEXT_PX, max_w, usize::MAX);
    let start = all.len().saturating_sub(INPUT_MAX_LINES);
    let shown: &[String] = &all[start..];

    let rect = layout.input_rect(shown.len().max(1));
    let bg = if focused {
        Color::from_rgba8(255, 255, 255, 250)
    } else {
        Color::from_rgba8(232, 226, 220, 235)
    };
    draw_round_rect(pixmap, rect, bg);

    let mut baseline = rect.y + 8.0 + TEXT_PX;
    if text.is_empty() {
        if !focused {
            draw_line(pixmap, font, "Type to me…", rect.x + pad, baseline, TEXT_PX, [130, 122, 114]);
        }
    } else {
        for line in shown {
            draw_line(pixmap, font, line, rect.x + pad, baseline, TEXT_PX, [30, 22, 16]);
            baseline += LINE_H;
        }
        baseline -= LINE_H; // caret sits on the last drawn line
    }

    // Blinking caret (~1.4 Hz) at the end of the last line.
    if focused && ((t * 1.4) as i32) % 2 == 0 {
        let last = shown.last().map(String::as_str).unwrap_or("");
        let caret_x = rect.x + pad + measure(font, last, TEXT_PX) + 2.0;
        let caret = Rect { x: caret_x, y: baseline - TEXT_PX, w: 2.0, h: TEXT_PX + 4.0 };
        draw_round_rect(pixmap, caret, Color::from_rgba8(201, 109, 60, 255));
    }
}

// --- primitives ---------------------------------------------------------------------

fn draw_round_rect(pixmap: &mut Pixmap, rect: Rect, color: Color) {
    let r = 14.0_f32.min(rect.w / 2.0).min(rect.h / 2.0);
    fill_round_rect(pixmap, rect, r, &solid(color));
}

fn fill_round_rect(pixmap: &mut Pixmap, rect: Rect, radius: f32, paint: &Paint) {
    let r = radius.min(rect.w / 2.0).min(rect.h / 2.0);
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
        pixmap.fill_path(&path, paint, FillRule::Winding, Transform::identity(), None);
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
    eprintln!("[bb-desktop-body] no system font found — bubble/input text disabled");
    None
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surface_grows_with_body_stretch() {
        let short = Layout { facing: Facing::Right, body_len: BODY_LEN_MIN };
        let tall = Layout { facing: Facing::Right, body_len: BODY_LEN_MAX };
        assert!(tall.surface_h() > short.surface_h());
        // Even fully squashed, the UI column still fits.
        assert!(short.surface_h() >= UI_MIN_H as u32);
    }

    #[test]
    fn ui_flips_to_face_inward() {
        let right = Layout { facing: Facing::Right, body_len: BODY_LEN_DEFAULT };
        let left = Layout { facing: Facing::Left, body_len: BODY_LEN_DEFAULT };
        // Facing right: UI sits right of the figure; facing left: entirely left of it.
        assert!(right.bubble_rect().x > FIG_CX);
        assert!(left.bubble_rect().x + left.bubble_rect().w < FIG_CX);
        assert!(right.input_rect(1).x > FIG_CX);
        assert!(left.input_rect(1).x + left.input_rect(1).w < FIG_CX);
    }

    #[test]
    fn input_expands_with_lines_up_to_cap() {
        let l = Layout::initial();
        let one = l.input_rect(1).h;
        let three = l.input_rect(3).h;
        assert!(three > one);
        // Capped: more lines than the max never outgrow the region rect.
        assert_eq!(l.input_rect(9).h, l.input_region_rect().h);
    }

    #[test]
    fn feet_sit_below_the_torso() {
        let l = Layout::initial();
        assert!(l.feet_rect().y >= TORSO_TOP + l.body_len - 8.0);
    }

    #[test]
    fn viseme_specs_are_sane() {
        for v in [
            Viseme::Rest,
            Viseme::AI,
            Viseme::OUWQ,
            Viseme::Cdgknrs,
            Viseme::ThL,
            Viseme::FV,
            Viseme::Mbp,
            Viseme::Agh,
        ] {
            let s = viseme_spec(v);
            assert!((0.0..=1.0).contains(&s.open), "{v:?} open out of range");
            assert!(s.rx > 0.0);
        }
        // The pressed-lips viseme draws as a closed mouth.
        assert!(viseme_spec(Viseme::Mbp).open <= 0.05);
    }
}
