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
pub const PINNED_SURFACE_W: u32 = 420;
pub const PINNED_SURFACE_H: u32 = 176;
pub const RECEIPT_RAIL_W: u32 = 160;
const FRAME_SIDE_PAD: f32 = 92.0;
const FRAME_TOP_PAD: f32 = 118.0;
const FRAME_BOTTOM_PAD: f32 = 72.0;
const FRAME_MIN_TARGET_W: f32 = 240.0;
const FRAME_MIN_TARGET_H: f32 = 160.0;
const FRAME_RAIL: f32 = 24.0;
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
pub const PINNED_BUBBLE_W_MIN: f32 = 188.0;
pub const PINNED_BUBBLE_W_MAX: f32 = 292.0;
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
const PERIMETER_SIZE: f32 = 20.0;
const PERIMETER_GAP: f32 = 6.0;
const SURFACE_BLOOM_W: f32 = 116.0;
const SURFACE_BLOOM_H: f32 = 24.0;
const SURFACE_BLOOM_R: f32 = 112.0;
const SURFACE_BLOOM_MAX_ITEMS: usize = 6;
const RECEIPT_RAIL_PAD: f32 = 8.0;
const RECEIPT_RAIL_CARD_H: f32 = 28.0;
const RECEIPT_RAIL_CARD_GAP: f32 = 5.0;

/// Default clay colour — Morph terracotta. Override per-buddy with `BB_COLOR`.
pub const CLAY_DEFAULT: [u8; 3] = [201, 109, 60];

/// Which side of the figure the UI (bubble + input) sits on. Computed by main.rs
/// from the buddy's screen position so the UI always faces the screen centre.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Facing {
    Left,
    Right,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PerimeterId {
    ArrowN,
    ArrowE,
    ArrowS,
    ArrowW,
    Quick0,
    Quick1,
    Quick2,
    Quick3,
    Add,
    Paste,
    Review,
    Edit,
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
        let pad_x = 7.0;
        let pad_y = 8.0;
        Rect {
            x: torso.x + pad_x,
            y: torso.y + pad_y,
            w: torso.w - pad_x * 2.0,
            h: (torso.h - pad_y * 2.0).max(0.0),
        }
    }

    pub fn perimeter_controls(&self) -> Vec<(PerimeterId, Rect)> {
        let torso = self.torso_rect();
        let size = PERIMETER_SIZE;
        let gap = PERIMETER_GAP;
        let center_x = torso.x + torso.w / 2.0 - size / 2.0;
        let center_y = torso.y + torso.h / 2.0 - size / 2.0;
        let top = torso.y - size - gap;
        let bottom = torso.y + torso.h + gap;
        let left = torso.x - size - gap;
        let right = torso.x + torso.w + gap;
        vec![
            (PerimeterId::ArrowN, Rect { x: center_x, y: top, w: size, h: size }),
            (PerimeterId::ArrowE, Rect { x: right, y: center_y, w: size, h: size }),
            (PerimeterId::ArrowS, Rect { x: center_x, y: bottom, w: size, h: size }),
            (PerimeterId::ArrowW, Rect { x: left, y: center_y, w: size, h: size }),
            (PerimeterId::Quick0, Rect { x: left, y: top, w: size, h: size }),
            (PerimeterId::Quick1, Rect { x: right, y: top, w: size, h: size }),
            (PerimeterId::Quick2, Rect { x: left, y: bottom, w: size, h: size }),
            (PerimeterId::Quick3, Rect { x: right, y: bottom, w: size, h: size }),
            (PerimeterId::Add, Rect { x: right + size + 5.0, y: bottom, w: size, h: size }),
            (PerimeterId::Paste, Rect { x: left, y: torso.y + 10.0, w: size, h: size }),
            (PerimeterId::Review, Rect { x: left, y: torso.y + 36.0, w: size, h: size }),
            (PerimeterId::Edit, Rect { x: left, y: torso.y + 62.0, w: size, h: size }),
        ]
    }

    pub fn perimeter_rect(&self, id: PerimeterId) -> Rect {
        self.perimeter_controls()
            .into_iter()
            .find_map(|(candidate, rect)| (candidate == id).then_some(rect))
            .unwrap_or_else(|| self.torso_rect())
    }

    pub fn surface_bloom_rects(&self, count: usize) -> Vec<Rect> {
        debug_assert!(count <= SURFACE_BLOOM_MAX_ITEMS, "surface bloom supports six visible slots");
        let torso = self.torso_rect();
        surface_bloom_rects_from_center(torso.x + torso.w / 2.0, torso.y + torso.h / 2.0, count.min(SURFACE_BLOOM_MAX_ITEMS))
    }

    pub fn torso_action_rect(&self, action: TorsoAction) -> Rect {
        let panel = self.output_panel_rect();
        let size = 18.0_f32.min(panel.w.max(0.0)).min((panel.h / 2.0).max(0.0));
        let x = panel.x + panel.w - size - 5.0;
        let y = match action {
            TorsoAction::Expand => panel.y + 5.0,
            TorsoAction::Copy => panel.y + (panel.h - size) / 2.0,
            TorsoAction::Scroll => panel.y + panel.h - size - 5.0,
        };
        Rect { x, y, w: size, h: size }
    }

    /// Small on-body governance control, shown while the chat input is open. Clicking it
    /// asks the soul to run the read-only `receipt_review` effector through the action
    /// gate; once the gate returns needs_confirmation the body flips it to a Confirm
    /// button. The body only requests and renders — the soul authorizes (law 7).
    pub fn review_button_rect(&self) -> Rect {
        self.perimeter_rect(PerimeterId::Review)
    }

    /// The on-body Edit / Confirm control — emits a typed `repo_edit` ActionIntent (an act
    /// effector). It sits one row ABOVE Review so the read-only and act governance controls
    /// never crowd the narrow column; like Review it flips to Confirm while the soul holds the
    /// action at needs_confirmation. The body builds + emits the intent; the soul authorizes (law 7).
    pub fn edit_button_rect(&self) -> Rect {
        self.perimeter_rect(PerimeterId::Edit)
    }

    pub fn paste_button_rect(&self) -> Rect {
        self.perimeter_rect(PerimeterId::Paste)
    }

}

#[derive(Clone, Copy)]
pub struct FrameTargetView {
    pub w: f32,
    pub h: f32,
}

#[derive(Clone, Copy)]
pub struct FrameLayout {
    pub target: Rect,
    pub surface_w: u32,
    pub surface_h: u32,
}

#[derive(Clone, Copy)]
pub struct PinnedLayout {
    pub bubble_w: f32,
}

impl PinnedLayout {
    pub fn new(bubble_w: f32) -> PinnedLayout {
        PinnedLayout { bubble_w: bubble_w.clamp(PINNED_BUBBLE_W_MIN, PINNED_BUBBLE_W_MAX) }
    }

    pub fn head_rect(&self) -> Rect {
        Rect { x: 10.0, y: 14.0, w: HEAD_R * 2.0, h: HEAD_R * 2.0 }
    }

    pub fn bubble_rect(&self) -> Rect {
        Rect { x: 106.0, y: 12.0, w: self.bubble_w, h: 62.0 }
    }

    pub fn input_rect(&self, lines: usize) -> Rect {
        let lines = lines.clamp(1, INPUT_MAX_LINES) as f32;
        Rect { x: 106.0, y: 88.0, w: self.bubble_w, h: 16.0 + lines * LINE_H }
    }

    pub fn input_region_rect(&self) -> Rect {
        self.input_rect(INPUT_MAX_LINES)
    }

    pub fn contains_head(&self, x: f64, y: f64) -> bool {
        self.head_rect().contains(x, y)
    }
}

impl FrameLayout {
    pub fn new(target: FrameTargetView) -> FrameLayout {
        let target_w = target.w.max(FRAME_MIN_TARGET_W);
        let target_h = target.h.max(FRAME_MIN_TARGET_H);
        let surface_w = (target_w + FRAME_SIDE_PAD * 2.0).ceil() as u32;
        let surface_h = (target_h + FRAME_TOP_PAD + FRAME_BOTTOM_PAD).ceil() as u32;
        FrameLayout {
            target: Rect {
                x: FRAME_SIDE_PAD,
                y: FRAME_TOP_PAD,
                w: target_w,
                h: target_h,
            },
            surface_w,
            surface_h,
        }
    }

    pub fn head_rect(&self) -> Rect {
        Rect {
            x: self.target.x - 50.0,
            y: self.target.y - 96.0,
            w: HEAD_R * 2.0,
            h: HEAD_R * 2.0,
        }
    }

    pub fn top_rail_rect(&self) -> Rect {
        Rect {
            x: self.target.x - FRAME_RAIL,
            y: self.target.y - FRAME_RAIL,
            w: self.target.w + FRAME_RAIL * 2.0,
            h: FRAME_RAIL,
        }
    }

    pub fn left_rail_rect(&self) -> Rect {
        Rect {
            x: self.target.x - FRAME_RAIL,
            y: self.target.y,
            w: FRAME_RAIL,
            h: self.target.h,
        }
    }

    pub fn right_rail_rect(&self) -> Rect {
        Rect {
            x: self.target.x + self.target.w,
            y: self.target.y,
            w: FRAME_RAIL,
            h: self.target.h,
        }
    }

    pub fn bottom_rail_rect(&self) -> Rect {
        Rect {
            x: self.target.x - FRAME_RAIL,
            y: self.target.y + self.target.h,
            w: self.target.w + FRAME_RAIL * 2.0,
            h: FRAME_RAIL + 6.0,
        }
    }

    pub fn visible_rects(&self) -> [Rect; 5] {
        [
            self.head_rect(),
            self.top_rail_rect(),
            self.left_rail_rect(),
            self.right_rail_rect(),
            self.bottom_rail_rect(),
        ]
    }

