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
    Color, FillRule, FilterQuality, Mask, Paint, PathBuilder, Pixmap, PixmapPaint, Shader,
    Stroke, Transform,
};

// --- figure geometry -------------------------------------------------------------

pub const SURFACE_W: u32 = 560;
/// Figure centreline. UI (bubble/input) flips to either side of this.
pub const FIG_CX: f32 = 280.0;
pub const HEAD_CY: f32 = 58.0;
pub const HEAD_R: f32 = 44.0;
/// Torso top — tucks up under the chin (Morph has no neck).
const TORSO_TOP: f32 = HEAD_CY + HEAD_R - 12.0;
/// Stretchable clay torso; wide enough to host the provider output pane.
pub const TORSO_W: f32 = 142.0;
const TORSO_R: f32 = 14.0;
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
const PANEL_TEXT_PX: f32 = 12.0;
const PANEL_LINE_H: f32 = PANEL_TEXT_PX * 1.25;
pub const INPUT_MAX_LINES: usize = 3;
/// Gap between the figure and its UI column.
const UI_GAP: f32 = 16.0;
const PANEL_LABEL_PX: f32 = 10.0;

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
        let figure_half = (TORSO_W / 2.0).max(HEAD_R);
        match self.facing {
            Facing::Right => FIG_CX + figure_half + UI_GAP,
            Facing::Left => FIG_CX - figure_half - UI_GAP - w,
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
        let w = TORSO_W + 24.0;
        Rect {
            x: FIG_CX - w / 2.0,
            y: self.hips_y() - 6.0,
            w,
            h: LEG_H + FOOT_H + BOTTOM_PAD + 6.0,
        }
    }

    pub fn torso_rect(&self) -> Rect {
        Rect {
            x: FIG_CX - TORSO_W / 2.0,
            y: TORSO_TOP,
            w: TORSO_W,
            h: self.body_len,
        }
    }

    pub fn output_panel_rect(&self) -> Rect {
        let torso = self.torso_rect();
        let pad = 10.0;
        Rect {
            x: torso.x + pad,
            y: torso.y + pad,
            w: torso.w - pad * 2.0,
            h: (torso.h - pad * 2.0).max(0.0),
        }
    }

}

// --- tucked bump (minimized, parked flush against a screen edge) -------------------
//
// Bump geometry takes the *actual* surface size explicitly rather than reading
// layout constants: the compositor can transiently shrink the surface while it
// crosses a screen edge, and the tab must hug the buffer edge that really exists
// - never a theoretical one - or the buddy vanishes off the side of its own
// buffer with nothing left to click.

/// Centre of the bump's full circle - ON the surface edge so only the
/// on-surface half shows (the off-surface half clips = the "split in half").
fn bump_center(edge: BumpEdge, w: u32, h: u32) -> (f32, f32) {
    let (w, h) = (w as f32, h as f32);
    let cy = if h >= BUMP_R * 2.0 { HEAD_CY.clamp(BUMP_R, h - BUMP_R) } else { h / 2.0 };
    let cx = if w >= BUMP_R * 2.0 { FIG_CX.clamp(BUMP_R, w - BUMP_R) } else { w / 2.0 };
    match edge {
        BumpEdge::Left => (0.0, cy),
        BumpEdge::Right => (w, cy),
        BumpEdge::Top => (cx, 0.0),
        BumpEdge::Bottom => (cx, h),
    }
}

/// Bounding box of the visible half of the bump - input region + on-screen
/// clamping while tucked.
pub fn bump_rect(edge: BumpEdge, w: u32, h: u32) -> Rect {
    let (cx, cy) = bump_center(edge, w, h);
    let rect = match edge {
        BumpEdge::Left => Rect { x: 0.0, y: cy - BUMP_R, w: BUMP_R, h: BUMP_R * 2.0 },
        BumpEdge::Right => Rect { x: cx - BUMP_R, y: cy - BUMP_R, w: BUMP_R, h: BUMP_R * 2.0 },
        BumpEdge::Top => Rect { x: cx - BUMP_R, y: 0.0, w: BUMP_R * 2.0, h: BUMP_R },
        BumpEdge::Bottom => Rect { x: cx - BUMP_R, y: cy - BUMP_R, w: BUMP_R * 2.0, h: BUMP_R },
    };
    clip_rect_to_surface(rect, w, h)
}

fn clip_rect_to_surface(rect: Rect, w: u32, h: u32) -> Rect {
    let (w, h) = (w as f32, h as f32);
    let x1 = rect.x.clamp(0.0, w);
    let y1 = rect.y.clamp(0.0, h);
    let x2 = (rect.x + rect.w).clamp(0.0, w);
    let y2 = (rect.y + rect.h).clamp(0.0, h);
    Rect { x: x1, y: y1, w: (x2 - x1).max(0.0), h: (y2 - y1).max(0.0) }
}

pub fn point_in_bump(edge: BumpEdge, w: u32, h: u32, px: f64, py: f64) -> bool {
    let (cx, cy) = bump_center(edge, w, h);
    let dx = px as f32 - cx;
    let dy = py as f32 - cy;
    dx * dx + dy * dy <= BUMP_R * BUMP_R
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

pub struct SessionCard<'a> {
    pub buddy: &'a str,
    pub provider: &'a str,
    pub model: &'a str,
    pub gateway: &'a str,
    pub status: &'a str,
    pub note: &'a str,
}

pub struct TextCard<'a> {
    pub title: &'a str,
    pub body: &'a str,
}

pub struct MediaStubCard<'a> {
    pub title: &'a str,
    pub caption: &'a str,
    pub hint: &'a str,
}

#[derive(Clone, Copy)]
pub enum BuiltinImageAsset {
    EiffelTower,
}

pub struct ImageCard {
    pub asset: BuiltinImageAsset,
}