    pub fn contains_head(&self, px: f64, py: f64) -> bool {
        let head = self.head_rect();
        let cx = head.x + head.w / 2.0;
        let cy = head.y + head.h / 2.0;
        let dx = px as f32 - cx;
        let dy = py as f32 - cy;
        dx * dx + dy * dy <= HEAD_R * HEAD_R
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

pub fn torso_action_at(layout: &Layout, px: f64, py: f64) -> Option<TorsoAction> {
    [TorsoAction::Expand, TorsoAction::Copy, TorsoAction::Scroll]
        .into_iter()
        .find(|action| layout.torso_action_rect(*action).contains(px, py))
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

    /// The face a governance `action_result` decision should wear. The face is the fastest,
    /// most pre-rational channel a user has, so each decision must read DISTINCT at a glance —
    /// and HONESTLY: this is the trust membrane projecting its real state, never affect that
    /// outruns the outcome. `allow` smiles, `needs_confirmation` asks (the questioning Curious
    /// mouth), `blocked` (and any unknown — fail loud) holds the firm open-mouth Alert stop.
    /// The body only renders the decision it was handed; it never makes it (AGENTS.md law 7).
    pub fn for_decision(decision: &str) -> Emotion {
        match decision {
            "allow" => Emotion::Happy,
            "needs_confirmation" => Emotion::Curious,
            _ => Emotion::Alert,
        }
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
    /// Display name shown in the header (e.g. "Border Wizard") — distinct from the wire
    /// id ("hermes"). The body no longer title-cases the id for display.
    pub name: &'a str,
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

/// A decoded raster image ready to blit into the torso. Decode provider bytes with
/// [`decode_image_bytes`]; the body owns the Pixmap and lends it to the card.
pub type TorsoImage = Pixmap;

pub struct ImageCard<'a> {
    /// The decoded image to fit into the pane; `None` draws an empty frame (e.g. while
    /// bytes are still arriving or failed to decode).
    pub image: Option<&'a TorsoImage>,
}

/// The idle/status ledger that supersedes [`SessionCard`] — a fixed-row "passport" sized to
/// fit the 142px torso instead of the freeform six-field card that overflowed it. Boring on
/// purpose: persona + posture, a route chip, a divider, and a one-line output peek. No halo,
/// ring, or glass — those are later slices. `SessionCard` is retained as a rollback fallback.
pub struct PassportCard<'a> {
    /// Surface label from `surface_active.label` (e.g. "Private local chat").
    pub persona_label: &'a str,
    /// "work" | "play" | "private" — drives the posture tag colour.
    pub posture: &'a str,
    /// Provider label from `surface_active.route.label` / `providerLabel`, if any.
    pub provider: Option<&'a str>,
    /// "local" | "cloud" from `surface_active.route.locality`, if any — drives the locality dot.
    pub locality: Option<&'a str>,
    /// Optional soul-derived route health. Absent means no health chrome.
    pub route_health: Option<&'a str>,
    /// First line of the last output, shown as an idle peek (never replaces the full Text/Image cards).
    pub output_preview: Option<&'a str>,
}

#[derive(Clone, Copy)]
pub struct SurfaceDialItem<'a> {
    pub label: &'a str,
    pub availability: &'a str,
    pub active: bool,
}

#[derive(Clone, Copy)]
pub struct ReceiptRailItem<'a> {
    pub glyph: &'a str,
    pub effector: &'a str,
    pub decision: &'a str,
    pub route_label: Option<&'a str>,
    /// Grade basis (law 6). `graded == 0` means no grade backed this action (memory off /
    /// nothing retrieved) → no ⚖ marker, same as absent. `trusted` is the trusted count.
    pub graded: u32,
    pub trusted: u32,
    pub time: &'a str,
    /// The entry the user last clicked. Its detail opens in the speech bubble (off-rail),
    /// so the rail must show which entry is active — an accent ring anchors the click.
    pub selected: bool,
}