pub enum TorsoOutput<'a> {
    Session(SessionCard<'a>),
    Text(TextCard<'a>),
    Image(ImageCard),
    ImageStub(MediaStubCard<'a>),
    FileStub(MediaStubCard<'a>),
}

pub struct BodyView<'a> {
    /// Seconds since start — drives bob, blink, and limb sway.
    pub t: f32,
    pub emotion: Emotion,
    pub speech: Option<&'a str>,
    /// Provider output rendered into the body torso. Output-only; it never grants the
    /// body authority to act or run provider tools.
    pub torso_output: TorsoOutput<'a>,
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
    eiffel_tower: Option<Pixmap>,
}

impl Sprite {
    pub fn new() -> Self {
        Sprite {
            font: load_font(),
            eiffel_tower: load_builtin_image(include_bytes!("../assets/eiffel-tower.jpg")),
        }
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
            draw_bump(&mut pixmap, edge, w, h, view.color);
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
        if let Some(font) = &self.font {
            draw_torso_output(
                &mut pixmap,
                font,
                &view.layout,
                &view.torso_output,
                self.eiffel_tower.as_ref(),
            );
        }
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
    let torso = layout.torso_rect();
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

/// The tucked bump: a sleeping clay head half-disc hugging the actual buffer edge.
fn draw_bump(pixmap: &mut Pixmap, edge: BumpEdge, w: u32, h: u32, color: [u8; 3]) {
    let (cx, cy) = bump_center(edge, w, h);

    let outer = solid(rgb(color));
    if let Some(c) = PathBuilder::from_circle(cx, cy, BUMP_R) {
        pixmap.fill_path(&c, &outer, FillRule::Winding, Transform::identity(), None);
    }
    let inner = solid(rgb(shade(color, 0.45)));
    if let Some(c) = PathBuilder::from_circle(cx, cy, BUMP_R - 7.0) {
        pixmap.fill_path(&c, &inner, FillRule::Winding, Transform::identity(), None);
    }

    // Nudge the sleeping face toward the on-screen side of the bump.
    let (dx, dy) = match edge {
        BumpEdge::Left => (BUMP_R * 0.45, 0.0),
        BumpEdge::Right => (-BUMP_R * 0.45, 0.0),
        BumpEdge::Top => (0.0, BUMP_R * 0.45),
        BumpEdge::Bottom => (0.0, -BUMP_R * 0.45),
    };
    draw_closed_eyes(pixmap, cx + dx, cy + dy - 2.0);
}

fn draw_closed_eyes(pixmap: &mut Pixmap, x: f32, y: f32) {
    let dark = solid(Color::from_rgba8(38, 28, 22, 230));
    let mut stroke = Stroke::default();
    stroke.width = 3.0;
    stroke.line_cap = tiny_skia::LineCap::Round;
    for eye_x in [x - 8.0, x + 8.0] {
        let mut pb = PathBuilder::new();
        pb.move_to(eye_x - 5.0, y);
        pb.quad_to(eye_x, y + 3.0, eye_x + 5.0, y);
        if let Some(path) = pb.finish() {
            pixmap.stroke_path(&path, &dark, &stroke, Transform::identity(), None);
        }
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

fn draw_torso_output(
    pixmap: &mut Pixmap,
    font: &Font,
    layout: &Layout,
    output: &TorsoOutput,
    eiffel_tower: Option<&Pixmap>,
) {
    let rect = layout.output_panel_rect();
    if rect.w <= 0.0 || rect.h <= 0.0 {
        return;
    }

    let bg = Color::from_rgba8(246, 249, 250, 226);
    let rim = solid(Color::from_rgba8(103, 69, 48, 95));
    draw_round_rect(pixmap, rect, bg);
    if let Some(path) = round_rect_path(rect, 8.0) {
        let mut stroke = Stroke::default();
        stroke.width = 1.5;
        pixmap.stroke_path(&path, &rim, &stroke, Transform::identity(), None);
    }

    match output {
        TorsoOutput::Session(card) => draw_session_card(pixmap, font, rect, card),
        TorsoOutput::Text(card) => draw_text_card(pixmap, font, rect, card),
        TorsoOutput::Image(card) => draw_image_card(pixmap, rect, card, eiffel_tower),
        TorsoOutput::ImageStub(card) => draw_media_stub(pixmap, font, rect, card, true),
        TorsoOutput::FileStub(card) => draw_media_stub(pixmap, font, rect, card, false),
    }
}

fn draw_session_card(pixmap: &mut Pixmap, font: &Font, rect: Rect, card: &SessionCard) {
    let pad = 8.0;
    let mut y = rect.y + pad + 12.0;
    let text_w = rect.w - pad * 2.0;

    y = draw_wrapped_block(
        pixmap,
        font,
        rect.x + pad,
        y,
        PANEL_LABEL_PX,
        PANEL_LINE_H - 1.0,
        text_w,
        1,
        &format!("{} surface", title_case(card.buddy)),
        [102, 88, 76],
    ) + 4.0;
    y = draw_wrapped_block(
        pixmap,
        font,
        rect.x + pad,
        y,
        14.0,
        15.0,
        text_w,
        2,
        card.status,
        [38, 34, 32],
    ) + 5.0;

    for (line, max_lines) in [
        (format!("Provider: {}", card.provider), 1usize),
        (format!("Model: {}", card.model), 2usize),
        (format!("Link: {}", card.gateway), 3usize),
    ] {
        y = draw_wrapped_block(
            pixmap,
            font,
            rect.x + pad,
            y,
            PANEL_TEXT_PX,
            PANEL_LINE_H,
            text_w,
            max_lines,
            &line,
            [63, 56, 52],
        ) + 2.0;
    }

    let note_y = y + 4.0;
    let note_h = (rect.y + rect.h - pad - note_y).max(0.0);
    if note_h >= PANEL_TEXT_PX + 10.0 {
        let note_rect = Rect {
            x: rect.x + pad,
            y: note_y,
            w: text_w,
            h: note_h,
        };
        let note_bg = Color::from_rgba8(255, 255, 255, 186);
        draw_round_rect(pixmap, note_rect, note_bg);
        let note_lines = ((note_rect.h - 10.0) / PANEL_LINE_H).floor().max(1.0) as usize;
        let _ = draw_wrapped_block(
            pixmap,
            font,
            note_rect.x + 5.0,
            note_rect.y + 7.0,
            PANEL_TEXT_PX,
            PANEL_LINE_H,
            note_rect.w - 10.0,
            note_lines,
            card.note,
            [88, 74, 64],
        );
    }
}

fn draw_text_card(pixmap: &mut Pixmap, font: &Font, rect: Rect, card: &TextCard) {
    let pad = 8.0;
    let mut y = rect.y + pad + 12.0;
    draw_line(pixmap, font, card.title, rect.x + pad, y, PANEL_LABEL_PX, [102, 88, 76]);
    y += PANEL_LABEL_PX + 8.0;
    let max_lines = ((rect.h - (y - rect.y) - pad) / PANEL_LINE_H).floor().max(1.0) as usize;
    let lines = wrap(font, card.body, PANEL_TEXT_PX, rect.w - pad * 2.0, usize::MAX);
    let start = lines.len().saturating_sub(max_lines);
    let mut baseline = y + PANEL_TEXT_PX;
    for line in &lines[start..] {
        draw_line(pixmap, font, line, rect.x + pad, baseline, PANEL_TEXT_PX, [30, 26, 24]);
        baseline += PANEL_LINE_H;
    }
}

fn draw_wrapped_block(
    pixmap: &mut Pixmap,
    font: &Font,
    x: f32,
    top: f32,
    px: f32,
    line_h: f32,
    max_w: f32,
    max_lines: usize,
    text: &str,
    color: [u8; 3],
) -> f32 {
    let lines = wrap(font, text, px, max_w, max_lines);
    let mut baseline = top + px;
    for line in &lines {
        draw_line(pixmap, font, line, x, baseline, px, color);
        baseline += line_h;
    }
    if lines.is_empty() {
        top
    } else {
        baseline - line_h
    }
}

fn draw_media_stub(
    pixmap: &mut Pixmap,
    font: &Font,
    rect: Rect,
    card: &MediaStubCard,
    is_image: bool,
) {
    let pad = 8.0;
    let thumb = Rect {
        x: rect.x + pad,
        y: rect.y + pad + 6.0,
        w: rect.w - pad * 2.0,
        h: (rect.h * 0.42).min(50.0).max(34.0),
    };
    draw_round_rect(pixmap, thumb, Color::from_rgba8(238, 233, 228, 255));
    if is_image {
        draw_image_stub_icon(pixmap, thumb);
    } else {
        draw_file_stub_icon(pixmap, thumb);
    }

    let mut y = thumb.y + thumb.h + 14.0;
    draw_line(pixmap, font, card.title, rect.x + pad, y, PANEL_LABEL_PX, [102, 88, 76]);
    y += PANEL_LABEL_PX + 8.0;
    for line in wrap(font, card.caption, PANEL_TEXT_PX, rect.w - pad * 2.0, 2) {
        draw_line(pixmap, font, &line, rect.x + pad, y, PANEL_TEXT_PX, [30, 26, 24]);
        y += PANEL_LINE_H;
    }
    for line in wrap(font, card.hint, PANEL_TEXT_PX, rect.w - pad * 2.0, 2) {
        draw_line(pixmap, font, &line, rect.x + pad, y, PANEL_TEXT_PX, [102, 88, 76]);
        y += PANEL_LINE_H;
    }
}

fn draw_image_card(
    pixmap: &mut Pixmap,
    rect: Rect,
    card: &ImageCard,
    eiffel_tower: Option<&Pixmap>,
) {
    let pad = 8.0;
    let image_rect = Rect {
        x: rect.x + pad,
        y: rect.y + pad + 6.0,
        w: rect.w - pad * 2.0,
        h: rect.h - pad * 2.0 - 6.0,
    };
    draw_round_rect(pixmap, image_rect, Color::from_rgba8(236, 232, 228, 255));
    if let Some(asset) = builtin_image_pixmap(card.asset, eiffel_tower) {
        draw_fitted_image(pixmap, asset, image_rect);
    }
    if let Some(path) = round_rect_path(image_rect, 8.0) {
        let mut stroke = Stroke::default();
        stroke.width = 1.0;
        pixmap.stroke_path(
            &path,
            &solid(Color::from_rgba8(103, 69, 48, 95)),
            &stroke,
            Transform::identity(),
            None,
        );
    }
}

fn builtin_image_pixmap<'a>(asset: BuiltinImageAsset, eiffel_tower: Option<&'a Pixmap>) -> Option<&'a Pixmap> {
    match asset {
        BuiltinImageAsset::EiffelTower => eiffel_tower,
    }
}

fn draw_fitted_image(pixmap: &mut Pixmap, image: &Pixmap, rect: Rect) {
    let scale = (rect.w / image.width() as f32).min(rect.h / image.height() as f32);
    let draw_w = image.width() as f32 * scale;
    let draw_h = image.height() as f32 * scale;
    let dx = rect.x + (rect.w - draw_w) / 2.0;
    let dy = rect.y + (rect.h - draw_h) / 2.0;
    let mut paint = PixmapPaint::default();
    paint.quality = FilterQuality::Bilinear;
    pixmap.draw_pixmap(
        0,
        0,
        image.as_ref(),
        &paint,
        Transform::from_row(scale, 0.0, 0.0, scale, dx, dy),
        None,
    );
}

fn draw_image_stub_icon(pixmap: &mut Pixmap, rect: Rect) {
    let stroke = solid(Color::from_rgba8(166, 136, 112, 255));
    let mut pb = PathBuilder::new();
    pb.move_to(rect.x + 8.0, rect.y + rect.h - 10.0);
    pb.line_to(rect.x + rect.w * 0.34, rect.y + rect.h * 0.48);
    pb.line_to(rect.x + rect.w * 0.54, rect.y + rect.h - 16.0);
    pb.line_to(rect.x + rect.w * 0.72, rect.y + rect.h * 0.38);
    pb.line_to(rect.x + rect.w - 8.0, rect.y + rect.h - 10.0);
    if let Some(path) = pb.finish() {
        let mut line = Stroke::default();
        line.width = 2.0;
        line.line_cap = tiny_skia::LineCap::Round;
        line.line_join = tiny_skia::LineJoin::Round;
        pixmap.stroke_path(&path, &stroke, &line, Transform::identity(), None);
    }
    if let Some(dot) = PathBuilder::from_circle(rect.x + rect.w - 18.0, rect.y + 14.0, 4.0) {
        pixmap.fill_path(&dot, &stroke, FillRule::Winding, Transform::identity(), None);
    }
}

fn draw_file_stub_icon(pixmap: &mut Pixmap, rect: Rect) {
    let doc = Rect {
        x: rect.x + rect.w * 0.32,
        y: rect.y + 7.0,
        w: rect.w * 0.36,
        h: rect.h - 14.0,
    };
    draw_round_rect(pixmap, doc, Color::from_rgba8(255, 255, 255, 225));
    let ink = solid(Color::from_rgba8(166, 136, 112, 255));
    for i in 0..3 {
        let y = doc.y + 12.0 + i as f32 * 8.0;
        let mut pb = PathBuilder::new();
        pb.move_to(doc.x + 8.0, y);
        pb.line_to(doc.x + doc.w - 8.0, y);
        if let Some(path) = pb.finish() {
            let mut line = Stroke::default();
            line.width = 1.5;
            line.line_cap = tiny_skia::LineCap::Round;
            pixmap.stroke_path(&path, &ink, &line, Transform::identity(), None);
        }
    }
}

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
    let Some(path) = round_rect_path(rect, radius) else { return };
    pixmap.fill_path(&path, paint, FillRule::Winding, Transform::identity(), None);
}

fn round_rect_path(rect: Rect, radius: f32) -> Option<tiny_skia::Path> {
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
    pb.finish()
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

fn title_case(text: &str) -> String {
    let mut chars = text.chars();
    let Some(first) = chars.next() else { return String::new() };
    let mut out = first.to_uppercase().to_string();
    out.push_str(chars.as_str());
    out
}

fn load_builtin_image(bytes: &[u8]) -> Option<Pixmap> {
    let decoded = image::load_from_memory(bytes).ok()?.to_rgba8();
    let (w, h) = decoded.dimensions();
    let mut rgba = decoded.into_raw();
    for px in rgba.chunks_exact_mut(4) {
        let alpha = px[3] as u16;
        px[0] = ((px[0] as u16 * alpha) / 255) as u8;
        px[1] = ((px[1] as u16 * alpha) / 255) as u8;
        px[2] = ((px[2] as u16 * alpha) / 255) as u8;
    }
    let size = tiny_skia::IntSize::from_wh(w, h)?;
    Pixmap::from_vec(rgba, size)
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
    let mut last_break: Option<usize> = None;

    for ch in text.chars() {
        if ch == '\n' {
            push_wrapped_line(&mut lines, &mut current, &mut last_break, max_lines);
            if lines.len() == max_lines {
                break;
            }
            continue;
        }

        current.push(ch);
        if ch.is_whitespace() {
            last_break = Some(current.len());
        }

        if measure(font, &current, px) <= max_w {
            continue;
        }

        if let Some(idx) = last_break {
            let mut overflow = current[idx..].trim_start().to_string();
            current.truncate(idx);
            trim_line_end(&mut current);
            push_wrapped_line(&mut lines, &mut current, &mut last_break, max_lines);
            if lines.len() == max_lines {
                break;
            }
            current = std::mem::take(&mut overflow);
            last_break = find_break_idx(&current);
            while measure(font, &current, px) > max_w && !current.is_empty() {
                hard_wrap_current(font, px, max_w, &mut lines, &mut current, &mut last_break, max_lines);
                if lines.len() == max_lines {
                    break;
                }
            }
        } else {
            hard_wrap_current(font, px, max_w, &mut lines, &mut current, &mut last_break, max_lines);
            if lines.len() == max_lines {
                break;
            }
        }
    }

    if lines.len() < max_lines && !current.is_empty() {
        trim_line_end(&mut current);
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

fn hard_wrap_current(
    font: &Font,
    px: f32,
    max_w: f32,
    lines: &mut Vec<String>,
    current: &mut String,
    last_break: &mut Option<usize>,
    max_lines: usize,
) {
    let mut carry_rev = String::new();
    while measure(font, current, px) > max_w {
        let Some(ch) = current.pop() else { break };
        carry_rev.push(ch);
    }
    trim_line_end(current);
    push_wrapped_line(lines, current, last_break, max_lines);
    if lines.len() == max_lines {
        return;
    }
    *current = carry_rev.chars().rev().collect::<String>().trim_start().to_string();
    *last_break = find_break_idx(current);
}

fn push_wrapped_line(
    lines: &mut Vec<String>,
    current: &mut String,
    last_break: &mut Option<usize>,
    max_lines: usize,
) {
    trim_line_end(current);
    if !current.is_empty() && lines.len() < max_lines {
        lines.push(std::mem::take(current));
    } else {
        current.clear();
    }
    *last_break = None;
}

fn trim_line_end(current: &mut String) {
    while current.chars().last().map(|ch| ch.is_whitespace()).unwrap_or(false) {
        current.pop();
    }
}

fn find_break_idx(text: &str) -> Option<usize> {
    text.char_indices()
        .rev()
        .find(|(_, ch)| ch.is_whitespace())
        .map(|(idx, ch)| idx + ch.len_utf8())
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
        assert!(right.bubble_rect().x + right.bubble_rect().w <= SURFACE_W as f32);
        assert!(left.bubble_rect().x >= 0.0);
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
    fn output_panel_lives_inside_stretchable_torso() {
        let short = Layout { facing: Facing::Right, body_len: BODY_LEN_MIN };
        let tall = Layout { facing: Facing::Right, body_len: BODY_LEN_MAX };
        let torso = tall.torso_rect();
        let panel = tall.output_panel_rect();

        assert!(panel.x > torso.x);
        assert!(panel.y > torso.y);
        assert!(panel.x + panel.w < torso.x + torso.w);
        assert!(panel.y + panel.h < torso.y + torso.h);
        assert!(tall.output_panel_rect().h > short.output_panel_rect().h);
    }

    #[test]
    fn wrap_breaks_long_unspaced_tokens_to_fit_width() {
        let font = load_font().expect("system font available for wrap test");
        let lines = wrap(
            &font,
            "ws://127.0.0.1:17387/border-buddies",
            PANEL_TEXT_PX,
            68.0,
            8,
        );
        assert!(lines.len() > 1);
        assert!(lines.iter().all(|line| measure(&font, line, PANEL_TEXT_PX) <= 68.0));
    }

    #[test]
    fn bump_hugs_actual_surface_edge() {
        let right = bump_rect(BumpEdge::Right, 160, 120);
        assert_eq!(right.x + right.w, 160.0);
        assert!(point_in_bump(BumpEdge::Right, 160, 120, 159.0, right.y as f64 + 8.0));

        let bottom = bump_rect(BumpEdge::Bottom, 160, 90);
        assert_eq!(bottom.y + bottom.h, 90.0);
        assert!(point_in_bump(BumpEdge::Bottom, 160, 90, bottom.x as f64 + 8.0, 89.0));
    }

    #[test]
    fn bump_survives_tiny_transient_surface() {
        let rect = bump_rect(BumpEdge::Left, 40, 40);
        assert!(rect.x >= 0.0);
        assert!(rect.y >= 0.0);
        assert!(rect.x + rect.w <= 40.0);
        assert!(rect.y + rect.h <= 40.0);
        assert!(rect.w > 0.0);
        assert!(rect.h > 0.0);
        assert!(point_in_bump(BumpEdge::Left, 40, 40, 1.0, 20.0));
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