pub enum TorsoOutput<'a> {
    Session(SessionCard<'a>),
    Passport(PassportCard<'a>),
    Text(TextCard<'a>),
    Image(ImageCard<'a>),
    ImageStub(MediaStubCard<'a>),
    FileStub(MediaStubCard<'a>),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TorsoAction {
    Expand,
    Copy,
    Scroll,
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
    pub input_placeholder: &'a str,
    pub input_focused: bool,
    /// The on-body Review control is a Confirm button when the soul's last action_result for
    /// `receipt_review` asked for confirmation. The body only renders this state; never authorizes.
    pub review_pending: bool,
    /// The Edit control is a Confirm button when the soul's last action_result for `repo_edit`
    /// asked for confirmation. Kept distinct from `review_pending` so each act's confirm is its own.
    pub edit_pending: bool,
    pub posture_badge: Option<&'a str>,
    /// Per-quick-button dim flag (Quick0..3): true when that quick surface is `unwired`, so the
    /// button renders faded. Availability is soul-pushed (Slice 2a) — the body never derives it.
    pub dim_quick: [bool; 4],
    /// Hold-to-bloom surface dial items, ordered active-at-12 by the body state machine.
    pub surface_bloom: &'a [SurfaceDialItem<'a>],
    /// Soul-derived route health from `surface_active.route.health`; absent means no ring.
    pub route_health: Option<&'a str>,
    /// Body-observed local→cloud transition flash. Separate from route health.
    pub route_flash: bool,
    /// Expanded-mode receipt rail items, newest first. Empty still draws the rail panel.
    pub receipt_rail: &'a [ReceiptRailItem<'a>],
    pub layout: Layout,
    pub pinned: Option<PinnedLayout>,
    pub frame: Option<FrameLayout>,
    /// Clay colour (BB_COLOR) — every shade on the figure derives from this.
    pub color: [u8; 3],
}

pub fn receipt_rail_visible_for_body_len(body_len: f32) -> bool {
    body_len >= BODY_LEN_MAX
}

pub fn receipt_rail_card_index(x: f64, y: f64, count: usize) -> Option<usize> {
    if x < 0.0 || x > RECEIPT_RAIL_W as f64 {
        return None;
    }
    (0..count).find(|idx| receipt_rail_card_rect(*idx).contains(x, y))
}

fn receipt_rail_card_rect(idx: usize) -> Rect {
    Rect {
        x: RECEIPT_RAIL_PAD,
        y: RECEIPT_RAIL_PAD + idx as f32 * (RECEIPT_RAIL_CARD_H + RECEIPT_RAIL_CARD_GAP),
        w: RECEIPT_RAIL_W as f32 - RECEIPT_RAIL_PAD * 2.0,
        h: RECEIPT_RAIL_CARD_H,
    }
}

pub struct Sprite {
    font: Option<Font>,
}

impl Sprite {
    pub fn new() -> Self {
        Sprite {
            font: load_font(),
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

        if let Some(pinned) = view.pinned {
            draw_pinned_view(
                &mut pixmap,
                pinned,
                view.color,
                bob,
                eye_open,
                face.pupil_dy,
                &face.mouth,
            );
            if let Some(font) = &self.font {
                if let Some(text) = view.speech {
                    draw_pinned_bubble(&mut pixmap, font, pinned, text);
                }
                if view.chat_open {
                    draw_pinned_input(
                        &mut pixmap,
                        font,
                        pinned,
                        view.input_text,
                        view.input_placeholder,
                        view.input_focused,
                        view.t,
                    );
                }
            }
            blit_premultiplied_bgra(pixmap.data(), canvas);
            return;
        }

        if let Some(frame) = view.frame {
            draw_frame_view(&mut pixmap, frame, view.color, bob, eye_open, face.pupil_dy, &face.mouth);
            if let Some(text) = view.speech {
                if let Some(font) = &self.font {
                    draw_frame_label(&mut pixmap, font, frame, text);
                }
            }
            blit_premultiplied_bgra(pixmap.data(), canvas);
            return;
        }

        let rail_visible = receipt_rail_visible_for_body_len(view.layout.body_len) && view.pinned.is_none() && view.tucked.is_none();
        if rail_visible {
            if let Some(mut body) = Pixmap::new(SURFACE_W, h) {
                draw_body_content(&mut body, self.font.as_ref(), view, bob, eye_open, face.pupil_dy, &face.mouth, &pose);
                if let Some(font) = &self.font {
                    draw_receipt_rail(&mut pixmap, font, view.receipt_rail);
                }
                let paint = PixmapPaint::default();
                pixmap.draw_pixmap(
                    0,
                    0,
                    body.as_ref(),
                    &paint,
                    Transform::from_translate(RECEIPT_RAIL_W as f32, 0.0),
                    None,
                );
            }
        } else {
            draw_body_content(&mut pixmap, self.font.as_ref(), view, bob, eye_open, face.pupil_dy, &face.mouth, &pose);
        }

        blit_premultiplied_bgra(pixmap.data(), canvas);
    }
}

fn draw_body_content(
    pixmap: &mut Pixmap,
    font: Option<&Font>,
    view: &BodyView,
    bob: f32,
    eye_open: f32,
    pupil_dy: f32,
    mouth: &Mouth,
    pose: &FigurePose,
) {
    draw_figure(pixmap, &view.layout, view.color, bob, pose, view.route_health, view.route_flash);
    if let Some(font) = font {
        draw_torso_output(pixmap, font, &view.layout, &view.torso_output);
        if let Some(label) = view.posture_badge {
            draw_posture_badge(pixmap, font, &view.layout, label);
        }
    }
    draw_eyes(pixmap, bob, eye_open, pupil_dy);
    draw_mouth(pixmap, bob, mouth);
    if let Some(font) = font {
        draw_perimeter_controls(pixmap, font, &view.layout, view.dim_quick);
        draw_surface_bloom(pixmap, font, &view.layout, view.surface_bloom);
    }

    if let Some(text) = view.speech {
        if let Some(font) = font {
            draw_bubble(pixmap, font, &view.layout, text);
        }
    }

    if view.chat_open {
        if let Some(font) = font {
            draw_input(
                pixmap,
                font,
                &view.layout,
                view.input_text,
                view.input_placeholder,
                view.input_focused,
                view.t,
            );
            draw_paste_button(pixmap, font, &view.layout);
            draw_governance_button(pixmap, font, view.layout.review_button_rect(), view.review_pending, "Review");
            draw_governance_button(pixmap, font, view.layout.edit_button_rect(), view.edit_pending, "Edit");
        }
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

fn draw_figure(
    pixmap: &mut Pixmap,
    layout: &Layout,
    color: [u8; 3],
    bob: f32,
    pose: &FigurePose,
    route_health: Option<&str>,
    route_flash: bool,
) {
    let hips_y = layout.hips_y();
    let limb = solid(rgb(shade(color, 0.94)));
    draw_route_boundary_chrome(pixmap, layout, route_health, route_flash);

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

fn draw_route_boundary_chrome(
    pixmap: &mut Pixmap,
    layout: &Layout,
    route_health: Option<&str>,
    route_flash: bool,
) {
    if let Some(color) = route_health.and_then(route_health_ring_color) {
        stroke_figure_boundary(pixmap, layout, color, 2.0, 4.0);
    }
    if route_flash {
        stroke_figure_boundary(pixmap, layout, Color::from_rgba8(238, 162, 55, 205), 3.0, 8.0);
    }
}

fn route_health_ring_color(health: &str) -> Option<Color> {
    route_health_ring_rgba(health).map(|[r, g, b, a]| Color::from_rgba8(r, g, b, a))
}

fn route_health_ring_rgba(health: &str) -> Option<[u8; 4]> {
    match health {
        "ready" => Some([52, 168, 96, 180]),
        "degraded" => Some([218, 147, 45, 205]),
        "unavailable" => Some([210, 63, 60, 215]),
        _ => None,
    }
}

fn stroke_figure_boundary(pixmap: &mut Pixmap, layout: &Layout, color: Color, width: f32, outset: f32) {
    let paint = solid(color);
    let mut stroke = Stroke::default();
    stroke.width = width;
    stroke.line_cap = tiny_skia::LineCap::Round;
    stroke.line_join = tiny_skia::LineJoin::Round;

    if let Some(path) = ellipse_path(FIG_CX, HEAD_CY, HEAD_R + outset, HEAD_R + outset) {
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
    let torso = inset_rect(layout.torso_rect(), -outset, -outset);
    if let Some(path) = round_rect_path(torso, TORSO_R + outset) {
        pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
    }
}

fn draw_frame_view(
    pixmap: &mut Pixmap,
    frame: FrameLayout,
    color: [u8; 3],
    bob: f32,
    eye_open: f32,
    pupil_dy: f32,
    mouth: &Mouth,
) {
    let target = frame.target;
    let rail = rgb(color);
    let rail_shadow = solid(Color::from_rgba8(0, 0, 0, 58));
    let rail_highlight = solid(Color::from_rgba8(255, 255, 255, 36));

    for rect in [
        frame.top_rail_rect(),
        frame.left_rail_rect(),
        frame.right_rail_rect(),
        frame.bottom_rail_rect(),
    ] {
        draw_round_rect(pixmap, rect, rail);
        if let Some(path) = round_rect_path(rect, 13.0) {
            let mut stroke = Stroke::default();
            stroke.width = 2.0;
            pixmap.stroke_path(&path, &rail_shadow, &stroke, Transform::identity(), None);
        }
    }

    let mut highlight = Stroke::default();
    highlight.width = 5.0;
    highlight.line_cap = tiny_skia::LineCap::Round;
    for (x1, y1, x2, y2) in [
        (target.x - 6.0, target.y - FRAME_RAIL + 7.0, target.x + target.w * 0.72, target.y - FRAME_RAIL + 7.0),
        (target.x - FRAME_RAIL + 7.0, target.y + 20.0, target.x - FRAME_RAIL + 7.0, target.y + target.h * 0.70),
    ] {
        let mut pb = PathBuilder::new();
        pb.move_to(x1, y1);
        pb.line_to(x2, y2);
        if let Some(path) = pb.finish() {
            pixmap.stroke_path(&path, &rail_highlight, &highlight, Transform::identity(), None);
        }
    }

    // Gentle corner blobs make the rails read as one pliable body, not four boxes.
    for (cx, cy) in [
        (target.x - FRAME_RAIL / 2.0, target.y - FRAME_RAIL / 2.0),
        (target.x + target.w + FRAME_RAIL / 2.0, target.y - FRAME_RAIL / 2.0),
        (target.x - FRAME_RAIL / 2.0, target.y + target.h + FRAME_RAIL / 2.0),
        (target.x + target.w + FRAME_RAIL / 2.0, target.y + target.h + FRAME_RAIL / 2.0),
    ] {
        if let Some(blob) = PathBuilder::from_circle(cx, cy, FRAME_RAIL * 0.72) {
            pixmap.fill_path(&blob, &solid(rail), FillRule::Winding, Transform::identity(), None);
        }
    }

    let head = frame.head_rect();
    let hx = head.x + head.w / 2.0;
    let hy = head.y + head.h / 2.0 + bob;
    let mut head_paint = Paint::default();
    head_paint.anti_alias = true;
    head_paint.shader = tiny_skia::LinearGradient::new(
        tiny_skia::Point::from_xy(hx, hy - HEAD_R),
        tiny_skia::Point::from_xy(hx, hy + HEAD_R),
        vec![
            tiny_skia::GradientStop::new(0.0, rgb(lighten(color, 0.22))),
            tiny_skia::GradientStop::new(0.55, rgb(color)),
            tiny_skia::GradientStop::new(1.0, rgb(shade(color, 0.72))),
        ],
        tiny_skia::SpreadMode::Pad,
        Transform::identity(),
    )
    .unwrap_or_else(|| Shader::SolidColor(rgb(color)));
    if let Some(circle) = PathBuilder::from_circle(hx, hy, HEAD_R) {
        pixmap.fill_path(&circle, &head_paint, FillRule::Winding, Transform::identity(), None);
    }

    draw_frame_face(pixmap, hx, hy, eye_open, pupil_dy, mouth);
    draw_frame_hands_and_feet(pixmap, frame, color);

    // Tiny smudges on the rails so the stretched frame still feels handmade.
    let smudge = solid(Color::from_rgba8(255, 255, 255, 24));
    for i in 0..8 {
        let x = target.x + 28.0 + i as f32 * (target.w / 8.5);
        if let Some(e) = ellipse_path(x, target.y + target.h + 12.0, 10.0, 3.0) {
            pixmap.fill_path(&e, &smudge, FillRule::Winding, Transform::identity(), None);
        }
    }
}

fn draw_pinned_view(
    pixmap: &mut Pixmap,
    pinned: PinnedLayout,
    color: [u8; 3],
    bob: f32,
    eye_open: f32,
    pupil_dy: f32,
    mouth: &Mouth,
) {
    let head = pinned.head_rect();
    let hx = head.x + head.w / 2.0;
    let hy = head.y + head.h / 2.0 + bob;
    draw_clay_head_at(pixmap, hx, hy, color);
    draw_frame_face(pixmap, hx, hy, eye_open, pupil_dy, mouth);

    let shine = solid(Color::from_rgba8(255, 255, 255, 34));
    if let Some(e) = ellipse_path(hx - 13.0, hy - 18.0, 15.0, 9.0) {
        pixmap.fill_path(&e, &shine, FillRule::Winding, Transform::identity(), None);
    }
}

fn draw_clay_head_at(pixmap: &mut Pixmap, cx: f32, cy: f32, color: [u8; 3]) {
    let mut head_paint = Paint::default();
    head_paint.anti_alias = true;
    head_paint.shader = tiny_skia::LinearGradient::new(
        tiny_skia::Point::from_xy(cx, cy - HEAD_R),
        tiny_skia::Point::from_xy(cx, cy + HEAD_R),
        vec![
            tiny_skia::GradientStop::new(0.0, rgb(lighten(color, 0.22))),
            tiny_skia::GradientStop::new(0.55, rgb(color)),
            tiny_skia::GradientStop::new(1.0, rgb(shade(color, 0.72))),
        ],
        tiny_skia::SpreadMode::Pad,
        Transform::identity(),
    )
    .unwrap_or_else(|| Shader::SolidColor(rgb(color)));
    if let Some(circle) = PathBuilder::from_circle(cx, cy, HEAD_R) {
        pixmap.fill_path(&circle, &head_paint, FillRule::Winding, Transform::identity(), None);
    }
}

fn draw_frame_face(
    pixmap: &mut Pixmap,
    cx: f32,
    cy: f32,
    eye_open: f32,
    pupil_dy: f32,
    mouth: &Mouth,
) {
    let white = solid(Color::from_rgba8(250, 250, 248, 255));
    let dark = solid(Color::from_rgba8(20, 18, 16, 255));
    for dx in [-14.0_f32, 14.0] {
        if let Some(eye) = ellipse_path(cx + dx, cy - 8.0, 12.0, 12.0 * eye_open.max(0.08)) {
            pixmap.fill_path(&eye, &white, FillRule::Winding, Transform::identity(), None);
        }
        if let Some(pupil) = PathBuilder::from_circle(cx + dx + 2.0, cy - 8.0 + pupil_dy, 4.0) {
            pixmap.fill_path(&pupil, &dark, FillRule::Winding, Transform::identity(), None);
        }
    }

    let my = cy + 18.0;
    match mouth {
        Mouth::Smile(amount) => {
            let mut pb = PathBuilder::new();
            pb.move_to(cx - 15.0, my);
            pb.quad_to(cx, my + 16.0 * amount, cx + 15.0, my);
            if let Some(path) = pb.finish() {
                let mut stroke = Stroke::default();
                stroke.width = 4.0;
                stroke.line_cap = tiny_skia::LineCap::Round;
                pixmap.stroke_path(&path, &dark, &stroke, Transform::identity(), None);
            }
        }
        Mouth::Flat => {
            let mut pb = PathBuilder::new();
            pb.move_to(cx - 13.0, my);
            pb.line_to(cx + 13.0, my);
            if let Some(path) = pb.finish() {
                let mut stroke = Stroke::default();
                stroke.width = 4.0;
                stroke.line_cap = tiny_skia::LineCap::Round;
                pixmap.stroke_path(&path, &dark, &stroke, Transform::identity(), None);
            }
        }
        Mouth::Spec(spec) => draw_mouth_spec(pixmap, cx, my + 4.0, spec),
    }
}

fn draw_pinned_bubble(pixmap: &mut Pixmap, font: &Font, pinned: PinnedLayout, text: &str) {
    let rect = pinned.bubble_rect();
    draw_round_rect(pixmap, rect, Color::from_rgba8(247, 251, 255, 238));
    if let Some(path) = round_rect_path(rect, 14.0) {
        let mut stroke = Stroke::default();
        stroke.width = 1.0;
        pixmap.stroke_path(&path, &solid(Color::from_rgba8(0, 0, 0, 120)), &stroke, Transform::identity(), None);
    }
    let tail = {
        let mut pb = PathBuilder::new();
        pb.move_to(rect.x + 3.0, rect.y + 28.0);
        pb.line_to(rect.x - 18.0, rect.y + 36.0);
        pb.line_to(rect.x + 7.0, rect.y + 43.0);
        pb.close();
        pb.finish()
    };
    if let Some(path) = tail {
        pixmap.fill_path(&path, &solid(Color::from_rgba8(247, 251, 255, 238)), FillRule::Winding, Transform::identity(), None);
    }

    let lines = wrap(font, text, 14.0, rect.w - 20.0, 2);
    let mut y = rect.y + 20.0;
    for line in lines {
        draw_line(pixmap, font, &line, rect.x + 10.0, y, 14.0, [16, 24, 44]);
        y += 17.0;
    }
}

fn draw_pinned_input(
    pixmap: &mut Pixmap,
    font: &Font,
    pinned: PinnedLayout,
    text: &str,
    placeholder_text: &str,
    focused: bool,
    t: f32,
) {
    let all = wrap(font, text, TEXT_PX, pinned.input_region_rect().w - 22.0, usize::MAX);
    let start = all.len().saturating_sub(INPUT_MAX_LINES);
    let shown: &[String] = &all[start..];
    let rect = pinned.input_rect(shown.len().max(1));
    draw_round_rect(pixmap, rect, Color::from_rgba8(255, 255, 255, 236));
    if let Some(path) = round_rect_path(rect, 13.0) {
        let mut stroke = Stroke::default();
        stroke.width = if focused { 2.0 } else { 1.0 };
        let c = if focused {
            Color::from_rgba8(56, 188, 214, 230)
        } else {
            Color::from_rgba8(0, 0, 0, 100)
        };
        pixmap.stroke_path(&path, &solid(c), &stroke, Transform::identity(), None);
    }

    let placeholder;
    let wrapped: &[String] = if text.is_empty() {
        placeholder = vec![placeholder_text.to_string()];
        &placeholder
    } else {
        shown
    };
    let color = if text.is_empty() { [92, 98, 112] } else { [18, 28, 46] };
    let mut y = rect.y + 22.0;
    for line in wrapped.iter() {
        draw_line(pixmap, font, line, rect.x + 12.0, y, TEXT_PX, color);
        y += LINE_H;
    }
    if focused && (t * 2.0).fract() < 0.55 {
        let last = wrapped.last().map(String::as_str).unwrap_or("");
        let caret_x = (rect.x + 12.0 + measure(font, last, TEXT_PX)).min(rect.x + rect.w - 13.0);
        let caret_y = rect.y + 10.0 + (wrapped.len().saturating_sub(1) as f32 * LINE_H);
        let mut pb = PathBuilder::new();
        pb.move_to(caret_x, caret_y);
        pb.line_to(caret_x, caret_y + LINE_H);
        if let Some(path) = pb.finish() {
            let mut stroke = Stroke::default();
            stroke.width = 2.0;
            pixmap.stroke_path(&path, &solid(Color::from_rgba8(18, 28, 46, 230)), &stroke, Transform::identity(), None);
        }
    }
}

/// An on-body governance control (Review / Edit). Plain text labels — the loaded font has no
/// guarantee of glyph icons. While the soul holds the action at needs_confirmation the control
/// reads "Confirm", tinted active. The body only renders this state; it never authorizes (law 7).
fn draw_governance_button(pixmap: &mut Pixmap, font: &Font, rect: Rect, pending: bool, idle_label: &str) {
    let (bg, fg, label) = if pending {
        (Color::from_rgba8(56, 188, 214, 240), [8, 30, 38], "C")
    } else {
        let short = match idle_label {
            "Review" => "R",
            "Edit" => "E",
            _ => "?",
        };
        (Color::from_rgba8(247, 251, 255, 238), [24, 40, 64], short)
    };
    draw_round_rect(pixmap, rect, bg);
    if let Some(path) = round_rect_path(rect, 9.0) {
        let mut stroke = Stroke::default();
        stroke.width = 1.0;
        pixmap.stroke_path(&path, &solid(Color::from_rgba8(0, 0, 0, 110)), &stroke, Transform::identity(), None);
    }
    let tw = measure(font, label, PANEL_TEXT_PX);
    let x = rect.x + (rect.w - tw) / 2.0;
    let y = rect.y + 13.0;
    draw_line(pixmap, font, label, x, y, PANEL_TEXT_PX, fg);
}

fn draw_paste_button(pixmap: &mut Pixmap, font: &Font, layout: &Layout) {
    let rect = layout.paste_button_rect();
    let bg = Color::from_rgba8(247, 251, 255, 238);
    let fg = [24, 40, 64];
    let label = "P";
    draw_round_rect(pixmap, rect, bg);
    if let Some(path) = round_rect_path(rect, 9.0) {
        let mut stroke = Stroke::default();
        stroke.width = 1.0;
        pixmap.stroke_path(&path, &solid(Color::from_rgba8(0, 0, 0, 110)), &stroke, Transform::identity(), None);
    }
    let tw = measure(font, label, PANEL_TEXT_PX);
    let x = rect.x + (rect.w - tw) / 2.0;
    let y = rect.y + 13.0;
    draw_line(pixmap, font, label, x, y, PANEL_TEXT_PX, fg);
}

fn perimeter_label(id: PerimeterId) -> &'static str {
    match id {
        PerimeterId::ArrowN => "N",
        PerimeterId::ArrowE => "E",
        PerimeterId::ArrowS => "S",
        PerimeterId::ArrowW => "W",
        PerimeterId::Quick0 => "1",
        PerimeterId::Quick1 => "2",
        PerimeterId::Quick2 => "3",
        PerimeterId::Quick3 => "4",
        PerimeterId::Add => "+",
        PerimeterId::Paste | PerimeterId::Review | PerimeterId::Edit => "",
    }
}

/// Which quick-button slot (0..3) a perimeter id drives, if any — so the dim flag lines up
/// with the surface that quick button activates.
fn quick_slot(id: PerimeterId) -> Option<usize> {
    match id {
        PerimeterId::Quick0 => Some(0),
        PerimeterId::Quick1 => Some(1),
        PerimeterId::Quick2 => Some(2),
        PerimeterId::Quick3 => Some(3),
        _ => None,
    }
}

fn draw_perimeter_controls(pixmap: &mut Pixmap, font: &Font, layout: &Layout, dim_quick: [bool; 4]) {
    for (id, rect) in layout.perimeter_controls() {
        if matches!(id, PerimeterId::Paste | PerimeterId::Review | PerimeterId::Edit) {
            continue;
        }
        // A quick button for an `unwired` surface renders faded so the dim reads at a glance;
        // a tap still lands and the body answers "not wired yet" (Slice 2a).
        let dimmed = quick_slot(id).map(|slot| dim_quick[slot]).unwrap_or(false);
        let label = perimeter_label(id);
        let bg = if matches!(id, PerimeterId::Add) {
            Color::from_rgba8(56, 188, 214, 238)
        } else if dimmed {
            Color::from_rgba8(248, 250, 252, 96)
        } else {
            Color::from_rgba8(248, 250, 252, 232)
        };
        draw_round_rect(pixmap, rect, bg);
        if let Some(path) = round_rect_path(rect, 7.0) {
            let mut stroke = Stroke::default();
            stroke.width = 1.0;
            let edge = if dimmed { 48 } else { 100 };
            pixmap.stroke_path(&path, &solid(Color::from_rgba8(0, 0, 0, edge)), &stroke, Transform::identity(), None);
        }
        let tw = measure(font, label, PANEL_TEXT_PX);
        let text = if dimmed { [120, 130, 146] } else { [18, 28, 46] };
        draw_line(
            pixmap,
            font,
            label,
            rect.x + (rect.w - tw) / 2.0,
            rect.y + 13.0,
            PANEL_TEXT_PX,
            text,
        );
    }
}

fn surface_bloom_rects_from_center(cx: f32, cy: f32, count: usize) -> Vec<Rect> {
    const ANGLES_DEG: [f32; SURFACE_BLOOM_MAX_ITEMS] = [-90.0, -30.0, 30.0, 90.0, 150.0, 210.0];
    ANGLES_DEG
        .iter()
        .take(count.min(SURFACE_BLOOM_MAX_ITEMS))
        .map(|deg| {
            let rad = deg.to_radians();
            Rect {
                x: cx + rad.cos() * SURFACE_BLOOM_R - SURFACE_BLOOM_W / 2.0,
                y: cy + rad.sin() * SURFACE_BLOOM_R - SURFACE_BLOOM_H / 2.0,
                w: SURFACE_BLOOM_W,
                h: SURFACE_BLOOM_H,
            }
        })
        .collect()
}

pub fn surface_bloom_hit(layout: &Layout, count: usize, x: f64, y: f64) -> Option<usize> {
    layout
        .surface_bloom_rects(count)
        .into_iter()
        .enumerate()
        .find_map(|(idx, rect)| rect.contains(x, y).then_some(idx))
}

fn draw_surface_bloom(pixmap: &mut Pixmap, font: &Font, layout: &Layout, items: &[SurfaceDialItem]) {
    if items.is_empty() {
        return;
    }
    for (item, rect) in items.iter().zip(layout.surface_bloom_rects(items.len())) {
        let unwired = item.availability == "unwired";
        let bg = if item.active {
            Color::from_rgba8(28, 42, 58, 238)
        } else if unwired {
            Color::from_rgba8(248, 250, 252, 112)
        } else {
            Color::from_rgba8(248, 250, 252, 236)
        };
        draw_round_rect(pixmap, rect, bg);
        if let Some(path) = round_rect_path(rect, 8.0) {
            let mut stroke = Stroke::default();
            stroke.width = if item.active { 2.0 } else { 1.0 };
            let edge = if item.active { 190 } else if unwired { 54 } else { 118 };
            pixmap.stroke_path(&path, &solid(Color::from_rgba8(0, 0, 0, edge)), &stroke, Transform::identity(), None);
        }
        let label = fit_line(font, item.label, 9.5, rect.w - 14.0);
        let tw = measure(font, &label, 9.5);
        let fg = if item.active {
            [238, 246, 255]
        } else if unwired {
            [118, 126, 140]
        } else {
            [22, 30, 42]
        };
        draw_line(pixmap, font, &label, rect.x + (rect.w - tw) / 2.0, rect.y + 15.0, 9.5, fg);
    }
}

fn draw_posture_badge(pixmap: &mut Pixmap, font: &Font, layout: &Layout, label: &str) {
    let torso = layout.torso_rect();
    let badge = Rect { x: torso.x + 9.0, y: torso.y + 8.0, w: torso.w - 18.0, h: 16.0 };
    draw_round_rect(pixmap, badge, Color::from_rgba8(12, 31, 38, 220));
    let px = 8.0;
    let text_w = measure(font, label, px);
    draw_line(
        pixmap,
        font,
        label,
        badge.x + (badge.w - text_w) / 2.0,
        badge.y + 11.0,
        px,
        [210, 250, 245],
    );
}

fn draw_frame_hands_and_feet(pixmap: &mut Pixmap, frame: FrameLayout, color: [u8; 3]) {
    let target = frame.target;
    let limb = solid(rgb(shade(color, 0.94)));
    for (x, y) in [
        (target.x - FRAME_RAIL - 7.0, target.y + target.h * 0.38),
        (target.x + target.w + FRAME_RAIL + 7.0, target.y + target.h * 0.38),
    ] {
        if let Some(hand) = PathBuilder::from_circle(x, y, HAND_R + 2.0) {
            pixmap.fill_path(&hand, &limb, FillRule::Winding, Transform::identity(), None);
        }
    }
    for x in [target.x + target.w * 0.30, target.x + target.w * 0.70] {
        if let Some(foot) = ellipse_path(x, target.y + target.h + FRAME_RAIL + 13.0, 24.0, 8.0) {
            pixmap.fill_path(&foot, &limb, FillRule::Winding, Transform::identity(), None);
        }
    }
}

fn draw_frame_label(pixmap: &mut Pixmap, font: &Font, frame: FrameLayout, text: &str) {
    let max = frame.target.w.min(360.0).max(180.0);
    let rect = Rect {
        x: frame.target.x + 14.0,
        y: frame.target.y - 78.0,
        w: max,
        h: 48.0,
    };
    draw_round_rect(pixmap, rect, Color::from_rgba8(247, 251, 255, 238));
    if let Some(path) = round_rect_path(rect, 14.0) {
        let mut stroke = Stroke::default();
        stroke.width = 1.0;
        pixmap.stroke_path(&path, &solid(Color::from_rgba8(0, 0, 0, 130)), &stroke, Transform::identity(), None);
    }
    let lines = wrap(font, text, 14.0, rect.w - 20.0, 2);
    let mut y = rect.y + 19.0;
    for line in lines {
        draw_line(pixmap, font, &line, rect.x + 10.0, y, 14.0, [16, 24, 44]);
        y += 17.0;
    }
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
) {
    let rect = layout.output_panel_rect();
    if rect.w <= 0.0 || rect.h <= 0.0 {
        return;
    }

    let bg = Color::from_rgba8(246, 249, 250, 226);
    let rim = solid(Color::from_rgba8(0, 0, 0, 153));
    draw_round_rect(pixmap, rect, bg);
    if let Some(path) = round_rect_path(rect, 8.0) {
        let mut stroke = Stroke::default();
        stroke.width = 1.0;
        pixmap.stroke_path(&path, &rim, &stroke, Transform::identity(), None);
    }

    let content = inset_rect(rect, 5.0, 5.0);

    match output {
        TorsoOutput::Session(card) => draw_session_card(pixmap, font, content, card),
        TorsoOutput::Passport(card) => draw_passport_card(pixmap, font, content, card),
        TorsoOutput::Text(card) => draw_text_card(pixmap, font, content, card),
        TorsoOutput::Image(card) => draw_image_card(pixmap, content, card),
        TorsoOutput::ImageStub(card) => draw_media_stub(pixmap, font, content, card, true),
        TorsoOutput::FileStub(card) => draw_media_stub(pixmap, font, content, card, false),
    }

    draw_torso_action(pixmap, layout, TorsoAction::Expand);
    draw_torso_action(pixmap, layout, TorsoAction::Copy);
    draw_torso_action(pixmap, layout, TorsoAction::Scroll);
}

fn draw_session_card(pixmap: &mut Pixmap, font: &Font, rect: Rect, card: &SessionCard) {
    let pad = 8.0;
    let mut y = rect.y + pad + 12.0;
    let text_w = rect.w - pad * 2.0;
    let bottom = rect.y + rect.h - pad;

    let Some(next_y) = draw_wrapped_block_clipped(
        pixmap,
        font,
        rect.x + pad,
        y,
        PANEL_LABEL_PX,
        PANEL_LINE_H - 1.0,
        text_w,
        1,
        &format!("{} surface", card.name),
        [102, 88, 76],
        bottom,
    ) else {
        return;
    };
    y = next_y + 4.0;
    let Some(next_y) = draw_wrapped_block_clipped(
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
        bottom,
    ) else {
        return;
    };
    y = next_y + 5.0;

    for (line, max_lines) in [
        (format!("Provider: {}", card.provider), 1usize),
        (format!("Model: {}", card.model), 1usize),
        (format!("Link: {}", card.gateway), 2usize),
    ] {
        let Some(next_y) = draw_wrapped_block_clipped(
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
            bottom,
        ) else {
            break;
        };
        y = next_y + 2.0;
    }

    let note_y = y + 4.0;
    let note_h = (bottom - note_y).max(0.0);
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

/// Boring, fixed-row passport ledger sized for the 142px torso. Rows: persona + posture tag,
/// route chip (provider · locality dot), divider, one-line output peek. No halo/ring/glass —
/// the only job here is that the torso stops overflowing.
fn draw_passport_card(pixmap: &mut Pixmap, font: &Font, rect: Rect, card: &PassportCard) {
    let pad = 8.0;
    let x = rect.x + pad;
    let text_w = (rect.w - pad * 2.0).max(0.0);
    let bottom = rect.y + rect.h - pad;

    // Row 0 — posture tag (right), persona label (left, truncated up to the tag).
    let tag_label = posture_tag_label(card.posture);
    let tag_text_w = measure(font, tag_label, 8.0);
    let tag = Rect {
        x: rect.x + rect.w - pad - (tag_text_w + 10.0),
        y: rect.y + pad,
        w: tag_text_w + 10.0,
        h: 14.0,
    };
    let (tag_bg, tag_fg) = posture_tag_colors(card.posture);
    draw_round_rect(pixmap, tag, tag_bg);
    draw_line(pixmap, font, tag_label, tag.x + (tag.w - tag_text_w) / 2.0, tag.y + 10.0, 8.0, tag_fg);

    let row0_baseline = rect.y + pad + 11.0;
    let persona_w = (tag.x - 4.0 - x).max(0.0);
    let persona = fit_line(font, card.persona_label, 11.0, persona_w);
    draw_line(pixmap, font, &persona, x, row0_baseline, 11.0, [38, 34, 32]);

    // Row 1 — route chip: provider name, then a locality dot (green=local, blue=cloud).
    let row1_baseline = row0_baseline + 14.0;
    if card.route_health == Some("degraded") {
        fill_round_rect(
            pixmap,
            Rect { x: x - 3.0, y: row1_baseline - 11.0, w: text_w + 6.0, h: 14.0 },
            6.0,
            &solid(Color::from_rgba8(218, 147, 45, 42)),
        );
    }
    if let Some(provider) = card.provider {
        let prov = fit_line(font, provider, 10.0, (text_w - 12.0).max(0.0));
        draw_line(pixmap, font, &prov, x, row1_baseline, 10.0, [63, 56, 52]);
        if let Some(loc) = card.locality {
            let dot_x = x + measure(font, &prov, 10.0) + 7.0;
            let dot_y = row1_baseline - 3.0;
            if let Some(path) = ellipse_path(dot_x, dot_y, 3.0, 3.0) {
                pixmap.fill_path(
                    &path,
                    &solid(locality_dot_color(loc)),
                    FillRule::Winding,
                    Transform::identity(),
                    None,
                );
            }
        }
    }

    // Divider.
    let div_y = row1_baseline + 6.0;
    fill_round_rect(
        pixmap,
        Rect { x, y: div_y, w: text_w, h: 1.0 },
        0.5,
        &solid(Color::from_rgba8(0, 0, 0, 38)),
    );

    // Output area — one-line idle peek; never replaces the full Text/Image output cards.
    let body_top = div_y + 6.0;
    let avail_h = (bottom - body_top).max(0.0);
    if avail_h < PANEL_TEXT_PX {
        return;
    }
    let max_lines = (avail_h / PANEL_LINE_H).floor().max(1.0) as usize;
    let body = card
        .output_preview
        .unwrap_or("Idle — text, image, and file output land here.");
    let lines = wrap(font, body, PANEL_TEXT_PX, text_w, max_lines);
    let mut baseline = body_top + PANEL_TEXT_PX;
    for line in &lines {
        draw_line(pixmap, font, line, x, baseline, PANEL_TEXT_PX, [88, 74, 64]);
        baseline += PANEL_LINE_H;
    }
}

/// Compose the receipt card's second (detail) line. When the action was backed by graded
/// memory, the grade marker "⚖ M/N trusted" replaces the decision word (the decision is already
/// encoded in the glyph + its color, the cheapest-to-read real estate doing nothing new). The
/// route provenance is appended only if the combined line fits — the grade is the new
/// information and wins the budget, so a long provider label is dropped before the marker is
/// ever truncated (the trusted ratio must stay intact). With no grade, the line falls back to
/// the route label or the decision word, exactly as before.
fn receipt_detail_line(font: &Font, item: &ReceiptRailItem, max_w: f32) -> String {
    if item.graded == 0 {
        return item.route_label.unwrap_or(item.decision).to_string();
    }
    let marker = format!("\u{2696} {}/{} trusted", item.trusted, item.graded);
    match item.route_label {
        Some(route) => {
            let combined = format!("{marker} \u{00b7} {route}");
            if measure(font, &combined, 8.0) <= max_w {
                combined
            } else {
                marker
            }
        }
        None => marker,
    }
}

fn draw_receipt_rail(pixmap: &mut Pixmap, font: &Font, items: &[ReceiptRailItem]) {
    fill_round_rect(
        pixmap,
        Rect { x: 0.0, y: 0.0, w: RECEIPT_RAIL_W as f32, h: pixmap.height() as f32 },
        0.0,
        &solid(Color::from_rgba8(36, 42, 48, 220)),
    );
    fill_round_rect(
        pixmap,
        Rect { x: RECEIPT_RAIL_W as f32 - 1.0, y: 0.0, w: 1.0, h: pixmap.height() as f32 },
        0.0,
        &solid(Color::from_rgba8(255, 255, 255, 32)),
    );

    for (idx, item) in items.iter().enumerate() {
        let rect = receipt_rail_card_rect(idx);
        if rect.y + rect.h > pixmap.height() as f32 - RECEIPT_RAIL_PAD {
            break;
        }
        if item.selected {
            // Accent ring behind the card: a clicked entry expands its detail in the speech
            // bubble (off-rail), so the rail itself needs a visible anchor for the click.
            let ring = Rect { x: rect.x - 2.0, y: rect.y - 2.0, w: rect.w + 4.0, h: rect.h + 4.0 };
            fill_round_rect(pixmap, ring, 8.0, &solid(Color::from_rgba8(58, 122, 200, 255)));
        }
        // Selected cards are opaque so the accent reads as a clean border, not a tint.
        let card_alpha = if item.selected { 255 } else { 224 };
        fill_round_rect(pixmap, rect, 6.0, &solid(Color::from_rgba8(248, 250, 252, card_alpha)));
        let glyph_color = receipt_glyph_color(item.glyph);
        draw_line(pixmap, font, item.glyph, rect.x + 5.0, rect.y + 18.0, 11.0, glyph_color);

        let effector_x = rect.x + 22.0;
        let top = fit_line(font, item.effector, 8.5, 72.0);
        draw_line(pixmap, font, &top, effector_x, rect.y + 11.0, 8.5, [35, 39, 43]);

        let detail = receipt_detail_line(font, item, 72.0);
        let detail = fit_line(font, &detail, 8.0, 72.0);
        draw_line(pixmap, font, &detail, effector_x, rect.y + 22.0, 8.0, [83, 91, 99]);

        draw_line(pixmap, font, item.time, rect.x + rect.w - 43.0, rect.y + 18.0, 8.0, [83, 91, 99]);
    }
}

fn receipt_glyph_color(glyph: &str) -> [u8; 3] {
    match glyph {
        "✅" => [33, 122, 76],
        "☑" | "⏳" => [166, 111, 28],
        _ => [170, 48, 45],
    }
}

fn posture_tag_label(posture: &str) -> &'static str {
    match posture {
        "private" => "PRIV",
        "play" => "PLAY",
        _ => "WORK",
    }
}

/// Posture tag colours: private = indigo, play = amber, work (default) = steel.
fn posture_tag_colors(posture: &str) -> (Color, [u8; 3]) {
    match posture {
        "private" => (Color::from_rgba8(63, 60, 140, 230), [232, 232, 255]),
        "play" => (Color::from_rgba8(204, 142, 36, 230), [40, 28, 8]),
        _ => (Color::from_rgba8(70, 92, 110, 230), [232, 244, 252]),
    }
}

fn locality_dot_color(locality: &str) -> Color {
    match locality {
        "local" => Color::from_rgba8(58, 170, 96, 255), // green = on-device
        _ => Color::from_rgba8(58, 122, 200, 255),      // blue = cloud
    }
}

/// Truncate `text` to fit `max_w` at `px`, appending "…" when clipped. The single-line
/// truncation the session card lacked — which is exactly why six fields overflowed 142px.
fn fit_line(font: &Font, text: &str, px: f32, max_w: f32) -> String {
    if measure(font, text, px) <= max_w {
        return text.to_string();
    }
    let ell = "…";
    let ell_w = measure(font, ell, px);
    let mut out = String::new();
    let mut w = 0.0;
    for ch in text.chars() {
        let cw = font.metrics(ch, px).advance_width;
        if w + cw + ell_w > max_w {
            break;
        }
        out.push(ch);
        w += cw;
    }
    out.push_str(ell);
    out
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

fn draw_wrapped_block_clipped(
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
    bottom: f32,
) -> Option<f32> {
    if top + px > bottom {
        return None;
    }
    let fit_lines = (((bottom - top - px) / line_h).floor() as usize).saturating_add(1);
    let lines = max_lines.min(fit_lines);
    if lines == 0 {
        return None;
    }
    Some(draw_wrapped_block(pixmap, font, x, top, px, line_h, max_w, lines, text, color))
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

fn draw_image_card(pixmap: &mut Pixmap, rect: Rect, card: &ImageCard) {
    let frame = inset_rect(rect, 1.5, 1.5);
    draw_round_rect(pixmap, frame, Color::from_rgba8(255, 252, 248, 240));
    if let Some(path) = round_rect_path(frame, 8.0) {
        let mut stroke = Stroke::default();
        stroke.width = 1.0;
        pixmap.stroke_path(
            &path,
            &solid(Color::from_rgba8(0, 0, 0, 155)),
            &stroke,
            Transform::identity(),
            None,
        );
    }

    let image_rect = inset_rect(frame, 4.0, 4.0);
    draw_round_rect(pixmap, image_rect, Color::from_rgba8(236, 232, 228, 255));
    if let Some(image) = card.image {
        draw_fitted_image(pixmap, image, image_rect);
    }
    if let Some(path) = round_rect_path(image_rect, 8.0) {
        let mut stroke = Stroke::default();
        stroke.width = 1.0;
        pixmap.stroke_path(
            &path,
            &solid(Color::from_rgba8(0, 0, 0, 135)),
            &stroke,
            Transform::identity(),
            None,
        );
    }
}

fn draw_torso_action(pixmap: &mut Pixmap, layout: &Layout, action: TorsoAction) {
    let rect = layout.torso_action_rect(action);
    if rect.w <= 0.0 || rect.h <= 0.0 {
        return;
    }

    draw_round_rect(pixmap, rect, Color::from_rgba8(0, 0, 0, 140));
    if let Some(path) = round_rect_path(rect, rect.w.min(rect.h) / 2.0) {
        let mut stroke = Stroke::default();
        stroke.width = 1.0;
        pixmap.stroke_path(
            &path,
            &solid(Color::from_rgba8(255, 255, 255, 175)),
            &stroke,
            Transform::identity(),
            None,
        );
    }

    let icon = solid(Color::from_rgba8(255, 255, 255, 225));
    match action {
        TorsoAction::Expand => {
            let inset = 5.0;
            let mut pb = PathBuilder::new();
            pb.move_to(rect.x + inset, rect.y + rect.h * 0.52);
            pb.line_to(rect.x + inset, rect.y + inset);
            pb.line_to(rect.x + rect.w * 0.52, rect.y + inset);
            pb.move_to(rect.x + rect.w - inset, rect.y + rect.h * 0.48);
            pb.line_to(rect.x + rect.w - inset, rect.y + rect.h - inset);
            pb.line_to(rect.x + rect.w * 0.48, rect.y + rect.h - inset);
            if let Some(path) = pb.finish() {
                let mut stroke = Stroke::default();
                stroke.width = 1.35;
                stroke.line_cap = tiny_skia::LineCap::Round;
                stroke.line_join = tiny_skia::LineJoin::Round;
                pixmap.stroke_path(&path, &icon, &stroke, Transform::identity(), None);
            }
        }
        TorsoAction::Copy => {
            let back = Rect {
                x: rect.x + 5.0,
                y: rect.y + 4.0,
                w: rect.w - 9.0,
                h: rect.h - 8.0,
            };
            let front = Rect {
                x: rect.x + 3.0,
                y: rect.y + 6.0,
                w: rect.w - 9.0,
                h: rect.h - 8.0,
            };
            if let Some(path) = round_rect_path(back, 2.0) {
                let mut stroke = Stroke::default();
                stroke.width = 1.15;
                pixmap.stroke_path(&path, &icon, &stroke, Transform::identity(), None);
            }
            if let Some(path) = round_rect_path(front, 2.0) {
                let mut stroke = Stroke::default();
                stroke.width = 1.15;
                pixmap.stroke_path(&path, &icon, &stroke, Transform::identity(), None);
            }
        }
        TorsoAction::Scroll => {
            let cx = rect.x + rect.w / 2.0;
            let up_y = rect.y + 5.0;
            let down_y = rect.y + rect.h - 5.0;
            let mid = rect.y + rect.h / 2.0;

            let mut pb = PathBuilder::new();
            pb.move_to(cx - 4.0, up_y + 4.0);
            pb.line_to(cx, up_y);
            pb.line_to(cx + 4.0, up_y + 4.0);
            pb.move_to(cx, up_y + 1.0);
            pb.line_to(cx, down_y - 1.0);
            pb.move_to(cx - 4.0, down_y - 4.0);
            pb.line_to(cx, down_y);
            pb.line_to(cx + 4.0, down_y - 4.0);
            if let Some(path) = pb.finish() {
                let mut stroke = Stroke::default();
                stroke.width = 1.3;
                stroke.line_cap = tiny_skia::LineCap::Round;
                stroke.line_join = tiny_skia::LineJoin::Round;
                pixmap.stroke_path(&path, &icon, &stroke, Transform::identity(), None);
            }

            if let Some(dot) = PathBuilder::from_circle(cx, mid, 1.25) {
                pixmap.fill_path(&dot, &icon, FillRule::Winding, Transform::identity(), None);
            }
        }
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
    let border = solid(Color::from_rgba8(0, 0, 0, 175));
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
        let mut stroke = Stroke::default();
        stroke.width = 1.0;
        stroke.line_join = tiny_skia::LineJoin::Round;
        pixmap.stroke_path(&path, &border, &stroke, Transform::identity(), None);
    }
    if let Some(path) = round_rect_path(rect, 14.0) {
        let mut stroke = Stroke::default();
        stroke.width = 1.0;
        pixmap.stroke_path(&path, &border, &stroke, Transform::identity(), None);
    }

    let mut baseline = rect.y + pad_top;
    for line in &lines {
        draw_line(pixmap, font, line, rect.x + pad_x, baseline, TEXT_PX, [16, 24, 44]);
        baseline += LINE_H;
    }
}

/// The on-body chat input: expands to fit the typed text (up to
/// `INPUT_MAX_LINES`, then scrolls), brighter when focused, blinking caret.
fn draw_input(
    pixmap: &mut Pixmap,
    font: &Font,
    layout: &Layout,
    text: &str,
    placeholder_text: &str,
    focused: bool,
    t: f32,
) {
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
    let border = solid(Color::from_rgba8(0, 0, 0, 175));
    draw_round_rect(pixmap, rect, bg);
    if let Some(path) = round_rect_path(rect, 14.0) {
        let mut stroke = Stroke::default();
        stroke.width = 1.0;
        pixmap.stroke_path(&path, &border, &stroke, Transform::identity(), None);
    }

    let mut baseline = rect.y + 8.0 + TEXT_PX;
    if text.is_empty() {
        if !focused {
            draw_line(pixmap, font, placeholder_text, rect.x + pad, baseline, TEXT_PX, [130, 122, 114]);
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

fn inset_rect(rect: Rect, dx: f32, dy: f32) -> Rect {
    let w = (rect.w - dx * 2.0).max(0.0);
    let h = (rect.h - dy * 2.0).max(0.0);
    Rect { x: rect.x + dx, y: rect.y + dy, w, h }
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

pub fn title_case(text: &str) -> String {
    let mut chars = text.chars();
    let Some(first) = chars.next() else { return String::new() };
    let mut out = first.to_uppercase().to_string();
    out.push_str(chars.as_str());
    out
}

/// Decode encoded image bytes (PNG/JPEG) into a premultiplied-alpha Pixmap ready to
/// blit into the torso. Returns None on any decode failure — the body shows an empty
/// frame rather than crashing. Used for provider images delivered as inline bytes.
pub fn decode_image_bytes(bytes: &[u8]) -> Option<Pixmap> {
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
    fn decodes_encoded_image_into_pixmap() {
        // Any format the `image` features cover; the on-disk JPEG stands in for a
        // provider-delivered image. decode_image_bytes is the path inline `output`
        // bytes take before reaching the torso.
        let pixmap = decode_image_bytes(include_bytes!("../assets/eiffel-tower.jpg"))
            .expect("decodes a real jpeg");
        assert!(pixmap.width() > 0 && pixmap.height() > 0);
    }

    #[test]
    fn rejects_garbage_image_bytes() {
        assert!(decode_image_bytes(b"definitely not an image").is_none());
    }

    #[test]
    fn image_card_draws_a_decoded_image_without_panicking() {
        // Drives the real draw path with a decoded image fitted into the output pane.
        let image = decode_image_bytes(include_bytes!("../assets/eiffel-tower.jpg")).unwrap();
        let layout = Layout { facing: Facing::Right, body_len: BODY_LEN_MIN };
        let rect = layout.output_panel_rect();
        let mut pixmap = Pixmap::new(SURFACE_W, layout.surface_h()).unwrap();
        draw_image_card(&mut pixmap, rect, &ImageCard { image: Some(&image) });
        // And the empty-frame branch (bytes still arriving / undecodable) is safe too.
        draw_image_card(&mut pixmap, rect, &ImageCard { image: None });
    }

    #[test]
    fn surface_grows_with_body_stretch() {
        let short = Layout { facing: Facing::Right, body_len: BODY_LEN_MIN };
        let tall = Layout { facing: Facing::Right, body_len: BODY_LEN_MAX };
        assert!(tall.surface_h() > short.surface_h());
        // Even fully squashed, the UI column still fits.
        assert!(short.surface_h() >= UI_MIN_H as u32);
    }

    #[test]
    fn receipt_rail_is_expanded_mode_only() {
        assert!(!receipt_rail_visible_for_body_len(BODY_LEN_MAX - 0.1));
        assert!(receipt_rail_visible_for_body_len(BODY_LEN_MAX));
    }

    #[test]
    fn receipt_rail_hit_maps_cards_only_inside_rail() {
        assert_eq!(receipt_rail_card_index(12.0, 12.0, 2), Some(0));
        assert_eq!(receipt_rail_card_index(12.0, 45.0, 2), Some(1));
        assert_eq!(receipt_rail_card_index(RECEIPT_RAIL_W as f64 + 1.0, 12.0, 2), None);
        assert_eq!(receipt_rail_card_index(12.0, 90.0, 2), None);
    }

    #[test]
    fn receipt_detail_line_shows_grade_marker_and_drops_route_before_truncating_it() {
        let font = load_font().expect("system font available for receipt detail test");
        let with_grade = |graded, trusted, route| ReceiptRailItem {
            glyph: "✅",
            effector: "repo_edit",
            decision: "allow",
            route_label: route,
            graded,
            trusted,
            time: "11:00:01",
            selected: false,
        };

        // No grade → falls back to route label (or decision), no ⚖ marker.
        let none = receipt_detail_line(&font, &with_grade(0, 0, Some("claude")), 72.0);
        assert_eq!(none, "claude");
        assert!(!none.contains('\u{2696}'));

        // Grade + route, given the room → marker leads, route appended as provenance.
        let roomy = receipt_detail_line(&font, &with_grade(3, 2, Some("claude")), 200.0);
        assert_eq!(roomy, "\u{2696} 2/3 trusted \u{00b7} claude");

        // At the REAL 72px card budget the marker + route don't both fit, so the route is
        // sacrificed and the marker stays intact — grade is the new info, route is nice-to-have.
        let tight = receipt_detail_line(&font, &with_grade(3, 2, Some("claude")), 72.0);
        assert_eq!(tight, "\u{2696} 2/3 trusted", "at card width the marker wins; route drops, never truncates");

        // A long route never forces the marker (or its ratio) to truncate, at any budget.
        let long_route = "anthropic-claude-sonnet-4-5-20260101-preview";
        let dropped = receipt_detail_line(&font, &with_grade(3, 2, Some(long_route)), 200.0);
        assert_eq!(dropped, "\u{2696} 2/3 trusted", "marker must never be truncated; route drops first");
        assert!(!dropped.contains(long_route));

        // Grade, no route → marker alone.
        assert_eq!(receipt_detail_line(&font, &with_grade(1, 1, None), 72.0), "\u{2696} 1/1 trusted");
    }

    #[test]
    fn selected_receipt_card_draws_an_accent_ring_only_around_the_clicked_entry() {
        let font = load_font().expect("system font available for receipt rail test");
        let card = |selected| ReceiptRailItem {
            glyph: "✅",
            effector: "repo_edit",
            decision: "allow",
            route_label: Some("claude"),
            graded: 1,
            trusted: 1,
            time: "11:00:01",
            selected,
        };
        // Two cards: index 0 selected, index 1 not. The ring sits in the 2px margin to the
        // left of the card (rect.x = RECEIPT_RAIL_PAD = 8, ring from x=6), so sample there.
        let mut pixmap = Pixmap::new(RECEIPT_RAIL_W, 120).expect("pixmap");
        draw_receipt_rail(&mut pixmap, &font, &[card(true), card(false)]);

        let accent = |px: u32, py: u32| {
            let p = pixmap.pixel(px, py).expect("pixel in bounds");
            // Accent is (58,122,200) opaque — blue-dominant. The rail bg (36,42,48) is dark.
            p.blue() as i32 > p.red() as i32 + 40 && p.blue() > 140
        };
        // Card 0 spans y∈[8,36]; its ring border is at x≈7, y≈22.
        assert!(accent(7, 22), "selected card 0 must show the accent ring in its left margin");
        // Card 1 spans y∈[41,69]; same left-margin x but NO ring → rail background, not accent.
        assert!(!accent(7, 55), "unselected card 1 must not show an accent ring");
    }

    #[test]
    fn receipt_glyph_colors_keep_authorized_not_run_distinct_from_blocked() {
        assert_eq!(receipt_glyph_color("✅"), [33, 122, 76]);
        assert_eq!(receipt_glyph_color("☑"), [166, 111, 28]);
        assert_eq!(receipt_glyph_color("❌"), [170, 48, 45]);
        assert_ne!(receipt_glyph_color("☑"), receipt_glyph_color("❌"));
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
    fn governance_decisions_wear_distinct_honest_faces() {
        // Each decision must read DISTINCT at a glance — the whole point of the face channel.
        assert_eq!(Emotion::for_decision("allow"), Emotion::Happy);
        assert_eq!(Emotion::for_decision("needs_confirmation"), Emotion::Curious);
        assert_eq!(Emotion::for_decision("blocked"), Emotion::Alert);
        // And they are genuinely three different faces, not aliases.
        let (allow, ask, block) = (
            Emotion::for_decision("allow"),
            Emotion::for_decision("needs_confirmation"),
            Emotion::for_decision("blocked"),
        );
        assert_ne!(allow, ask);
        assert_ne!(ask, block);
        assert_ne!(allow, block);
        // Fail loud: an unknown decision must NOT smile — it holds the alert stop.
        assert_eq!(Emotion::for_decision("garbage"), Emotion::Alert);
    }

    #[test]
    fn perimeter_controls_surround_the_torso_and_own_chat_buttons() {
        for facing in [Facing::Left, Facing::Right] {
            let l = Layout { facing, body_len: BODY_LEN_MIN };
            let review = l.review_button_rect();
            let paste = l.paste_button_rect();
            let edit = l.edit_button_rect();
            let torso = l.torso_rect();
            let controls = l.perimeter_controls();
            assert_eq!(controls.len(), 12);
            for id in [PerimeterId::ArrowN, PerimeterId::ArrowE, PerimeterId::ArrowS, PerimeterId::ArrowW, PerimeterId::Add] {
                assert!(controls.iter().any(|(candidate, _)| *candidate == id), "missing {id:?}");
            }
            for btn in [review, paste, edit] {
                assert!(btn.x + btn.w <= torso.x, "chat button should sit on the left border ({facing:?})");
                assert!(btn.y >= torso.y - PERIMETER_SIZE - PERIMETER_GAP);
            }
            assert!(paste.y < review.y && review.y < edit.y, "chat buttons should stack on the border ({facing:?})");
        }
    }

    #[test]
    fn unwired_quick_button_renders_fainter_than_wired() {
        let layout = Layout::initial();
        let w = SURFACE_W;
        let h = layout.surface_h();
        let sprite = Sprite::new();

        let paint = |dim: [bool; 4]| {
            let mut canvas = vec![0_u8; (w * h * 4) as usize];
            let view = BodyView {
                t: 0.0,
                emotion: Emotion::Neutral,
                speech: None,
                torso_output: TorsoOutput::Session(SessionCard {
                    name: "B",
                    provider: "echo",
                    model: "m",
                    gateway: "g",
                    status: "s",
                    note: "n",
                }),
                chat_open: false,
                tucked: None,
                input_text: "",
                input_placeholder: "",
                input_focused: false,
                review_pending: false,
                edit_pending: false,
                posture_badge: None,
                dim_quick: dim,
                surface_bloom: &[],
                route_health: None,
                route_flash: false,
                receipt_rail: &[],
                layout,
                pinned: None,
                frame: None,
                color: CLAY_DEFAULT,
            };
            sprite.paint(&mut canvas, w, h, &view);
            canvas
        };

        let bright = paint([false; 4]);
        let dimmed = paint([true, false, false, false]);

        // Total alpha over the Quick0 button area: the dimmer fill paints lower coverage.
        let rect = layout.perimeter_rect(PerimeterId::Quick0);
        let alpha_sum = |canvas: &[u8]| -> u64 {
            let mut acc = 0_u64;
            for y in (rect.y as u32)..((rect.y + rect.h) as u32) {
                for x in (rect.x as u32)..((rect.x + rect.w) as u32) {
                    acc += canvas[((y * w + x) * 4 + 3) as usize] as u64;
                }
            }
            acc
        };
        assert!(
            alpha_sum(&dimmed) < alpha_sum(&bright),
            "an unwired Quick0 button should paint fainter than a wired one",
        );
    }

    #[test]
    fn route_health_ring_colors_follow_closed_set() {
        assert_eq!(route_health_ring_rgba("ready"), Some([52, 168, 96, 180]));
        assert_eq!(route_health_ring_rgba("degraded"), Some([218, 147, 45, 205]));
        assert_eq!(route_health_ring_rgba("unavailable"), Some([210, 63, 60, 215]));
        assert_eq!(route_health_ring_rgba("flaky"), None);
    }

    #[test]
    fn surface_bloom_slots_follow_clock_positions() {
        let layout = Layout::initial();
        let torso = layout.torso_rect();
        let center = (torso.x + torso.w / 2.0, torso.y + torso.h / 2.0);
        let rects = layout.surface_bloom_rects(6);

        assert_eq!(rects.len(), 6);
        let mid = |rect: Rect| (rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
        let slot0 = mid(rects[0]);
        let slot1 = mid(rects[1]);
        let slot3 = mid(rects[3]);
        let slot5 = mid(rects[5]);

        assert!((slot0.0 - center.0).abs() < 0.5, "active slot should be centered at 12");
        assert!(slot0.1 < center.1, "active slot should sit above the torso center");
        assert!(slot1.0 > center.0 && slot1.1 < center.1, "next slot should sit at 2 o'clock");
        assert!(slot5.0 < center.0 && slot5.1 < center.1, "previous slot should sit at 10 o'clock");
        assert!(slot3.1 > center.1, "opposite slot should sit at 6 o'clock");
    }

    #[test]
    fn surface_bloom_hit_maps_each_pill_to_its_index() {
        let layout = Layout::initial();
        let rects = layout.surface_bloom_rects(6);
        for (idx, rect) in rects.iter().enumerate() {
            let x = (rect.x + rect.w / 2.0) as f64;
            let y = (rect.y + rect.h / 2.0) as f64;
            assert_eq!(surface_bloom_hit(&layout, 6, x, y), Some(idx));
        }
        assert_eq!(surface_bloom_hit(&layout, 6, 1.0, 1.0), None);
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
    fn torso_actions_live_inside_output_panel() {
        let layout = Layout { facing: Facing::Right, body_len: BODY_LEN_DEFAULT };
        let panel = layout.output_panel_rect();
        for action in [TorsoAction::Expand, TorsoAction::Copy, TorsoAction::Scroll] {
            let rect = layout.torso_action_rect(action);
            assert!(rect.x >= panel.x);
            assert!(rect.y >= panel.y);
            assert!(rect.x + rect.w <= panel.x + panel.w);
            assert!(rect.y + rect.h <= panel.y + panel.h);
        }
    }

    #[test]
    fn fit_line_truncates_overflowing_text_with_an_ellipsis() {
        let font = load_font().expect("system font available for fit_line test");
        // Short text passes through untouched.
        assert_eq!(fit_line(&font, "LM Studio", 10.0, 200.0), "LM Studio");
        // Overflowing text is clipped to fit and marked with an ellipsis.
        let long = "Some Very Long Provider Gateway Label That Cannot Possibly Fit";
        let fitted = fit_line(&font, long, 10.0, 70.0);
        assert!(fitted.ends_with('…'), "truncated text must signal the clip");
        assert!(measure(&font, &fitted, 10.0) <= 70.0, "truncated text must fit the budget");
    }

    #[test]
    fn passport_rows_stay_within_the_142px_torso() {
        // The whole point of the passport: overflowing persona/provider/preview must NOT spill
        // past the torso column the way the old six-field SessionCard did.
        let font = load_font().expect("system font available for passport layout test");
        let layout = Layout { facing: Facing::Right, body_len: BODY_LEN_MIN };
        let panel = layout.output_panel_rect();
        // The drawing column itself is no wider than the 142px torso.
        assert!(panel.w <= TORSO_W, "output panel ({}) must fit TORSO_W ({TORSO_W})", panel.w);
        let content = inset_rect(panel, 5.0, 5.0);

        let mut pixmap = Pixmap::new(SURFACE_W, layout.surface_h()).unwrap();
        let card = PassportCard {
            persona_label: "Private Local Chat With An Absurdly Long Persona Name",
            posture: "private",
            provider: Some("Some Very Long Provider Gateway Label That Should Truncate"),
            locality: Some("local"),
            route_health: Some("degraded"),
            output_preview: Some(
                "A long idle preview line that should wrap and clip inside the panel, never spilling past the torso edge.",
            ),
        };
        draw_passport_card(&mut pixmap, &font, content, &card);

        // No drawn pixel may sit beyond the content's right edge (+1px AA tolerance).
        let right_limit = (content.x + content.w).ceil() as i32 + 1;
        let w = pixmap.width() as i32;
        let data = pixmap.data();
        let mut drew_something = false;
        for y in 0..pixmap.height() as i32 {
            for x in 0..w {
                let idx = ((y * w + x) * 4) as usize;
                let touched = data[idx] != 0 || data[idx + 1] != 0 || data[idx + 2] != 0 || data[idx + 3] != 0;
                if touched {
                    drew_something = true;
                    assert!(x <= right_limit, "passport pixel at x={x} exceeds torso column right edge {right_limit}");
                }
            }
        }
        assert!(drew_something, "passport should have drawn its rows");
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
    fn frame_layout_keeps_visible_regions_outside_target_hole() {
        let frame = FrameLayout::new(FrameTargetView { w: 1280.0, h: 720.0 });

        assert_eq!(frame.target.x, FRAME_SIDE_PAD);
        assert_eq!(frame.target.y, FRAME_TOP_PAD);
        assert!(frame.surface_w as f32 > frame.target.x + frame.target.w);
        assert!(frame.surface_h as f32 > frame.target.y + frame.target.h);

        let target_center = (
            frame.target.x as f64 + frame.target.w as f64 / 2.0,
            frame.target.y as f64 + frame.target.h as f64 / 2.0,
        );
        assert!(!frame.visible_rects().iter().any(|rect| rect.contains(target_center.0, target_center.1)));
        assert!(frame.contains_head(
            (frame.head_rect().x + frame.head_rect().w / 2.0) as f64,
            (frame.head_rect().y + frame.head_rect().h / 2.0) as f64,
        ));
    }

    #[test]
    fn frame_render_leaves_target_center_transparent() {
        let frame = FrameLayout::new(FrameTargetView { w: 640.0, h: 360.0 });
        let mut canvas = vec![0_u8; (frame.surface_w * frame.surface_h * 4) as usize];
        let sprite = Sprite::new();
        let view = BodyView {
            t: 0.0,
            emotion: Emotion::Happy,
            speech: Some("Framing Firefox."),
            torso_output: TorsoOutput::Session(SessionCard {
                name: "Border Wizard",
                provider: "echo",
                model: "not configured",
                gateway: "ws://127.0.0.1:17387/border-buddies",
                status: "Linked",
                note: "Idle",
            }),
            chat_open: false,
            tucked: None,
            input_text: "",
            input_placeholder: "Ask Border Wizard...",
            input_focused: false,
            review_pending: false,
            edit_pending: false,
            posture_badge: None,
            dim_quick: [false; 4],
            surface_bloom: &[],
            route_health: None,
            route_flash: false,
            receipt_rail: &[],
            layout: Layout::initial(),
            pinned: None,
            frame: Some(frame),
            color: CLAY_DEFAULT,
        };

        sprite.paint(&mut canvas, frame.surface_w, frame.surface_h, &view);

        let center_x = (frame.target.x + frame.target.w / 2.0) as u32;
        let center_y = (frame.target.y + frame.target.h / 2.0) as u32;
        let center_idx = ((center_y * frame.surface_w + center_x) * 4 + 3) as usize;
        assert_eq!(canvas[center_idx], 0, "target center must remain transparent");

        let rail_x = (frame.target.x + 12.0) as u32;
        let rail_y = (frame.target.y - FRAME_RAIL / 2.0) as u32;
        let rail_idx = ((rail_y * frame.surface_w + rail_x) * 4 + 3) as usize;
        assert!(canvas[rail_idx] > 0, "top rail should render opaque pixels");
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
