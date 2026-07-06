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

use crate::presence::AlertLevel;

// --- figure geometry -------------------------------------------------------------

pub const SURFACE_W: u32 = 560;
pub const PINNED_SURFACE_W: u32 = 420;
pub const PINNED_SURFACE_H: u32 = 176;
/// Legacy name — receipts now paint inside the torso panel at full stretch, not a side column.
const RECEIPT_LEDGER_ROW_H: f32 = 26.0;
const RECEIPT_LEDGER_ROW_GAP: f32 = 4.0;
const RECEIPT_LEDGER_PAD: f32 = 5.0;
const RECEIPT_LEDGER_ACTION_W: f32 = 26.0;
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

// --- ring skin geometry (BB_SKIN=ring) --------------------------------------------
// The standalone state halo. Its own geometry — a clean circle on the presence column —
// deliberately NOT derived from the figure's outline or the stretchable torso, so it holds
// its shape with the figure absent. Sits just above the (self-backed) pane content.
const RING_CX: f32 = FIG_CX;
const RING_CY: f32 = HEAD_CY - 2.0;
const RING_R: f32 = 40.0;
const RING_THICKNESS: f32 = 7.0;

// R4 — the tucked edge light bar. When the body tucks under Skin::Ring, a thin light bar
// flush to the tucked edge mirrors the ring hue, so the governance tier stays peripheral-
// readable with the figure gone. Thin enough to read as chrome, thick enough to read at a
// glance from the corner of the eye.
const BAR_THICKNESS: f32 = 10.0;
/// Fraction of the along-edge extent used as bar length (half-edge, centered on the tuck anchor).
pub const BAR_LENGTH_FRAC: f32 = 0.5;

/// Alpha for the bar body (always the buddy's instance color).
const BAR_BODY_ALPHA: u8 = 180;
/// Fraction of bar length used for each traffic-light tip (one at each end).
const BAR_TIP_FRAC: f32 = 0.2;

/// Shared eye pupil ink (reused from draw_eyes; no new color).
const EYE_INK: [u8; 4] = [28, 22, 18, 255]; // draw_eyes dark pupil

// --- UI geometry -----------------------------------------------------------------

pub const BUBBLE_W: f32 = 172.0;
pub const BUBBLE_W_DEFAULT: f32 = BUBBLE_W;
pub const BUBBLE_W_MIN: f32 = BUBBLE_W;
pub const BUBBLE_W_MAX: f32 = 420.0;
/// Horizontal space reclaimed from the retired side receipt rail — given to the speech column.
pub const EXPANDED_SURFACE_EXTRA: u32 = 160;
/// Drag strip on the bubble/reader edge away from the body (reorients with facing).
const BUBBLE_OUTER_RESIZE_W: f32 = 10.0;
pub const PINNED_BUBBLE_W_MIN: f32 = 188.0;
pub const PINNED_BUBBLE_W_MAX: f32 = 292.0;
const BUBBLE_Y: f32 = 8.0;
const INPUT_Y: f32 = 196.0;

/// Lines the speech bubble may grow to before the input region (single source for paint + hit).
fn bubble_line_budget() -> usize {
    ((INPUT_Y - BUBBLE_Y - 42.0) / LINE_H).floor() as usize
}

/// Lines the full-height reader may show on a surface of `surface_h` pixels.
pub fn reader_line_budget(surface_h: u32) -> usize {
    let pad_top = 30.0 + PANEL_LABEL_PX + 8.0;
    let pad_bottom = 12.0;
    ((surface_h as f32 - pad_top - pad_bottom) / LINE_H).floor().max(1.0) as usize
}

fn expand_glyph_rect(card: Rect) -> Rect {
    let size = 18.0;
    Rect {
        x: card.x + card.w - size - 6.0,
        y: card.y + 6.0,
        w: size,
        h: size,
    }
}

fn copy_glyph_beside(expand: Rect) -> Rect {
    let size = 18.0;
    let gap = 4.0;
    Rect {
        x: expand.x - size - gap,
        y: expand.y,
        w: size,
        h: size,
    }
}

const READER_PAD: f32 = 8.0;
/// Top strip — drag left/right to reposition the reader on-screen (like the head on the body).
pub const READER_MOVE_DRAG_H: f32 = 15.0;
/// Narrowest the reader may squash when pressed against a screen edge.
pub const READER_W_MIN: f32 = BUBBLE_W_MIN;

/// Full-surface reader card — the takeover spans the layer, not the speech-bubble column.
pub fn reader_card_rect(surface_w: f32, surface_h: u32) -> Rect {
    Rect {
        x: READER_PAD,
        y: READER_PAD,
        w: (surface_w - READER_PAD * 2.0).max(0.0),
        h: surface_h as f32 - READER_PAD * 2.0,
    }
}

/// Copy-all control on the reader card (single source for paint + hit).
pub fn reader_copy_rect(surface_w: f32, surface_h: u32) -> Rect {
    copy_glyph_beside(reader_collapse_rect(surface_w, surface_h))
}

/// Reader body text area (below title, above footer) for drag-select hit tests.
pub fn reader_text_rect(surface_w: f32, surface_h: u32) -> Rect {
    let card = reader_card_rect(surface_w, surface_h);
    let top = card.y + 30.0 + PANEL_LABEL_PX + 8.0;
    let bottom = card.y + card.h - 14.0;
    Rect {
        x: card.x + 14.0,
        y: top,
        w: card.w - 28.0,
        h: (bottom - top).max(0.0),
    }
}

/// Prefix-measure search: which character index sits at `x_offset` from the line start.
pub fn hit_char_index(font: &Font, line: &str, px: f32, x_offset: f32) -> usize {
    if x_offset <= 0.0 || line.is_empty() {
        return 0;
    }
    let char_count = line.chars().count();
    let mut best = 0;
    for i in 0..=char_count {
        let prefix: String = line.chars().take(i).collect();
        if measure(font, &prefix, px) <= x_offset {
            best = i;
        } else {
            break;
        }
    }
    best.min(char_count)
}

/// Collapse control on the reader card (single source for paint + hit).
pub fn reader_collapse_rect(surface_w: f32, surface_h: u32) -> Rect {
    expand_glyph_rect(reader_card_rect(surface_w, surface_h))
}

/// After a horizontal drag (or refit), slide the reader and squash its width against screen edges.
/// Right push: pin the right edge and compress as `left` moves. Left push: pin `left` at 0 and
/// compress `current_w`; dragging back restores toward `pref_w` whenever there is room.
pub fn reader_drag_layout(
    margin_left: f64,
    current_w: f64,
    pref_w: f64,
    dx: f64,
    sw: f64,
) -> (f64, f64) {
    let min_w = f64::from(READER_W_MIN);
    let pref_w = pref_w.max(min_w);
    let current_w = current_w.max(min_w);

    if !sw.is_finite() || sw > 1e9 {
        let proposed_left = margin_left + dx;
        if proposed_left < 0.0 {
            return (0.0, (current_w + proposed_left).max(min_w));
        }
        return (proposed_left.max(0.0), pref_w);
    }

    let proposed_left = margin_left + dx;

    // Push into the left wall — pin x=0 and eat the over-drag out of width.
    if proposed_left < 0.0 {
        let w = (current_w + proposed_left).max(min_w).min(sw);
        return (0.0, w);
    }

    let max_left = (sw - min_w).max(0.0);
    let left = proposed_left.min(max_left);
    let available = (sw - left).max(0.0);
    let w = if available < min_w {
        available
    } else {
        pref_w.min(available).max(min_w)
    };

    (left, w)
}

/// Top strip — horizontal drag repositions the full-height reader (margin_left only).
pub fn reader_move_drag_rect(surface_w: f32, surface_h: u32) -> Rect {
    let card = reader_card_rect(surface_w, surface_h);
    Rect {
        x: card.x,
        y: card.y,
        w: card.w,
        h: READER_MOVE_DRAG_H.min(card.h),
    }
}

/// Wheel notch → line delta (discrete = 3 lines per step; else absolute-derived, min magnitude 1).
pub fn scroll_delta_lines(discrete: Option<i32>, absolute: f64) -> i32 {
    if let Some(d) = discrete {
        if d != 0 {
            return d * 3;
        }
    }
    if absolute == 0.0 {
        return 0;
    }
    let from_abs = (absolute / f64::from(LINE_H)).round() as i32;
    let mag = from_abs.abs().max(1);
    if absolute > 0.0 {
        -mag
    } else {
        mag
    }
}

pub fn bubble_base_x(facing: Facing, bubble_w: f32) -> f32 {
    let figure_half = (TORSO_W / 2.0).max(HEAD_R);
    match facing {
        Facing::Right => FIG_CX + figure_half + UI_GAP,
        Facing::Left => FIG_CX - figure_half - UI_GAP - bubble_w,
    }
}

pub fn surface_w_for_body_len(body_len: f32) -> f32 {
    if receipt_ledger_visible_for_body_len(body_len) {
        SURFACE_W as f32 + EXPANDED_SURFACE_EXTRA as f32
    } else {
        SURFACE_W as f32
    }
}

pub fn bubble_max_w(facing: Facing, surface_w: f32) -> f32 {
    match facing {
        Facing::Right => {
            let base = bubble_base_x(facing, BUBBLE_W_MIN);
            (surface_w - 4.0 - base).max(BUBBLE_W_MIN)
        }
        Facing::Left => {
            let figure_half = (TORSO_W / 2.0).max(HEAD_R);
            (FIG_CX - figure_half - UI_GAP - 4.0).max(BUBBLE_W_MIN)
        }
    }
    .min(BUBBLE_W_MAX)
}

pub fn clamp_bubble_w(facing: Facing, w: f32, surface_w: f32) -> f32 {
    let max_w = bubble_max_w(facing, surface_w);
    w.clamp(BUBBLE_W_MIN, max_w)
}

/// Pointer delta along the outer resize strip, converted to a width change.
pub fn bubble_resize_delta_w(facing: Facing, dx: f32) -> f32 {
    match facing {
        Facing::Right => dx,
        Facing::Left => -dx,
    }
}

pub fn bubble_outer_resize_rect(card: Rect, facing: Facing) -> Rect {
    match facing {
        Facing::Right => Rect {
            x: card.x + card.w - BUBBLE_OUTER_RESIZE_W,
            y: card.y,
            w: BUBBLE_OUTER_RESIZE_W,
            h: card.h,
        },
        Facing::Left => Rect {
            x: card.x,
            y: card.y,
            w: BUBBLE_OUTER_RESIZE_W,
            h: card.h,
        },
    }
}

pub fn clamp_reader_scroll(scroll: usize, total: usize, budget: usize) -> usize {
    scroll.min(total.saturating_sub(budget))
}

fn reader_body_budget(surface_h: u32, total_lines: usize) -> usize {
    let line_budget = reader_line_budget(surface_h);
    if total_lines > line_budget {
        line_budget.saturating_sub(1).max(1)
    } else {
        line_budget
    }
}

pub fn reader_total_lines(font: &Font, text: &str, card_w: f32) -> usize {
    reader_wrapped_md_lines(font, text, card_w).len()
}

pub fn reader_scroll_apply(
    font: &Font,
    text: &str,
    card_w: f32,
    surface_h: u32,
    scroll: usize,
    delta: i32,
) -> usize {
    let total = reader_total_lines(font, text, card_w);
    let budget = reader_body_budget(surface_h, total);
    let max_scroll = total.saturating_sub(budget);
    ((scroll as i32) + delta).clamp(0, max_scroll as i32) as usize
}

pub fn reader_footer_text(scroll: usize, visible_count: usize, total: usize, copied: bool) -> String {
    if copied {
        return "Copied ✓".to_string();
    }
    if total <= visible_count {
        return String::new();
    }
    let a = scroll + 1;
    let b = (scroll + visible_count).min(total);
    format!("lines {a}–{b} of {total}")
}
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
/// Vertical gap between stacked pills within a column.
const SURFACE_BLOOM_GAP: f32 = 8.0;
/// Horizontal gap between the torso edge and a pill column — keeps the dial OUTSIDE the torso.
const SURFACE_BLOOM_SIDE_GAP: f32 = 12.0;
/// The dial blooms into two vertical columns flanking the torso, up to five pills a side.
const SURFACE_BLOOM_PER_SIDE: usize = 5;
const SURFACE_BLOOM_MAX_ITEMS: usize = SURFACE_BLOOM_PER_SIDE * 2;


/// Default clay colour — Morph terracotta. Override per-buddy with `BB_COLOR`.
pub const CLAY_DEFAULT: [u8; 3] = [201, 109, 60];

/// Which side of the figure the UI (bubble + input) sits on. Computed by main.rs
/// from the buddy's screen position so the UI always faces the screen centre.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Facing {
    Left,
    Right,
}

/// Which body skin renders. `Clay` (the default) is the daily surface: the frozen
/// anthropomorphic figure wearing the alert halo. `Ring` is the dev/test track: a standalone
/// state halo with the figure absent (docs/laminal-ring-pivot.md §Amendment 2026-07-02).
/// Selected once at startup from `BB_SKIN`. State and identity are orthogonal: the halo
/// speaks state (hue); the figure is identity, not a governance inference path.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Skin {
    #[default]
    Clay,
    Ring,
}

/// What renders when tucked — orthogonal to `Skin`. Default `Both`; parse refuses "neither"
/// (unset/garbage/`none` -> `Both`). Under `Skin::Ring` every mode coerces to `Bar` only.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum DockShow {
    Head,
    Bar,
    #[default]
    Both,
}

/// Ring skin has no head primitive — coerce every dock mode to bar-only so something always paints.
pub fn effective_dock_show(skin: Skin, dock: DockShow) -> DockShow {
    match skin {
        Skin::Ring => DockShow::Bar,
        Skin::Clay => dock,
    }
}

pub fn shows_tucked_head(dock: DockShow) -> bool {
    matches!(dock, DockShow::Head | DockShow::Both)
}

pub fn shows_tucked_bar(dock: DockShow) -> bool {
    matches!(dock, DockShow::Bar | DockShow::Both)
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
    pub bubble_w: f32,
}

impl Layout {
    pub fn new(facing: Facing, body_len: f32, bubble_w: f32) -> Layout {
        Layout { facing, body_len, bubble_w }
    }

    #[cfg(test)]
    pub fn initial() -> Layout {
        Layout::new(Facing::Right, BODY_LEN_DEFAULT, BUBBLE_W_DEFAULT)
    }

    /// Outer edge away from the body — drag to resize column width.
    pub fn bubble_outer_resize_rect(&self) -> Rect {
        bubble_outer_resize_rect(self.bubble_rect(), self.facing)
    }

    /// Receipt ledger list area inside the torso output panel (excludes action buttons).
    pub fn receipt_ledger_content_rect(&self) -> Rect {
        let panel = self.output_panel_rect();
        Rect {
            x: panel.x + RECEIPT_LEDGER_PAD,
            y: panel.y + RECEIPT_LEDGER_PAD,
            w: (panel.w - RECEIPT_LEDGER_PAD * 2.0 - RECEIPT_LEDGER_ACTION_W).max(0.0),
            h: (panel.h - RECEIPT_LEDGER_PAD * 2.0).max(0.0),
        }
    }

    /// One visible receipt row at `visible_idx` (0 = top of the scrolled window).
    pub fn receipt_ledger_row_rect(&self, visible_idx: usize) -> Rect {
        let content = self.receipt_ledger_content_rect();
        Rect {
            x: content.x,
            y: content.y + visible_idx as f32 * (RECEIPT_LEDGER_ROW_H + RECEIPT_LEDGER_ROW_GAP),
            w: content.w,
            h: RECEIPT_LEDGER_ROW_H,
        }
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
        bubble_base_x(self.facing, w)
    }

    /// Speech bubble at its maximum extent (drawing shrinks to the text; the
    /// input region uses this full rect).
    pub fn bubble_rect(&self) -> Rect {
        let h = 30.0 + bubble_line_budget() as f32 * LINE_H + 12.0;
        Rect { x: self.ui_x(self.bubble_w), y: BUBBLE_Y, w: self.bubble_w, h }
    }

    /// The expand affordance at the speech bubble's top-right (single source for paint + hit).
    pub fn bubble_expand_rect(&self) -> Rect {
        expand_glyph_rect(self.bubble_rect())
    }

    /// Copy-all on the speech bubble, beside the expand glyph (single source for paint + hit).
    pub fn bubble_copy_rect(&self) -> Rect {
        copy_glyph_beside(self.bubble_expand_rect())
    }

    /// Chat input box sized for `lines` lines of text.
    pub fn input_rect(&self, lines: usize) -> Rect {
        let lines = lines.clamp(1, INPUT_MAX_LINES) as f32;
        Rect { x: self.ui_x(self.bubble_w), y: INPUT_Y, w: self.bubble_w, h: 16.0 + lines * LINE_H }
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
        debug_assert!(count <= SURFACE_BLOOM_MAX_ITEMS, "surface bloom supports ten visible slots (five per side)");
        surface_bloom_rects_two_columns(self.torso_rect(), count.min(SURFACE_BLOOM_MAX_ITEMS))
    }

    /// The interior-view rows: the perimeter controls re-laid-out as a vertical list INSIDE
    /// the torso panel, top-to-bottom in display order. The caller picks which controls to
    /// fold in (surfaces always; Paste/Review/Edit only while chat is open) and passes the
    /// count via `rows.len()` — each row is sized to fill the panel height evenly. Returns the
    /// rows that fit (stretched bodies show all; a very short torso drops the last rows rather
    /// than overlap). Order is fixed so muscle memory transfers from the perimeter ring.
    pub fn interior_rows_for(&self, count: usize) -> Vec<Rect> {
        let panel = self.output_panel_rect();
        if count == 0 {
            return Vec::new();
        }
        let gap = 3.0;
        // Evenly size rows to fill the panel height; clamp so a short torso still shows whole rows.
        let row_h = ((panel.h - gap * (count as f32 - 1.0)) / count as f32).max(0.0).min(20.0);
        if row_h < 8.0 {
            // Too short to legibly fit even one row — show nothing rather than a cramped list.
            // 8px is the floor at which the glyph + label still read; the full chat-open set
            // (10 rows) fits a default 140px torso at ~9.7px per row.
            return Vec::new();
        }
        (0..count)
            .map(|i| {
                let y = panel.y + i as f32 * (row_h + gap);
                Rect { x: panel.x, y, w: panel.w, h: row_h }
            })
            .collect()
    }

    /// Kept for the closed-chat case (surfaces only, 7 rows). Callers that need the chat
    /// controls too should build their own list and call `interior_rows_for`.
    #[cfg(test)]
    pub fn interior_rows(&self) -> Vec<(PerimeterId, Rect)> {
        let ids = [
            PerimeterId::ArrowN,
            PerimeterId::Quick0,
            PerimeterId::Quick1,
            PerimeterId::Quick2,
            PerimeterId::Quick3,
            PerimeterId::Add,
            PerimeterId::ArrowS,
        ];
        self.interior_rows_for(ids.len())
            .into_iter()
            .zip(ids.iter())
            .map(|(rect, id)| (*id, rect))
            .collect()
    }

    /// Minimum torso stretch so a connect-style panel (many options + credential fields) fits
    /// without overlapping rows. Callers may bump `body_len` to this when a `panel` cue opens.
    pub fn min_body_len_for_onboarding(
        list_rows: usize,
        field_rows: usize,
        has_prompt: bool,
        has_primary: bool,
    ) -> f32 {
        let content = Self::min_onboarding_content_h(list_rows, field_rows, has_prompt, has_primary);
        // output_panel_rect: h = body_len - 16; content: h = panel.h - 8
        (content + 24.0).clamp(BODY_LEN_MIN, BODY_LEN_MAX)
    }

    fn min_onboarding_content_h(
        list_rows: usize,
        field_rows: usize,
        has_prompt: bool,
        has_primary: bool,
    ) -> f32 {
        let title = 15.0 + 2.0;
        let prompt = if has_prompt { 13.0 + 2.0 } else { 0.0 };
        let primary = if has_primary { 24.0 + 6.0 } else { 0.0 };
        let gap = 2.0;
        let section_gap = 4.0;
        let opt_h = 18.0;
        let field_h = 16.0;
        let list_h = if list_rows > 0 {
            list_rows as f32 * opt_h + (list_rows as f32 - 1.0).max(0.0) * gap + section_gap
        } else {
            0.0
        };
        let fields_h = if field_rows > 0 {
            field_rows as f32 * field_h + (field_rows as f32 - 1.0).max(0.0) * gap
        } else {
            0.0
        };
        title + prompt + list_h + fields_h + primary + 4.0
    }

    /// Interactive rects for the in-torso onboarding panel (Build C). Shares the torso output
    /// card with the settings/interior views; reserves the right-edge strip for torso actions.
    pub fn onboarding_layout(
        &self,
        list_rows: usize,
        field_rows: usize,
        has_prompt: bool,
        has_primary: bool,
    ) -> OnboardingLayout {
        let panel = self.output_panel_rect();
        let inset = 4.0;
        let right_reserve = 22.0;
        let content = Rect {
            x: panel.x + inset,
            y: panel.y + inset,
            w: (panel.w - inset * 2.0 - right_reserve).max(0.0),
            h: (panel.h - inset * 2.0).max(0.0),
        };
        if content.h < 24.0 {
            return OnboardingLayout {
                card: content,
                title: content,
                prompt: None,
                options: Vec::new(),
                fields: Vec::new(),
                primary: None,
            };
        }

        let title_h = 15.0_f32;
        let mut y = content.y;
        let title = Rect { x: content.x, y, w: content.w, h: title_h };
        y += title_h + 2.0;

        let prompt = if has_prompt {
            let h = 13.0_f32.min((content.y + content.h - y).max(0.0));
            let rect = Rect { x: content.x, y, w: content.w, h };
            y += h + 2.0;
            Some(rect)
        } else {
            None
        };

        let primary_h = 24.0_f32;
        let primary_gap = 6.0_f32;
        let primary = if has_primary {
            Some(Rect {
                x: content.x,
                y: content.y + content.h - primary_h,
                w: content.w,
                h: primary_h,
            })
        } else {
            None
        };

        let bottom = primary.map(|r| r.y - primary_gap).unwrap_or(content.y + content.h);
        let mid_h = (bottom - y - 2.0).max(0.0);
        let row_gap = 2.0;
        let section_gap = 4.0;
        let field_row_h = 16.0_f32;
        let fields_total = if field_rows > 0 {
            field_rows as f32 * field_row_h + (field_rows as f32 - 1.0).max(0.0) * row_gap + section_gap
        } else {
            0.0
        };
        let list_budget = (mid_h - fields_total).max(0.0);

        let opt_h_detailed = 18.0_f32;
        let opt_h_compact = 12.0_f32;
        let option_row_h = if list_rows == 0 {
            0.0
        } else if list_budget >= list_rows as f32 * opt_h_detailed + (list_rows as f32 - 1.0).max(0.0) * row_gap {
            opt_h_detailed
        } else if list_budget >= list_rows as f32 * opt_h_compact + (list_rows as f32 - 1.0).max(0.0) * row_gap {
            opt_h_compact
        } else {
            ((list_budget - (list_rows as f32 - 1.0).max(0.0) * row_gap) / list_rows as f32)
                .max(10.0)
                .min(opt_h_compact)
        };

        let option_rects = if list_rows > 0 && option_row_h >= 10.0 {
            (0..list_rows)
                .map(|i| Rect {
                    x: content.x,
                    y: y + i as f32 * (option_row_h + row_gap),
                    w: content.w,
                    h: option_row_h,
                })
                .collect()
        } else {
            Vec::new()
        };

        let fields_y = if list_rows > 0 {
            option_rects.last().map(|r| r.y + r.h + section_gap).unwrap_or(y)
        } else {
            y
        };
        let field_rects = if field_rows > 0 && (bottom - fields_y) >= field_row_h {
            (0..field_rows)
                .map(|i| Rect {
                    x: content.x,
                    y: fields_y + i as f32 * (field_row_h + row_gap),
                    w: content.w,
                    h: field_row_h,
                })
                .collect()
        } else {
            Vec::new()
        };

        let card_top = title.y - 4.0;
        let card_bottom = primary.map(|r| r.y + r.h + 4.0).unwrap_or(bottom);
        let card = Rect {
            x: content.x - 4.0,
            y: card_top,
            w: content.w + 8.0,
            h: (card_bottom - card_top).max(0.0),
        };

        OnboardingLayout {
            card,
            title,
            prompt,
            options: option_rects,
            fields: field_rects,
            primary,
        }
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

/// Along-edge coordinate shared by the tucked bump and the bar anchor (`bump_center` on the
/// free axis).
pub fn bump_along_edge(edge: BumpEdge, w: u32, h: u32) -> f32 {
    let (cx, cy) = bump_center(edge, w, h);
    match edge {
        BumpEdge::Left | BumpEdge::Right => cy,
        BumpEdge::Top | BumpEdge::Bottom => cx,
    }
}

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

/// Predicate for waking the tucked head's eyes: ONLY while an action is in flight (the
/// F4 activity wire). Soul tiers and route health never wake them — the predicate takes
/// the activity bool alone.
fn bump_eyes_awake(activity: bool) -> bool {
    activity
}

const GAZE_SWEEP_DX: f32 = 3.0;
const GAZE_PERIOD_S: f32 = 2.6;

/// Horizontal pupil sweep while an action is in flight. Takes the activity bool ONLY — a
/// soul-emitted Ready tier greens the chrome but never moves the gaze, and route health has
/// no parameter to sneak through (the F4 law, third application).
fn activity_gaze_dx(activity: bool, t: f32) -> f32 {
    if activity {
        (t * std::f32::consts::TAU / GAZE_PERIOD_S).sin() * GAZE_SWEEP_DX
    } else {
        0.0
    }
}

/// The white-eye centers for the awake tucked head. Mirrors the sleeping-face anchor
/// computed inside the frozen `draw_bump` (nudge + -2.0 y) then offsets ±BUMP_EYE_DX
/// horizontally (always screen-horizontal pair so the face reads correctly on every edge).
/// +1.5 y shifts the whites down a hair to sit nicely under the lid arcs.
fn bump_eye_centers(edge: BumpEdge, w: u32, h: u32) -> [(f32, f32); 2] {
    let (cx, cy) = bump_center(edge, w, h);
    let (dx, dy) = match edge {
        BumpEdge::Left => (BUMP_R * BUMP_FACE_NUDGE, 0.0),
        BumpEdge::Right => (-BUMP_R * BUMP_FACE_NUDGE, 0.0),
        BumpEdge::Top => (0.0, BUMP_R * BUMP_FACE_NUDGE),
        BumpEdge::Bottom => (0.0, -BUMP_R * BUMP_FACE_NUDGE),
    };
    let ax = cx + dx;
    let ay = cy + dy - 2.0;
    [(ax - BUMP_EYE_DX, ay + 1.5), (ax + BUMP_EYE_DX, ay + 1.5)]
}

/// Along-edge bar length: symmetric shrink near corners, capped at half the edge extent.
fn bar_along_length(extent: f32, along: f32, max_len: f32) -> f32 {
    (2.0 * along.min(extent - along))
        .min(extent * BAR_LENGTH_FRAC)
        .min(max_len)
}

/// Reference along-edge length from a left/right tuck at the head anchor — top/bottom bars
/// use this cap so every edge reads the same size.
fn tuck_bar_along_length(w: u32, h: u32) -> f32 {
    let hf = h as f32;
    let along = bump_along_edge(BumpEdge::Left, w, h);
    bar_along_length(hf, along, f32::INFINITY)
}

/// Bounding box of the tucked edge light bar — shared geometry for `draw_edge_bar` paint and
/// tuck hit-testing. Centered on `along`; length shrinks symmetrically near corners (never
/// slides). Top/bottom width matches the left/right bar height at the head anchor.
pub fn bar_rect(edge: BumpEdge, w: u32, h: u32, along: f32) -> Rect {
    let wf = w as f32;
    let hf = h as f32;
    let t = BAR_THICKNESS;
    let reference_len = tuck_bar_along_length(w, h);
    match edge {
        BumpEdge::Left => {
            let len = bar_along_length(hf, along, reference_len);
            let y = along - len / 2.0;
            Rect { x: 0.0, y, w: t, h: len }
        }
        BumpEdge::Right => {
            let len = bar_along_length(hf, along, reference_len);
            let y = along - len / 2.0;
            Rect { x: wf - t, y, w: t, h: len }
        }
        BumpEdge::Top => {
            let len = bar_along_length(wf, along, reference_len);
            let x = along - len / 2.0;
            Rect { x, y: 0.0, w: len, h: t }
        }
        BumpEdge::Bottom => {
            let len = bar_along_length(wf, along, reference_len);
            let x = along - len / 2.0;
            Rect { x, y: hf - t, w: len, h: t }
        }
    }
}

pub fn point_in_bar(edge: BumpEdge, w: u32, h: u32, along: f32, px: f64, py: f64) -> bool {
    bar_rect(edge, w, h, along).contains(px, py)
}

/// Union bounding box of every tucked summon primitive visible for this skin/dock pair.
pub fn tucked_summon_bounds(skin: Skin, dock: DockShow, edge: BumpEdge, w: u32, h: u32) -> Rect {
    let dock = effective_dock_show(skin, dock);
    let along = bump_along_edge(edge, w, h);
    let mut rects: Vec<Rect> = Vec::new();
    if shows_tucked_head(dock) {
        rects.push(bump_rect(edge, w, h));
    }
    if shows_tucked_bar(dock) {
        rects.push(bar_rect(edge, w, h, along));
    }
    rects
        .into_iter()
        .reduce(|a, b| Rect {
            x: a.x.min(b.x),
            y: a.y.min(b.y),
            w: (a.x + a.w).max(b.x + b.w) - a.x.min(b.x),
            h: (a.y + a.h).max(b.y + b.h) - a.y.min(b.y),
        })
        .unwrap_or(Rect { x: 0.0, y: 0.0, w: 0.0, h: 0.0 })
}

/// Hit-test union of painted tucked summon targets (head circle + bar rect).
pub fn point_in_tucked_summon(
    skin: Skin,
    dock: DockShow,
    edge: BumpEdge,
    w: u32,
    h: u32,
    px: f64,
    py: f64,
) -> bool {
    let dock = effective_dock_show(skin, dock);
    let along = bump_along_edge(edge, w, h);
    (shows_tucked_head(dock) && point_in_bump(edge, w, h, px, py))
        || (shows_tucked_bar(dock) && point_in_bar(edge, w, h, along, px, py))
}

/// Input-region rects for every visible tucked summon primitive (one rect per primitive).
pub fn tucked_summon_rects(skin: Skin, dock: DockShow, edge: BumpEdge, w: u32, h: u32) -> Vec<Rect> {
    let dock = effective_dock_show(skin, dock);
    let along = bump_along_edge(edge, w, h);
    let mut rects = Vec::new();
    if shows_tucked_head(dock) {
        rects.push(bump_rect(edge, w, h));
    }
    if shows_tucked_bar(dock) {
        rects.push(bar_rect(edge, w, h, along));
    }
    rects
}

pub fn torso_action_at(layout: &Layout, px: f64, py: f64) -> Option<TorsoAction> {
    [TorsoAction::Expand, TorsoAction::Copy, TorsoAction::Scroll]
        .into_iter()
        .find(|action| layout.torso_action_rect(*action).contains(px, py))
}

/// Radius of the tucked "bump" — smaller than the head so it frees screen space.
pub const BUMP_R: f32 = 34.0;
/// H2 — stroked alert halo around the tucked bump (outside the face, never covering eyes).
const BUMP_HALO_OUTSET: f32 = 4.0;
const BUMP_HALO_STROKE: f32 = 3.0;

/// Mirror of the nudge used inside the frozen `draw_bump` for the sleeping face anchor.
/// Value must stay identical; the comment is the sync point.
const BUMP_FACE_NUDGE: f32 = 0.45;
/// Horizontal spacing for awake eyes on the tucked head (matches draw_closed_eyes ±8).
const BUMP_EYE_DX: f32 = 8.0;
const BUMP_EYE_WHITE_R: f32 = 7.0;
const BUMP_EYE_PUPIL_R: f32 = 3.0;
/// Reuses the figure's eye white verbatim (named + pointer).
const BUMP_EYE_WHITE: [u8; 4] = [250, 250, 248, 255]; // draw_eyes white

// --- tucked "peek" extras (bubble + input drawn beside the bump) -------------------
//
// Fixed-size so the rects are pure geometry (no font) and hit-testing/input-region match
// the drawing exactly. The group (bubble above input) is anchored just inward of the bump
// on its on-screen side; for left/right edges it straddles the bump centre, for top/bottom
// it hangs inward from the edge.
const TUCK_PEEK_W: f32 = 212.0;
const TUCK_PEEK_BUBBLE_H: f32 = 88.0;
const TUCK_PEEK_INPUT_H: f32 = 34.0;
const TUCK_PEEK_GAP: f32 = 6.0;

/// Top-left of the peek group (bubble) and the shared x, clamped onto the surface.
fn tucked_peek_origin(edge: BumpEdge, w: u32, h: u32) -> (f32, f32) {
    let (wf, hf) = (w as f32, h as f32);
    let (cx, cy) = bump_center(edge, w, h);
    let group_h = TUCK_PEEK_BUBBLE_H + TUCK_PEEK_GAP + TUCK_PEEK_INPUT_H;
    let clear = BUMP_R + TUCK_PEEK_GAP;
    let (x, top) = match edge {
        BumpEdge::Left => (clear, cy - group_h / 2.0),
        BumpEdge::Right => (wf - clear - TUCK_PEEK_W, cy - group_h / 2.0),
        BumpEdge::Top => (cx - TUCK_PEEK_W / 2.0, clear),
        BumpEdge::Bottom => (cx - TUCK_PEEK_W / 2.0, hf - clear - group_h),
    };
    let x = x.clamp(4.0, (wf - TUCK_PEEK_W - 4.0).max(4.0));
    let top = top.clamp(4.0, (hf - group_h - 4.0).max(4.0));
    (x, top)
}

/// The peek speech bubble rect (surface-local), beside the bump on its on-screen side.
/// Expand affordance on the tucked peek bubble (single source for paint + hit).
pub fn tucked_bubble_expand_rect(edge: BumpEdge, w: u32, h: u32) -> Rect {
    expand_glyph_rect(tucked_bubble_rect(edge, w, h))
}

/// Copy-all on the tucked peek bubble, beside the expand glyph (single source for paint + hit).
pub fn tucked_bubble_copy_rect(edge: BumpEdge, w: u32, h: u32) -> Rect {
    copy_glyph_beside(tucked_bubble_expand_rect(edge, w, h))
}

pub fn tucked_bubble_rect(edge: BumpEdge, w: u32, h: u32) -> Rect {
    let (x, top) = tucked_peek_origin(edge, w, h);
    Rect { x, y: top, w: TUCK_PEEK_W, h: TUCK_PEEK_BUBBLE_H }
}

/// The peek input field rect (surface-local), directly below the peek bubble.
pub fn tucked_input_rect(edge: BumpEdge, w: u32, h: u32) -> Rect {
    let (x, top) = tucked_peek_origin(edge, w, h);
    Rect { x, y: top + TUCK_PEEK_BUBBLE_H + TUCK_PEEK_GAP, w: TUCK_PEEK_W, h: TUCK_PEEK_INPUT_H }
}

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

/// The bounding box of the whole clay figure (head ∪ torso ∪ arms-at-full-reach ∪ legs ∪
/// feet) in surface-local coordinates, for a given torso stretch. This is what `clamp_margins`
/// keeps on-screen so the buddy can never be dragged fully off — the head alone is too
/// small a guarantee now that the whole body is a drag handle. Matches the reach used by
/// `point_in_draggable_body` so the clamp protects every grabbable limb.
pub fn figure_bbox(body_len: f32) -> Rect {
    let shoulder_x = TORSO_W / 2.0 - 4.0;
    let half_w = HEAD_R.max(shoulder_x + ARM_UPPER + ARM_FORE + HAND_R + 6.0);
    let top = HEAD_CY - HEAD_R;
    let bottom = TORSO_TOP + body_len + LEG_H + FOOT_H;
    Rect { x: FIG_CX - half_w, y: top, w: half_w * 2.0, h: (bottom - top).max(0.0) }
}

/// Minimum sliver of the figure that must stay visible on each axis when dragging —
/// belt-and-suspenders beyond the body-drag handle, so a fast drag can never park the
/// buddy fully off-screen even if the head slips past an edge.
pub const DRAG_KEEP_VISIBLE: f32 = 36.0;

pub fn point_in_head(px: f64, py: f64) -> bool {
    let dx = px as f32 - FIG_CX;
    let dy = py as f32 - HEAD_CY;
    dx * dx + dy * dy <= HEAD_R * HEAD_R
}

/// True if the point is on the clay figure itself — head, torso, arms, or legs. The WHOLE
/// body is a move handle, not just the head, so a buddy whose head was dragged off-screen can
/// still be grabbed by a visible arm/torso and pulled back. Spans the torso plus an arm-reach
/// flank on each side (arm upper + forearm + hand radius + a comfort margin, so the hand
/// circles at the swing extremes stay grabbable), from above the shoulders (a raised arm sits
/// above the torso top) down past the feet. Callers check the specific controls (perimeter
/// buttons, torso actions, input, feet-stretch) FIRST, so this only claims the figure's
/// non-interactive body and the flanks where the arms swing.
pub fn point_in_draggable_body(layout: &Layout, px: f64, py: f64) -> bool {
    if point_in_head(px, py) {
        return true;
    }
    let torso = layout.torso_rect();
    // Full reach of a swung arm: shoulder offset + upper + fore + hand circle + a few px of
    // grab comfort so the hand tips never fall outside the move handle.
    let shoulder_x = TORSO_W / 2.0 - 4.0;
    let reach = shoulder_x + ARM_UPPER + ARM_FORE + HAND_R + 6.0;
    // The shoulder sits 14px below the torso top; a raised arm can reach above it, so start
    // the grab rect at the torso top minus the upper-arm length (clamped so it never goes
    // above the head, which `point_in_head` already owns).
    let top = (torso.y - ARM_UPPER).max(HEAD_CY - HEAD_R);
    let body = Rect {
        x: FIG_CX - reach,
        y: top,
        w: reach * 2.0,
        h: (torso.y + torso.h + LEG_H + FOOT_H - top).max(0.0),
    };
    body.contains(px, py)
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
/// One surface pill on the connection card — label, active highlight, wired/dimmed state.
#[derive(Clone, Copy)]
pub struct SurfacePill<'a> {
    pub label: &'a str,
    pub active: bool,
    pub wired: bool,
}

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
    /// F5 activity bracket — drives the Working status line when true.
    pub activity: bool,
    /// Ordered surface pills (launchers excluded); empty when the soul has not hydrated surfaces.
    pub pills: &'a [SurfacePill<'a>],
    /// First line of the last output, shown as an idle peek (never replaces the full Text/Image cards).
    pub output_preview: Option<&'a str>,
}

#[derive(Clone, Copy)]
pub struct SurfaceDialItem<'a> {
    pub label: &'a str,
    pub availability: &'a str,
    pub active: bool,
    /// `"surface"` (switches the active surface) or `"launcher"` (opens an external tool via
    /// a reach action_request). Launcher pills render a distinct `→` glyph so they read
    /// differently from surface-switch pills at a glance.
    pub kind: &'a str,
}

/// One row of the interior (in-torso) control list. The perimeter controls fold into this list
/// when the Torso scroll action toggles the interior view on. `id` is the original perimeter
/// control (so a tap dispatches through the same `on_perimeter_control` path); `glyph` is the
/// short perimeter label (N/S/1/2/3/4/+); `text` is the surface label the row shows beside it.
#[derive(Clone, Copy)]
pub struct InteriorRow<'a> {
    pub id: PerimeterId,
    pub glyph: &'a str,
    pub text: &'a str,
    pub dim: bool,
}

/// One row of the body-local settings panel: a `label` and its current `value`. `editable` rows
/// (colour, size — genuinely body-local presentation prefs) wear a tap-chip; read-only rows
/// (posture, buddy — governance/identity the body only *reflects*, AGENTS.md law 7) render plainer
/// so the user can see at a glance what they can change here versus only view.
pub struct SettingsRow<'a> {
    pub label: &'a str,
    pub value: &'a str,
    pub editable: bool,
}

/// The in-torso onboarding panel (Build C) — the native twin of the React `OnboardingWizardPanel`.
/// Purely a render snapshot of the `panel` cue the wizard Host pushed plus the user's local edits;
/// the body owns none of the meaning (AGENTS.md law 7). `single_select` distinguishes the connect /
/// posture pick-one sections from the placement pick-many toggles.
pub struct OnboardingPanelView<'a> {
    pub title: &'a str,
    pub prompt: Option<&'a str>,
    pub options: &'a [OnboardingOptionView<'a>],
    pub fields: &'a [OnboardingFieldView<'a>],
    pub summary_rows: &'a [OnboardingRowView<'a>],
    pub primary_label: Option<&'a str>,
    #[allow(dead_code)]
    /// Connect/posture pick-one vs placement pick-many — reserved for future render hints.
    pub single_select: bool,
}

/// One selectable option row (a provider preset, a posture card, a buddy toggle).
pub struct OnboardingOptionView<'a> {
    pub label: &'a str,
    pub detail: Option<&'a str>,
    pub selected: bool,
}

/// One input field. `display` is what to paint — already masked to dots for a credential, so the
/// secret never reaches the render path. `action` labels a tap-chip (e.g. "Paste key") when the
/// field fills from the clipboard rather than the keyboard.
pub struct OnboardingFieldView<'a> {
    pub label: &'a str,
    pub display: &'a str,
    pub focused: bool,
    pub action: Option<&'a str>,
}

/// One summary receipt row: a setup step and whether its lifecycle receipt has landed yet.
pub struct OnboardingRowView<'a> {
    pub label: &'a str,
    pub recorded: bool,
}

/// The interactive rects of the onboarding panel, computed once and shared between the renderer and
/// the body's hit-test so a tap always lands on exactly what was drawn (the same contract the
/// settings panel keeps via `interior_rows_for`).
pub struct OnboardingLayout {
    pub card: Rect,
    pub title: Rect,
    pub prompt: Option<Rect>,
    /// Option / summary rows (the middle list), in order.
    pub options: Vec<Rect>,
    pub fields: Vec<Rect>,
    pub primary: Option<Rect>,
}

/// A press inside the onboarding panel, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnboardingHit {
    Option(usize),
    Field(usize),
    Primary,
}

/// Hit-test the onboarding panel using the same layout the renderer drew.
pub fn onboarding_hit_at(layout: &OnboardingLayout, px: f64, py: f64) -> Option<OnboardingHit> {
    if let Some(rect) = layout.primary {
        if rect.contains(px, py) {
            return Some(OnboardingHit::Primary);
        }
    }
    for (i, rect) in layout.fields.iter().enumerate() {
        if rect.contains(px, py) {
            return Some(OnboardingHit::Field(i));
        }
    }
    for (i, rect) in layout.options.iter().enumerate() {
        if rect.contains(px, py) {
            return Some(OnboardingHit::Option(i));
        }
    }
    None
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
    /// While tucked, also draw the latest speech in a "peek" bubble beside the bump.
    pub tucked_show_bubble: bool,
    /// While tucked, also draw a live input field beside the bump (implies the bubble).
    pub tucked_show_input: bool,
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
    /// Hold-to-bloom surface dial items, ordered active-at-12 by the body state machine.
    pub surface_bloom: &'a [SurfaceDialItem<'a>],
    /// Soul-derived route health from `surface_active.route.health`; absent means no ring.
    pub route_health: Option<&'a str>,
    /// Body-observed local→cloud transition flash. Separate from route health.
    pub route_flash: bool,
    /// Soul-derived governance alert tier from `action_result.alertLevel` (law 7: painted, never
    /// inferred). Drives the figure's boundary ring hue via `alert_level_ring_rgba` (the primary
    /// ring voice; route health is the fallback until a tier is set). Absent → no governance ring.
    pub alert_level: Option<AlertLevel>,
    /// Whether an action is in flight (body's view of its own request bracket). Used to present
    /// activity green for tips/halo/eyes without polluting the raw soul tier.
    pub activity: bool,
    /// Expanded-mode receipt ledger items (torso panel), newest first.
    pub receipt_rail: &'a [ReceiptRailItem<'a>],
    /// Top row offset into `receipt_rail` while the ledger is visible.
    pub receipt_scroll: usize,
    /// User-toggled receipt ledger at max stretch (torso scroll cycles away and back).
    pub show_receipt_ledger: bool,
    /// The interior view: perimeter controls folded into a labeled list inside the torso,
    /// toggled by the Torso scroll action. When non-empty, the torso output is hidden and a
    /// press inside the torso hits a row instead of the body-drag handle. Each item carries
    /// the control's glyph + its surface label + the dim flag (for unwired quick surfaces).
    pub interior_rows: &'a [InteriorRow<'a>],
    /// Body-local settings panel rows. Non-empty only while the settings panel is open, in which
    /// case it takes the torso over the output/interior view.
    pub settings: &'a [SettingsRow<'a>],
    /// Wizard onboarding form section (Build C). When present it owns the torso over settings,
    /// interior, and torso output.
    pub onboarding: Option<&'a OnboardingPanelView<'a>>,
    pub layout: Layout,
    pub pinned: Option<PinnedLayout>,
    pub frame: Option<FrameLayout>,
    /// Clay colour (BB_COLOR) — every shade on the figure derives from this.
    pub color: [u8; 3],
    /// Which skin paints the presence. From `BB_SKIN`, set once at startup.
    pub skin: Skin,
    /// Tucked appearance preference. From `BB_DOCK`, set once at startup; coerced under ring skin.
    pub dock_show: DockShow,
    /// When `Some`, the reader takes over the whole surface (onboarding-style takeover).
    pub reader: Option<&'a str>,
    /// Top wrapped-line offset into the reader's full text.
    pub reader_scroll: usize,
    /// Event-bracketed copy-all feedback for the reader footer (no timers).
    pub reader_copied: bool,
    /// Normalized drag-select range in the full wrapped-line list (survives scroll).
    pub reader_selection: Option<(ReaderPos, ReaderPos)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReaderPos {
    pub line: usize,
    pub ch: usize,
}

pub fn receipt_ledger_visible_for_body_len(body_len: f32) -> bool {
    body_len >= BODY_LEN_MAX
}

/// Torso scroll state machine input — pure so receipt paging and view cycling are unit-testable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TorsoScrollState {
    pub at_max_stretch: bool,
    pub interior_open: bool,
    pub show_receipt_ledger: bool,
    pub receipt_scroll: usize,
    pub receipt_count: usize,
    pub receipt_budget: usize,
}

/// Result of one Torso scroll tap: the next panel + receipt page offset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TorsoScrollResult {
    pub interior_open: bool,
    pub show_receipt_ledger: bool,
    pub receipt_scroll: usize,
}

/// Advance the torso scroll action. At max stretch the cycle is output → interior → receipt
/// (paging receipts when they overflow) → output. Below max stretch it toggles output ↔ interior.
pub fn advance_torso_scroll(state: TorsoScrollState) -> TorsoScrollResult {
    let in_receipt = state.at_max_stretch && state.show_receipt_ledger && !state.interior_open;
    if in_receipt {
        let max_scroll = state.receipt_count.saturating_sub(state.receipt_budget.max(1));
        if max_scroll > 0 && state.receipt_scroll < max_scroll {
            return TorsoScrollResult {
                interior_open: false,
                show_receipt_ledger: true,
                receipt_scroll: state.receipt_scroll + 1,
            };
        }
        return TorsoScrollResult {
            interior_open: false,
            show_receipt_ledger: false,
            receipt_scroll: 0,
        };
    }
    if state.interior_open {
        if state.at_max_stretch {
            return TorsoScrollResult {
                interior_open: false,
                show_receipt_ledger: true,
                receipt_scroll: 0,
            };
        }
        return TorsoScrollResult {
            interior_open: false,
            show_receipt_ledger: false,
            receipt_scroll: 0,
        };
    }
    TorsoScrollResult {
        interior_open: true,
        show_receipt_ledger: false,
        receipt_scroll: 0,
    }
}

pub fn clamp_receipt_scroll(scroll: usize, total: usize, budget: usize) -> usize {
    scroll.min(total.saturating_sub(budget.max(1)))
}

pub fn receipt_ledger_row_budget(layout: &Layout) -> usize {
    let content = layout.receipt_ledger_content_rect();
    if content.h <= 0.0 {
        return 0;
    }
    ((content.h + RECEIPT_LEDGER_ROW_GAP) / (RECEIPT_LEDGER_ROW_H + RECEIPT_LEDGER_ROW_GAP))
        .floor() as usize
}

pub fn receipt_ledger_card_index(
    layout: &Layout,
    scroll: usize,
    x: f64,
    y: f64,
    count: usize,
) -> Option<usize> {
    let panel = layout.output_panel_rect();
    if !panel.contains(x, y) {
        return None;
    }
    let budget = receipt_ledger_row_budget(layout);
    for vis in 0..budget {
        let idx = scroll + vis;
        if idx >= count {
            break;
        }
        if layout.receipt_ledger_row_rect(vis).contains(x, y) {
            return Some(idx);
        }
    }
    None
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

    pub fn font(&self) -> Option<&Font> {
        self.font.as_ref()
    }

    /// Render the body into a premultiplied-BGRA `wl_shm` canvas of size `w`×`h`.
    /// The pixmap matches the actual buffer so the row stride always matches;
    /// drawing uses surface-local coordinates and simply clips.
    pub fn paint(&self, canvas: &mut [u8], w: u32, h: u32, view: &BodyView) {
        let Some(mut pixmap) = Pixmap::new(w, h) else {
            return;
        };
        let gaze_dx = activity_gaze_dx(view.activity, view.t);

        // Reader takeover wins over every other mode (onboarding precedent).
        if let Some(text) = view.reader {
            if let Some(font) = &self.font {
                draw_reader(
                    &mut pixmap,
                    font,
                    text,
                    w as f32,
                    h,
                    view.reader_scroll,
                    view.reader_copied,
                    view.reader_selection,
                    view.color,
                );
            }
            blit_premultiplied_bgra(pixmap.data(), canvas);
            return;
        }

        // Tucked: the minimized bump hugging the edge, plus an optional "peek" (speech bubble,
        // and an input field) beside it when the user has cycled the tucked view open.
        if let Some(edge) = view.tucked {
            // R4: under Skin::Ring the tucked buddy paints the edge light bar (hue mirrors the
            // ring); under Skin::Clay the sleeping bump stays byte-identical. draw_bump,
            // draw_bump_halo and draw_closed_eyes bodies are untouched — only the call site
            // gains the awake-eyes gate (F3b), the same move as H2/R3.
            let dock = effective_dock_show(view.skin, view.dock_show);
            let along = bump_along_edge(edge, w, h);
            // Bar first, head second: in Both mode the head sits ON TOP of the bar (owner
            // walk ruling 2026-07-04) — the face is never cut by the bar stripe. Paint order
            // only; no fn bodies change.
            if shows_tucked_bar(dock) {
                draw_edge_bar(
                    &mut pixmap,
                    edge,
                    w,
                    h,
                    along,
                    view.color,
                    view.activity,
                    view.alert_level,
                );
            }
            if shows_tucked_head(dock) {
                draw_bump(&mut pixmap, edge, w, h, view.color);
                draw_bump_halo(&mut pixmap, edge, w, h, view.activity, view.alert_level);
                if bump_eyes_awake(view.activity) {
                    draw_bump_eyes_awake(&mut pixmap, edge, w, h, gaze_dx);
                }
            }
            if let Some(font) = &self.font {
                if view.tucked_show_bubble {
                    let (chip_provider, chip_health) = tucked_connection_chip(view);
                    draw_tucked_bubble(
                        &mut pixmap,
                        font,
                        edge,
                        w,
                        h,
                        view.speech.unwrap_or(""),
                        chip_provider,
                        chip_health,
                    );
                }
                if view.tucked_show_input {
                    draw_tucked_input(
                        &mut pixmap,
                        font,
                        edge,
                        w,
                        h,
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

        draw_body_content(&mut pixmap, self.font.as_ref(), view, bob, eye_open, face.pupil_dy, gaze_dx, &face.mouth, &pose);

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
    pupil_dx: f32,
    mouth: &Mouth,
    pose: &FigurePose,
) {
    match view.skin {
        // The laminal default: the standalone state halo, no figure. The pane content below is
        // self-backed, so it still renders; polishing that pane into ring language is F-series.
        Skin::Ring => draw_ring(pixmap, view.alert_level, view.route_health, view.route_flash),
        // The frozen figure — byte-identical to before the pivot (the ring rides its silhouette).
        // F4: the presented tier (activity wins as Ready green) drives the boundary chrome, and
        // route health no longer reaches the clay — `None` at this call site, not a fn change.
        Skin::Clay => draw_figure(
            pixmap,
            &view.layout,
            view.color,
            bob,
            pose,
            presented_alert_level(view.activity, view.alert_level),
            None,
            view.route_flash,
        ),
    }
    if let Some(font) = font {
        if let Some(panel) = view.onboarding {
            draw_onboarding_view(pixmap, font, &view.layout, panel, view.color);
        } else if !view.settings.is_empty() {
            draw_settings_view(pixmap, font, &view.layout, view.settings, view.color);
        } else if receipt_ledger_visible_for_body_len(view.layout.body_len) && view.show_receipt_ledger {
            draw_torso_receipt_ledger(
                pixmap,
                font,
                &view.layout,
                view.receipt_rail,
                view.receipt_scroll,
            );
        } else if view.interior_rows.is_empty() {
            draw_torso_output(pixmap, font, &view.layout, &view.torso_output);
        } else {
            draw_interior_view(pixmap, font, &view.layout, view.interior_rows, view.color);
        }
        if let Some(label) = view.posture_badge {
            draw_posture_badge(pixmap, font, &view.layout, label);
        }
    }
    // The face is figure behavior — it only exists on the clay skin. In ring skin the hue and its
    // cadence carry state; there is no face to draw (docs/laminal-ring-pivot.md decision 1).
    if view.skin == Skin::Clay {
        draw_eyes(pixmap, bob, eye_open, pupil_dy, pupil_dx);
        draw_mouth(pixmap, bob, mouth);
    }
    if let Some(font) = font {
        // The external perimeter ring is retired — every perimeter control now lives inside
        // the torso as a labeled interior row (drawn above, in place of the torso output).
        // Only the surface bloom dial still paints around the torso, on a long-press.
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
            // When the interior view is open, Paste/Review/Edit live as labeled rows inside the
            // torso — don't also draw the standalone perimeter buttons (they'd duplicate and
            // float outside, exactly what the interior view exists to retire). The onboarding
            // panel owns the torso too — drawing P/R/E here would overlap the form (law 7: body
            // only presents; overlapping chrome makes the Host panel unusable).
            if view.interior_rows.is_empty() && view.onboarding.is_none() {
                draw_paste_button(pixmap, font, &view.layout);
                draw_governance_button(pixmap, font, view.layout.review_button_rect(), view.review_pending, "Review");
                draw_governance_button(pixmap, font, view.layout.edit_button_rect(), view.edit_pending, "Edit");
            }
        }
    }
}

// --- colour helpers -----------------------------------------------------------------

fn rgb(c: [u8; 3]) -> Color {
    Color::from_rgba8(c[0], c[1], c[2], 255)
}

fn rgba(c: [u8; 3], a: u8) -> Color {
    Color::from_rgba8(c[0], c[1], c[2], a)
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
    alert_level: Option<AlertLevel>,
    route_health: Option<&str>,
    route_flash: bool,
) {
    let hips_y = layout.hips_y();
    let limb = solid(rgb(shade(color, 0.94)));
    draw_route_boundary_chrome(pixmap, layout, alert_level, route_health, route_flash);

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
    alert_level: Option<AlertLevel>,
    route_health: Option<&str>,
    route_flash: bool,
) {
    // Clay skin: the ring rides the figure silhouette. When neither a tier nor route health is
    // set, no ring paints — the figure itself still shows the buddy, so absence is fine here.
    // (The standalone ring skin resolves absence to Quiet instead — see `draw_ring`.)
    if let Some([r, g, b, a]) = ring_hue_rgba(alert_level, route_health) {
        stroke_figure_boundary(pixmap, layout, Color::from_rgba8(r, g, b, a), 2.0, 4.0);
    }
    if route_flash {
        stroke_figure_boundary(pixmap, layout, Color::from_rgba8(238, 162, 55, 205), 3.0, 8.0);
    }
}

/// The R2 precedence resolved to rgba, shared by both skins: the governance alert tier is the
/// ring's primary voice; route health is the fallback until an `action_result` sets a tier (and,
/// later, until the soul folds route/provider failure into `critical`). `None` means no signal —
/// each caller decides what that renders as (clay chrome: no ring; standalone ring: Quiet).
fn ring_hue_rgba(alert_level: Option<AlertLevel>, route_health: Option<&str>) -> Option<[u8; 4]> {
    alert_level
        .map(alert_level_ring_rgba)
        .or_else(|| route_health.and_then(route_health_ring_rgba))
}

/// The single source of the ring/bar hue once precedence + the absent-tier stance are
/// applied: `alert_level` → `route_health` → `Quiet`. Both `draw_ring` and `draw_edge_bar`
/// resolve through this, so "bar hue === ring hue" is true by construction (and the tests
/// pin it). No second palette table — the only literal hues live in `alert_level_ring_rgba`
/// and `route_health_ring_rgba`.
fn ring_hue_or_quiet(alert_level: Option<AlertLevel>, route_health: Option<&str>) -> [u8; 4] {
    ring_hue_rgba(alert_level, route_health).unwrap_or_else(|| alert_level_ring_rgba(AlertLevel::Quiet))
}

/// One pure precedence: activity (F2 bracket) presents as Ready green; otherwise the raw soul tier.
/// Used for bar tips, bump halo, untucked chrome. Retires F2 halo_alert_level.
pub fn presented_alert_level(activity: bool, tier: Option<AlertLevel>) -> Option<AlertLevel> {
    if activity { Some(AlertLevel::Ready) } else { tier }
}

/// R3 — the ring detached from the figure. A standalone state halo on its **own** geometry (a
/// clean circle centred on the presence column), not a stroke of the figure's silhouette, so it
/// reads correctly with the figure absent (`BB_SKIN=ring`). It always paints: an absent tier
/// resolves to `Quiet` (the resting hue) rather than nothing, because in ring skin the ring *is*
/// the buddy — it can never vanish. Hue precedence is R2's (alert_level → route_health → Quiet).
fn draw_ring(
    pixmap: &mut Pixmap,
    alert_level: Option<AlertLevel>,
    route_health: Option<&str>,
    route_flash: bool,
) {
    let [r, g, b, a] = ring_hue_or_quiet(alert_level, route_health);

    // A faint inner disc — the presence "breath" the hue washes over. Alpha well below the ring
    // so the halo reads as a ring, not a filled coin.
    if let Some(disc) = PathBuilder::from_circle(RING_CX, RING_CY, RING_R - RING_THICKNESS * 0.5) {
        let glow = Color::from_rgba8(r, g, b, (a as f32 * 0.22) as u8);
        pixmap.fill_path(&disc, &solid(glow), FillRule::Winding, Transform::identity(), None);
    }

    // The ring proper — a bold annulus in the tier hue.
    let mut stroke = Stroke::default();
    stroke.width = RING_THICKNESS;
    stroke.line_cap = tiny_skia::LineCap::Round;
    if let Some(circle) = PathBuilder::from_circle(RING_CX, RING_CY, RING_R) {
        pixmap.stroke_path(
            &circle,
            &solid(Color::from_rgba8(r, g, b, a)),
            &stroke,
            Transform::identity(),
            None,
        );
    }

    // The local→cloud transition flash rides the same halo (parity with the clay chrome) — an
    // amber pulse just outside the ring. Preserved so the signal doesn't silently vanish in ring
    // skin; it is transport chrome, not a governance tier.
    if route_flash {
        let mut flash = Stroke::default();
        flash.width = 3.0;
        flash.line_cap = tiny_skia::LineCap::Round;
        if let Some(circle) = PathBuilder::from_circle(RING_CX, RING_CY, RING_R + 5.0) {
            pixmap.stroke_path(
                &circle,
                &solid(Color::from_rgba8(238, 162, 55, 205)),
                &flash,
                Transform::identity(),
                None,
            );
        }
    }
}

/// The bar body is always the buddy's instance color. Traffic-light tips (if any) carry the
/// presented tier hue at the ends. Quiet/None: no tips — pure identity color. Route health
/// no longer affects the bar.
fn draw_edge_bar(
    pixmap: &mut Pixmap,
    edge: BumpEdge,
    w: u32,
    h: u32,
    along: f32,
    color: [u8; 3],
    activity: bool,
    tier: Option<AlertLevel>,
) {
    let rect = bar_rect(edge, w, h, along);
    // Body: instance color.
    let [br, bg, bb] = color;
    if let Some(path) = round_rect_path(rect, 0.0) {
        pixmap.fill_path(
            &path,
            &solid(Color::from_rgba8(br, bg, bb, BAR_BODY_ALPHA)),
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
    // Tips only for non-Quiet presented tier. Source blend: the tip zone IS the palette
    // value, not a palette-over-clay blend — the traffic light must read the same hue on
    // every instance color (the ratified tip law: tip pixel == palette hue).
    if let Some(level) = presented_alert_level(activity, tier) {
        if level != AlertLevel::Quiet {
            let tips = bar_tip_rects(&rect, edge);
            let [tr, tg, tb, ta] = alert_level_ring_rgba(level);
            let mut tip_paint = solid(Color::from_rgba8(tr, tg, tb, ta));
            tip_paint.blend_mode = tiny_skia::BlendMode::Source;
            for &tip in &tips {
                if let Some(path) = round_rect_path(tip, 0.0) {
                    pixmap.fill_path(&path, &tip_paint, FillRule::Winding, Transform::identity(), None);
                }
            }
        }
    }
}

/// Two tip rects, each BAR_TIP_FRAC of the bar's along-length, at the ends. Pure geometry.
fn bar_tip_rects(rect: &Rect, edge: BumpEdge) -> [Rect; 2] {
    let f = BAR_TIP_FRAC;
    match edge {
        BumpEdge::Left | BumpEdge::Right => {
            let tip_h = rect.h * f;
            [
                Rect { x: rect.x, y: rect.y, w: rect.w, h: tip_h },
                Rect { x: rect.x, y: rect.y + rect.h - tip_h, w: rect.w, h: tip_h },
            ]
        }
        BumpEdge::Top | BumpEdge::Bottom => {
            let tip_w = rect.w * f;
            [
                Rect { x: rect.x, y: rect.y, w: tip_w, h: rect.h },
                Rect { x: rect.x + rect.w - tip_w, y: rect.y, w: tip_w, h: rect.h },
            ]
        }
    }
}

fn route_health_ring_rgba(health: &str) -> Option<[u8; 4]> {
    match health {
        "ready" => Some([52, 168, 96, 180]),
        "degraded" => Some([218, 147, 45, 205]),
        "unavailable" => Some([210, 63, 60, 215]),
        _ => None,
    }
}

/// The one place the 5-state governance ring palette lives. Each `AlertLevel` maps to its ring
/// hue, borrowed straight from the trust vocabulary: calm green trusts, amber asks, red refuses,
/// violet guards a boundary, muted blue-grey rests. Total and exhaustive by construction — adding
/// an `AlertLevel` variant is a compile error here until it is given a hue. This is the seed the
/// route-health ring (three hues) generalizes into: the halo now speaks the full five-state set.
fn alert_level_ring_rgba(level: AlertLevel) -> [u8; 4] {
    match level {
        AlertLevel::Quiet => [122, 138, 168, 150],   // muted blue-grey — resting, barely there
        AlertLevel::Ready => [52, 168, 96, 180],      // calm green — a reply/answer is ready
        AlertLevel::Confirm => [218, 147, 45, 205],   // amber — an action waits at the gate
        AlertLevel::Blocked => [210, 63, 60, 215],    // red — refused / unwired, shown honestly
        AlertLevel::Critical => [138, 79, 214, 220],  // violet — privacy/route/provider boundary
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

/// How an interior row reads in the list — drives the glyph-chip colour and the
/// group separators, so the chat actions, surface switches, and system rows look
/// like distinct bands instead of one undifferentiated stack.
#[derive(Clone, Copy, PartialEq, Eq)]
enum RowKind {
    Action,
    Nav,
    Surface,
    System,
}

fn row_kind(id: PerimeterId) -> RowKind {
    match id {
        PerimeterId::Paste | PerimeterId::Review | PerimeterId::Edit => RowKind::Action,
        PerimeterId::ArrowN | PerimeterId::ArrowS | PerimeterId::ArrowE | PerimeterId::ArrowW => RowKind::Nav,
        PerimeterId::Quick0 | PerimeterId::Quick1 | PerimeterId::Quick2 | PerimeterId::Quick3 => RowKind::Surface,
        PerimeterId::Add => RowKind::System,
    }
}

/// Glyph-chip fill + ink for a row, all derived from the buddy's clay colour so the
/// panel reads as part of the figure. Dim (unwired) rows get a faint chip that lets
/// the dark screen show through.
fn chip_palette(kind: RowKind, color: [u8; 3], dim: bool) -> (Color, [u8; 3]) {
    if dim {
        return (rgba(lighten(color, 0.82), 70), lighten(color, 0.45));
    }
    match kind {
        // The "+ Customize" action gets the same cyan accent the governance confirm uses.
        RowKind::System => (rgba([56, 188, 214], 240), [8, 30, 38]),
        // Chat actions and surface switches share a bright pale-clay chip; nav sits a touch softer.
        RowKind::Nav => (rgba(lighten(color, 0.62), 235), shade(color, 0.34)),
        _ => (rgba(lighten(color, 0.82), 245), shade(color, 0.32)),
    }
}

fn draw_interior_view(
    pixmap: &mut Pixmap,
    font: &Font,
    layout: &Layout,
    rows: &[InteriorRow],
    color: [u8; 3],
) {
    let row_rects = layout.interior_rows_for(rows.len());
    if row_rects.is_empty() {
        return;
    }
    // One cohesive "screen" recessed into the clay torso — the rows live inside it instead
    // of floating as a stack of identical pills (which is what read as unfinished).
    let first = row_rects[0];
    let last = row_rects[row_rects.len() - 1];
    let card = Rect {
        x: first.x - 4.0,
        y: first.y - 4.0,
        w: first.w + 8.0,
        h: (last.y + last.h) - first.y + 8.0,
    };
    draw_round_rect(pixmap, card, rgba(shade(color, 0.28), 240));
    if let Some(path) = round_rect_path(card, 9.0) {
        let mut stroke = Stroke::default();
        stroke.width = 1.0;
        pixmap.stroke_path(&path, &solid(rgba(shade(color, 0.5), 200)), &stroke, Transform::identity(), None);
    }

    let label_ink = lighten(color, 0.86);
    let dim_ink = lighten(color, 0.5);

    for (i, (row, rect)) in rows.iter().zip(row_rects.iter()).enumerate() {
        let rect = *rect;
        let kind = row_kind(row.id);
        // Hairline between rows; a touch stronger where the category changes so the
        // action / surface / system groups read as separate bands.
        if i > 0 {
            let stronger = row_kind(rows[i - 1].id) != kind;
            let sep_a = if stronger { 95 } else { 38 };
            fill_round_rect(
                pixmap,
                Rect { x: rect.x + 5.0, y: rect.y - 1.0, w: rect.w - 10.0, h: 1.0 },
                0.5,
                &solid(rgba(shade(color, 0.55), sep_a)),
            );
        }

        // Leading glyph chip — the perimeter glyph muscle memory came from, now a real tile.
        let inset = (rect.h * 0.16).clamp(1.5, 4.0);
        let chip_h = (rect.h - inset * 2.0).max(0.0);
        let chip = Rect { x: rect.x + 3.0, y: rect.y + inset, w: chip_h + 4.0, h: chip_h };
        let (chip_bg, chip_ink) = chip_palette(kind, color, row.dim);
        draw_round_rect(pixmap, chip, chip_bg);
        let glyph_px = (chip.h * 0.72).clamp(7.0, 12.0);
        let gw = measure(font, row.glyph, glyph_px);
        draw_line(
            pixmap,
            font,
            row.glyph,
            chip.x + (chip.w - gw) / 2.0,
            rect.y + rect.h / 2.0 + glyph_px * 0.34,
            glyph_px,
            chip_ink,
        );

        // Label, scaled to the row so big type never overruns a short torso's rows.
        let text_px = (rect.h * 0.55).clamp(8.0, 12.0);
        let text_x = chip.x + chip.w + 7.0;
        let baseline = rect.y + rect.h / 2.0 + text_px * 0.34;
        let mut right = rect.x + rect.w - 5.0;
        // Unwired rows say so plainly with a right-aligned "soon" tag, not just a fade.
        if row.dim {
            let tag = "soon";
            let tag_px = (text_px - 1.0).clamp(7.0, 10.0);
            let tw = measure(font, tag, tag_px);
            draw_line(pixmap, font, tag, right - tw, baseline, tag_px, lighten(color, 0.42));
            right -= tw + 6.0;
        }
        let ink = if row.dim { dim_ink } else { label_ink };
        let max_w = (right - text_x).max(0.0);
        let text = truncate_to_width(font, row.text, text_px, max_w);
        draw_line(pixmap, font, &text, text_x, baseline, text_px, ink);
    }
    draw_torso_actions(pixmap, layout);
}

/// The body-local settings panel: the same recessed torso card as the interior view, but each row
/// is `label …… value`. Editable rows (colour, size) wear a bright tap-chip around the value;
/// read-only rows (posture, buddy) render the value dimmed and chip-less, so the user can tell at a
/// glance what they can change here versus only see (AGENTS.md law 7 made visible).
fn draw_settings_view(
    pixmap: &mut Pixmap,
    font: &Font,
    layout: &Layout,
    rows: &[SettingsRow],
    color: [u8; 3],
) {
    let row_rects = layout.interior_rows_for(rows.len());
    if row_rects.is_empty() {
        return;
    }
    let first = row_rects[0];
    let last = row_rects[row_rects.len() - 1];
    let card = Rect {
        x: first.x - 4.0,
        y: first.y - 4.0,
        w: first.w + 8.0,
        h: (last.y + last.h) - first.y + 8.0,
    };
    draw_round_rect(pixmap, card, rgba(shade(color, 0.28), 240));
    if let Some(path) = round_rect_path(card, 9.0) {
        let mut stroke = Stroke::default();
        stroke.width = 1.0;
        pixmap.stroke_path(&path, &solid(rgba(shade(color, 0.5), 200)), &stroke, Transform::identity(), None);
    }

    let label_ink = lighten(color, 0.86);
    let dim_ink = lighten(color, 0.5);
    for (i, (row, rect)) in rows.iter().zip(row_rects.iter()).enumerate() {
        let rect = *rect;
        if i > 0 {
            fill_round_rect(
                pixmap,
                Rect { x: rect.x + 5.0, y: rect.y - 1.0, w: rect.w - 10.0, h: 1.0 },
                0.5,
                &solid(rgba(shade(color, 0.55), 38)),
            );
        }
        let text_px = (rect.h * 0.55).clamp(8.0, 12.0);
        let baseline = rect.y + rect.h / 2.0 + text_px * 0.34;
        // Reserve the right-edge strip the torso action buttons (Expand/Copy/Scroll) occupy, so a
        // value chip never renders under them — `torso_action_at` wins that strip in hit-testing,
        // which would otherwise eat clicks meant to cycle the setting.
        let right_limit = rect.x + rect.w - 26.0;
        // Label, left.
        draw_line(pixmap, font, row.label, rect.x + 8.0, baseline, text_px, label_ink);
        // Value — chipped + bright when editable, plain + dim when read-only — anchored to the
        // reserved right limit and width-capped so it can't collide with the label.
        let value = fit_line(font, row.value, text_px, (right_limit - rect.x - 8.0).max(0.0) * 0.7);
        let vw = measure(font, &value, text_px);
        if row.editable {
            let chip_h = (text_px + 6.0).min(rect.h - 2.0);
            let chip = Rect {
                x: right_limit - vw - 12.0,
                y: rect.y + (rect.h - chip_h) / 2.0,
                w: vw + 12.0,
                h: chip_h,
            };
            draw_round_rect(pixmap, chip, rgba(lighten(color, 0.62), 235));
            draw_line(pixmap, font, &value, chip.x + 6.0, baseline, text_px, shade(color, 0.34));
        } else {
            draw_line(pixmap, font, &value, right_limit - vw, baseline, text_px, dim_ink);
        }
    }
    draw_torso_actions(pixmap, layout);
}

/// The wizard onboarding panel: soul-pushed section rendered inside the torso card. The body draws
/// only; confirming emits the Host-stamped `primary_panel` token (AGENTS.md law 7).
fn draw_onboarding_view(
    pixmap: &mut Pixmap,
    font: &Font,
    layout: &Layout,
    panel: &OnboardingPanelView,
    color: [u8; 3],
) {
    let list_rows = if panel.summary_rows.is_empty() {
        panel.options.len()
    } else {
        panel.summary_rows.len()
    };
    let panel_layout = layout.onboarding_layout(
        list_rows,
        panel.fields.len(),
        panel.prompt.is_some(),
        panel.primary_label.is_some(),
    );
    if panel_layout.card.h < 8.0 {
        return;
    }

    draw_round_rect(pixmap, panel_layout.card, rgba(shade(color, 0.28), 240));
    if let Some(path) = round_rect_path(panel_layout.card, 9.0) {
        let mut stroke = Stroke::default();
        stroke.width = 1.0;
        pixmap.stroke_path(
            &path,
            &solid(rgba(shade(color, 0.5), 200)),
            &stroke,
            Transform::identity(),
            None,
        );
    }

    let title_px = 11.0;
    let title_ink = lighten(color, 0.92);
    draw_line(
        pixmap,
        font,
        panel.title,
        panel_layout.title.x + 4.0,
        panel_layout.title.y + title_px,
        title_px,
        title_ink,
    );

    if let (Some(prompt), Some(rect)) = (panel.prompt, panel_layout.prompt) {
        let prompt_px = 8.5;
        let text = truncate_to_width(font, prompt, prompt_px, rect.w - 6.0);
        draw_line(
            pixmap,
            font,
            &text,
            rect.x + 3.0,
            rect.y + prompt_px,
            prompt_px,
            lighten(color, 0.62),
        );
    }

    let label_ink = lighten(color, 0.86);
    let dim_ink = lighten(color, 0.5);

    if !panel.summary_rows.is_empty() {
        for (row, rect) in panel.summary_rows.iter().zip(panel_layout.options.iter()) {
            let text_px = (rect.h * 0.55).clamp(8.0, 11.0);
            let baseline = rect.y + rect.h / 2.0 + text_px * 0.34;
            draw_line(pixmap, font, row.label, rect.x + 6.0, baseline, text_px, label_ink);
            let status = if row.recorded { "Recorded" } else { "Pending" };
            let sw = measure(font, status, text_px - 1.0);
            draw_line(
                pixmap,
                font,
                status,
                rect.x + rect.w - sw - 6.0,
                baseline,
                text_px - 1.0,
                if row.recorded { lighten(color, 0.72) } else { dim_ink },
            );
        }
    } else {
        for (opt, rect) in panel.options.iter().zip(panel_layout.options.iter()) {
            let bg = if opt.selected {
                rgba(lighten(color, 0.58), 235)
            } else {
                rgba(shade(color, 0.42), 180)
            };
            draw_round_rect(pixmap, *rect, bg);
            let text_px = (rect.h * 0.55).clamp(8.0, 11.0);
            let baseline = rect.y + rect.h / 2.0 + text_px * 0.34;
            draw_line(pixmap, font, opt.label, rect.x + 6.0, baseline, text_px, label_ink);
            if rect.h >= 14.0 {
                if let Some(detail) = opt.detail {
                    let detail_px = (text_px - 1.5).clamp(7.0, 9.0);
                    let max_w = (rect.w - 12.0).max(0.0);
                    let text = truncate_to_width(font, detail, detail_px, max_w);
                    draw_line(
                        pixmap,
                        font,
                        &text,
                        rect.x + 6.0,
                        rect.y + rect.h - 3.0,
                        detail_px,
                        dim_ink,
                    );
                }
            }
        }
    }

    for (field, rect) in panel.fields.iter().zip(panel_layout.fields.iter()) {
        let text_px = (rect.h * 0.52).clamp(8.0, 10.5);
        let baseline = rect.y + rect.h / 2.0 + text_px * 0.34;
        let label_w = (rect.w * 0.34).clamp(36.0, 52.0);
        draw_line(pixmap, font, field.label, rect.x + 6.0, baseline, text_px, label_ink);
        let display = if field.display.is_empty() {
            "—"
        } else {
            field.display
        };
        let action_px = (text_px - 1.0).clamp(7.0, 9.0);
        let chip_w = field
            .action
            .map(|action| measure(font, action, action_px) + 10.0)
            .unwrap_or(0.0)
            .max(0.0);
        let value_right = rect.x + rect.w - chip_w - 8.0;
        let max_value_w = (value_right - (rect.x + label_w + 6.0)).max(0.0);
        let value = truncate_to_width(font, display, text_px, max_value_w);
        let vw = measure(font, &value, text_px);
        let value_x = value_right - vw;
        let ink = if field.focused { lighten(color, 0.95) } else { dim_ink };
        draw_line(pixmap, font, &value, value_x, baseline, text_px, ink);
        if let Some(action) = field.action {
            let aw = measure(font, action, action_px);
            let chip = Rect {
                x: rect.x + rect.w - aw - 12.0,
                y: rect.y + 2.0,
                w: aw + 8.0,
                h: (rect.h - 4.0).max(10.0),
            };
            draw_round_rect(pixmap, chip, rgba(lighten(color, 0.62), 220));
            draw_line(
                pixmap,
                font,
                action,
                chip.x + 4.0,
                chip.y + chip.h * 0.72,
                action_px,
                shade(color, 0.3),
            );
        }
    }

    if let (Some(label), Some(rect)) = (panel.primary_label, panel_layout.primary) {
        draw_round_rect(pixmap, rect, rgba(lighten(color, 0.68), 245));
        let text_px = (rect.h * 0.55).clamp(8.0, 11.0);
        let tw = measure(font, label, text_px);
        draw_line(
            pixmap,
            font,
            label,
            rect.x + (rect.w - tw) / 2.0,
            rect.y + rect.h / 2.0 + text_px * 0.34,
            text_px,
            shade(color, 0.28),
        );
    }
    draw_torso_actions(pixmap, layout);
}

/// Lay the bloom pills out as two vertical columns flanking the torso — the dial renders
/// ENTIRELY OUTSIDE the torso so it never overdraws the interior list. Items fill the left column
/// top→bottom (the first ceil(count/2)) then the right column top→bottom; each column is centered
/// on the torso's vertical midline so the two sides stay balanced. Up to five pills a side (ten
/// total). Returned in item order, so the hit-test/selection index maps straight to a slot.
fn surface_bloom_rects_two_columns(torso: Rect, count: usize) -> Vec<Rect> {
    if count == 0 {
        return Vec::new();
    }
    let left_n = (count + 1) / 2;
    let right_n = count - left_n;
    let cy = torso.y + torso.h / 2.0;
    let left_x = torso.x - SURFACE_BLOOM_SIDE_GAP - SURFACE_BLOOM_W;
    let right_x = torso.x + torso.w + SURFACE_BLOOM_SIDE_GAP;
    let mut rects = Vec::with_capacity(count);
    rects.extend(surface_bloom_column(left_x, cy, left_n));
    rects.extend(surface_bloom_column(right_x, cy, right_n));
    rects
}

/// One vertical column of `n` pills at `x`, stacked with `SURFACE_BLOOM_GAP` and centered on `cy`.
fn surface_bloom_column(x: f32, cy: f32, n: usize) -> Vec<Rect> {
    if n == 0 {
        return Vec::new();
    }
    let total_h = n as f32 * SURFACE_BLOOM_H + (n as f32 - 1.0) * SURFACE_BLOOM_GAP;
    let top = cy - total_h / 2.0;
    (0..n)
        .map(|i| Rect {
            x,
            y: top + i as f32 * (SURFACE_BLOOM_H + SURFACE_BLOOM_GAP),
            w: SURFACE_BLOOM_W,
            h: SURFACE_BLOOM_H,
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
        let launcher = item.kind == "launcher";
        let bg = if item.active {
            Color::from_rgba8(28, 42, 58, 238)
        } else if unwired {
            // Opaque enough that the muted label + "soon" tag read clearly over the dark desktop —
            // the faint near-transparent wash made the pill look like an empty grey slot.
            Color::from_rgba8(232, 235, 240, 214)
        } else if launcher {
            // Launchers wear a faint warm wash so they read as "open a tool", not "switch surface".
            Color::from_rgba8(255, 244, 224, 236)
        } else {
            Color::from_rgba8(248, 250, 252, 236)
        };
        draw_round_rect(pixmap, rect, bg);
        if let Some(path) = round_rect_path(rect, 8.0) {
            let mut stroke = Stroke::default();
            stroke.width = if item.active { 2.0 } else { 1.0 };
            let edge = if item.active { 190 } else if unwired { 96 } else if launcher { 150 } else { 118 };
            pixmap.stroke_path(&path, &solid(Color::from_rgba8(0, 0, 0, edge)), &stroke, Transform::identity(), None);
        }
        // Launcher pills lead with a `→` glyph (the reach metaphor: hand off to an external
        // tool). Surface-switch pills stay label-only, so the two kinds are distinct at a glance.
        let baseline = rect.y + 15.0;
        let label_x;
        let label_text;
        if launcher {
            let glyph = "→";
            let gx = rect.x + 8.0;
            draw_line(pixmap, font, glyph, gx, baseline, 9.5, fg_for_bloom(item.active, unwired));
            label_x = gx + measure(font, glyph, 9.5) + 3.0;
            label_text = item.label;
        } else {
            label_text = item.label;
            label_x = rect.x + 4.0;
        }
        // An unwired surface isn't actionable yet — say so with a right-aligned "soon" tag (mirrors
        // the interior list) so the pill never reads as an empty grey slot, and reserve its width.
        let mut right_budget = rect.x + rect.w - 6.0;
        if unwired {
            let tag = "soon";
            let tag_px = 8.0;
            let tw = measure(font, tag, tag_px);
            draw_line(pixmap, font, tag, right_budget - tw, baseline, tag_px, [120, 128, 142]);
            right_budget -= tw + 6.0;
        }
        let label = fit_line(font, label_text, 9.5, (right_budget - label_x).max(0.0));
        let tw = measure(font, &label, 9.5);
        let fg = fg_for_bloom(item.active, unwired);
        // Left-align the label when a "soon" tag shares the row (so they can't collide); otherwise
        // centre it in the remaining budget as before.
        let label_draw_x = if unwired {
            label_x
        } else {
            label_x + (right_budget - label_x - tw).max(0.0) / 2.0
        };
        draw_line(pixmap, font, &label, label_draw_x, baseline, 9.5, fg);
    }
}

fn fg_for_bloom(active: bool, unwired: bool) -> [u8; 3] {
    if active {
        [238, 246, 255]
    } else if unwired {
        // Muted slate — clearly legible on the opaque unwired pill, still visibly subdued.
        [92, 100, 114]
    } else {
        [22, 30, 42]
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

/// H2 — alert hue stroked around the tucked bump circle. Hue only (no pulse/cadence), same
/// precedence as the ring and edge bar. `draw_bump` stays byte-identical; this is a sibling call.
/// Route health no longer falls back into the halo (clay chrome is identity only).
fn draw_bump_halo(
    pixmap: &mut Pixmap,
    edge: BumpEdge,
    w: u32,
    h: u32,
    activity: bool,
    tier: Option<AlertLevel>,
) {
    let (cx, cy) = bump_center(edge, w, h);
    let [r, g, b, a] = if let Some(level) = presented_alert_level(activity, tier) {
        alert_level_ring_rgba(level)
    } else {
        alert_level_ring_rgba(AlertLevel::Quiet)
    };
    let mut stroke = Stroke::default();
    stroke.width = BUMP_HALO_STROKE;
    stroke.line_cap = tiny_skia::LineCap::Round;
    let radius = BUMP_R + BUMP_HALO_OUTSET;
    if let Some(circle) = PathBuilder::from_circle(cx, cy, radius) {
        pixmap.stroke_path(
            &circle,
            &solid(Color::from_rgba8(r, g, b, a)),
            &stroke,
            Transform::identity(),
            None,
        );
    }
}

/// F3b sibling to `draw_bump` (and `draw_bump_halo`). Draws the awake Morph eyes (white + pupil)
/// when the tucked head is visible and activity green is on. Bodies of `draw_bump`,
/// `draw_bump_halo`, `draw_closed_eyes` and `draw_eyes` remain byte-identical.
fn draw_bump_eyes_awake(pixmap: &mut Pixmap, edge: BumpEdge, w: u32, h: u32, pupil_dx: f32) {
    let centers = bump_eye_centers(edge, w, h);
    let white = solid(Color::from_rgba8(BUMP_EYE_WHITE[0], BUMP_EYE_WHITE[1], BUMP_EYE_WHITE[2], BUMP_EYE_WHITE[3]));
    let pupil = solid(Color::from_rgba8(EYE_INK[0], EYE_INK[1], EYE_INK[2], EYE_INK[3]));
    for &(ex, ey) in &centers {
        if let Some(eye) = PathBuilder::from_circle(ex, ey, BUMP_EYE_WHITE_R) {
            pixmap.fill_path(&eye, &white, FillRule::Winding, Transform::identity(), None);
        }
        if let Some(p) = PathBuilder::from_circle(ex + pupil_dx, ey, BUMP_EYE_PUPIL_R) {
            pixmap.fill_path(&p, &pupil, FillRule::Winding, Transform::identity(), None);
        }
    }
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
fn draw_eyes(pixmap: &mut Pixmap, bob: f32, eye_open: f32, pupil_dy: f32, pupil_dx: f32) {
    let white = solid(Color::from_rgba8(250, 250, 248, 255));
    let dark = solid(Color::from_rgba8(28, 22, 18, 255));
    for sign in [-1.0_f32, 1.0] {
        let ex = FIG_CX + sign * 18.0;
        let ey = HEAD_CY - 10.0 + bob;
        if let Some(eye) = ellipse_path(ex, ey, 11.0, 14.0 * eye_open) {
            pixmap.fill_path(&eye, &white, FillRule::Winding, Transform::identity(), None);
        }
        if eye_open > 0.35 {
            if let Some(pupil) = PathBuilder::from_circle(ex + sign * 2.0 + pupil_dx, ey + 5.0 * pupil_dy, 4.5) {
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

    draw_torso_actions(pixmap, layout);
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

const SURFACE_PILL_H: f32 = 18.0;
const SURFACE_PILL_GAP: f32 = 4.0;
const SURFACE_PILL_PAD_X: f32 = 8.0;

/// Single-source layout for connection-card surface pills. Returns pill rects (origin 0,0) and
/// how many labels did not fit (caller draws an honest `+N` when hidden > 0).
pub fn surface_pill_rects(font: &Font, avail_w: f32, labels: &[&str]) -> (Vec<Rect>, usize) {
    if labels.is_empty() || avail_w <= 0.0 {
        return (Vec::new(), 0);
    }
    let mut rects = Vec::new();
    let mut x = 0.0;
    let mut shown = 0usize;
    for (i, label) in labels.iter().enumerate() {
        let pill_w = (measure(font, label, 9.0) + SURFACE_PILL_PAD_X * 2.0).max(24.0);
        let remaining = labels.len() - i;
        let overflow_w = if remaining > 1 {
            let tag = format!("+{}", remaining - 1);
            measure(font, &tag, 9.0) + SURFACE_PILL_PAD_X * 2.0
        } else {
            0.0
        };
        let need = pill_w + if remaining > 1 { SURFACE_PILL_GAP + overflow_w } else { 0.0 };
        if !rects.is_empty() && x + need > avail_w {
            break;
        }
        if rects.is_empty() && pill_w > avail_w {
            break;
        }
        if x + pill_w > avail_w {
            break;
        }
        rects.push(Rect { x, y: 0.0, w: pill_w, h: SURFACE_PILL_H });
        x += pill_w + SURFACE_PILL_GAP;
        shown += 1;
    }
    (rects, labels.len().saturating_sub(shown))
}

/// Y baseline of the route row inside a passport card content rect.
pub fn passport_route_baseline(content: Rect) -> f32 {
    content.y + 8.0 + 11.0 + 14.0
}

/// Pill-row origin inside a passport card content rect (below the route row).
pub fn passport_pill_row_origin(content: Rect) -> (f32, f32) {
    let route_baseline = passport_route_baseline(content);
    (content.x + 8.0, route_baseline + 8.0)
}

/// Positioned pill rects for hit-test and input-region registration (single-source with paint).
/// Empty when the row cannot fit above the card's bottom pad — a short torso (min stretch)
/// gets no pill row at all, and therefore no invisible click targets below the panel.
pub fn passport_pill_hit_rects(font: &Font, content: Rect, labels: &[&str]) -> Vec<Rect> {
    let (origin_x, origin_y) = passport_pill_row_origin(content);
    if origin_y + SURFACE_PILL_H > content.y + content.h - 8.0 {
        return Vec::new();
    }
    let avail_w = (content.w - 16.0).max(0.0);
    let (rel, _) = surface_pill_rects(font, avail_w, labels);
    rel.into_iter()
        .map(|r| Rect {
            x: origin_x + r.x,
            y: origin_y + r.y,
            w: r.w,
            h: r.h,
        })
        .collect()
}

/// Hit-test a press against the pill row; index matches `labels` order (launchers excluded upstream).
pub fn passport_pill_hit(font: &Font, content: Rect, labels: &[&str], px: f64, py: f64) -> Option<usize> {
    passport_pill_hit_rects(font, content, labels)
        .into_iter()
        .enumerate()
        .find_map(|(idx, rect)| rect.contains(px, py).then_some(idx))
}

fn draw_surface_pill(pixmap: &mut Pixmap, font: &Font, rect: Rect, label: &str, active: bool, wired: bool) {
    let bg = if active {
        Color::from_rgba8(255, 255, 255, 186)
    } else if !wired {
        Color::from_rgba8(232, 235, 240, 214)
    } else {
        Color::from_rgba8(248, 250, 252, 236)
    };
    fill_round_rect(pixmap, rect, 6.0, &solid(bg));
    if let Some(path) = round_rect_path(rect, 6.0) {
        let mut stroke = Stroke::default();
        stroke.width = if active { 1.5 } else { 1.0 };
        let edge = if active { 140 } else if !wired { 96 } else { 118 };
        pixmap.stroke_path(
            &path,
            &solid(Color::from_rgba8(0, 0, 0, edge)),
            &stroke,
            Transform::identity(),
            None,
        );
    }
    let fg = if active {
        [22, 30, 42]
    } else if !wired {
        [92, 100, 114]
    } else {
        [22, 30, 42]
    };
    let fitted = fit_line(font, label, 9.0, rect.w - SURFACE_PILL_PAD_X * 2.0);
    let tw = measure(font, &fitted, 9.0);
    let baseline = rect.y + 14.0;
    draw_line(
        pixmap,
        font,
        &fitted,
        rect.x + (rect.w - tw) / 2.0,
        baseline,
        9.0,
        fg,
    );
}

/// Boring, fixed-row connection card sized for the 142px torso. Rows: persona + posture tag,
/// route chip (provider · locality · health dot), optional surface pills, divider, status peek.
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

    // Row 1 — route chip: provider or honest empty, locality dot, health dot.
    let row1_baseline = row0_baseline + 14.0;
    if card.route_health == Some("degraded") {
        fill_round_rect(
            pixmap,
            Rect { x: x - 3.0, y: row1_baseline - 11.0, w: text_w + 6.0, h: 14.0 },
            6.0,
            &solid(Color::from_rgba8(218, 147, 45, 42)),
        );
    }
    let mut trail_x;
    if let Some(provider) = card.provider {
        let reserve = 18.0
            + card.locality.is_some().then_some(10.0).unwrap_or(0.0)
            + card.route_health.is_some().then_some(10.0).unwrap_or(0.0);
        let prov = fit_line(font, provider, 10.0, (text_w - reserve).max(0.0));
        draw_line(pixmap, font, &prov, x, row1_baseline, 10.0, [63, 56, 52]);
        trail_x = x + measure(font, &prov, 10.0);
    } else {
        let no_route = "No route yet";
        draw_line(pixmap, font, no_route, x, row1_baseline, 10.0, [130, 122, 114]);
        trail_x = x + measure(font, no_route, 10.0);
    }
    if let Some(loc) = card.locality {
        let dot_x = trail_x + 7.0;
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
        trail_x = dot_x + 6.0;
    }
    if let Some(health) = card.route_health {
        draw_route_health_dot(pixmap, trail_x + 3.0, row1_baseline - 3.0, health);
    }

    // Row 2 — surface pills (launchers excluded upstream; no row when empty or when the
    // row can't fit the card height — paint consumes the same guarded rects the hit-test
    // and input region use, so a skipped row skips everywhere).
    let mut div_y = row1_baseline + 6.0;
    if !card.pills.is_empty() {
        let pill_labels: Vec<&str> = card.pills.iter().map(|p| p.label).collect();
        let placed = passport_pill_hit_rects(font, rect, &pill_labels);
        if !placed.is_empty() {
            let (origin_x, origin_y) = passport_pill_row_origin(rect);
            let hidden = card.pills.len().saturating_sub(placed.len());
            for (pill_rect, pill) in placed.iter().zip(card.pills.iter()) {
                draw_surface_pill(pixmap, font, *pill_rect, pill.label, pill.active, pill.wired);
            }
            if hidden > 0 {
                let tag = format!("+{hidden}");
                let tw = measure(font, &tag, 9.0);
                let ox = placed.last().map(|r| r.x + r.w + SURFACE_PILL_GAP).unwrap_or(origin_x);
                if ox + tw <= origin_x + text_w {
                    draw_line(pixmap, font, &tag, ox, origin_y + 13.0, 9.0, [130, 122, 114]);
                }
            }
            div_y = origin_y + SURFACE_PILL_H + 4.0;
        }
    }

    // Divider.
    fill_round_rect(
        pixmap,
        Rect { x, y: div_y, w: text_w, h: 1.0 },
        0.5,
        &solid(Color::from_rgba8(0, 0, 0, 38)),
    );

    // Status / output peek — Working while activity bracket is open.
    let body_top = div_y + 6.0;
    let avail_h = (bottom - body_top).max(0.0);
    if avail_h < PANEL_TEXT_PX {
        return;
    }
    let max_lines = (avail_h / PANEL_LINE_H).floor().max(1.0) as usize;
    let body = connection_status_line(card.activity, card.provider, card.output_preview);
    let lines = wrap(font, &body, PANEL_TEXT_PX, text_w, max_lines);
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

fn draw_receipt_ledger_card(pixmap: &mut Pixmap, font: &Font, rect: Rect, item: &ReceiptRailItem) {
    if rect.w <= 0.0 || rect.h <= 0.0 {
        return;
    }
    if item.selected {
        let ring = Rect { x: rect.x - 2.0, y: rect.y - 2.0, w: rect.w + 4.0, h: rect.h + 4.0 };
        fill_round_rect(pixmap, ring, 6.0, &solid(Color::from_rgba8(58, 122, 200, 255)));
    }
    let card_alpha = if item.selected { 255 } else { 224 };
    fill_round_rect(pixmap, rect, 5.0, &solid(Color::from_rgba8(248, 250, 252, card_alpha)));
    let glyph_color = receipt_glyph_color(item.glyph);
    draw_line(pixmap, font, item.glyph, rect.x + 4.0, rect.y + 17.0, 10.0, glyph_color);

    let effector_x = rect.x + 20.0;
    let detail_budget = (rect.w - 46.0).max(24.0);
    let top = fit_line(font, item.effector, 8.0, detail_budget);
    draw_line(pixmap, font, &top, effector_x, rect.y + 10.0, 8.0, [35, 39, 43]);

    let detail = receipt_detail_line(font, item, detail_budget);
    let detail = fit_line(font, &detail, 7.5, detail_budget);
    draw_line(pixmap, font, &detail, effector_x, rect.y + 20.0, 7.5, [83, 91, 99]);

    draw_line(pixmap, font, item.time, rect.x + rect.w - 40.0, rect.y + 17.0, 7.5, [83, 91, 99]);
}

/// Expanded-mode governance ledger — lives in the stretchable torso panel.
fn draw_torso_receipt_ledger(
    pixmap: &mut Pixmap,
    font: &Font,
    layout: &Layout,
    items: &[ReceiptRailItem],
    scroll: usize,
) {
    let panel = layout.output_panel_rect();
    if panel.w <= 0.0 || panel.h <= 0.0 {
        return;
    }

    let bg = Color::from_rgba8(36, 42, 48, 220);
    let rim = solid(Color::from_rgba8(0, 0, 0, 153));
    draw_round_rect(pixmap, panel, bg);
    if let Some(path) = round_rect_path(panel, 8.0) {
        let mut stroke = Stroke::default();
        stroke.width = 1.0;
        pixmap.stroke_path(&path, &rim, &stroke, Transform::identity(), None);
    }

    let budget = receipt_ledger_row_budget(layout);
    let scroll = clamp_receipt_scroll(scroll, items.len(), budget);
    if items.is_empty() {
        let content = layout.receipt_ledger_content_rect();
        draw_line(
            pixmap,
            font,
            "No receipts yet",
            content.x,
            content.y + 14.0,
            PANEL_LABEL_PX,
            [180, 188, 196],
        );
    } else {
        for vis in 0..budget {
            let idx = scroll + vis;
            if idx >= items.len() {
                break;
            }
            draw_receipt_ledger_card(pixmap, font, layout.receipt_ledger_row_rect(vis), &items[idx]);
        }
        if items.len() > budget {
            let a = scroll + 1;
            let b = (scroll + budget).min(items.len());
            let footer = format!("{a}–{b} of {}", items.len());
            let content = layout.receipt_ledger_content_rect();
            draw_line(
                pixmap,
                font,
                &footer,
                content.x,
                panel.y + panel.h - 14.0,
                8.0,
                [140, 148, 156],
            );
        }
    }

    draw_torso_actions(pixmap, layout);
}

fn draw_torso_actions(pixmap: &mut Pixmap, layout: &Layout) {
    for action in [TorsoAction::Expand, TorsoAction::Copy, TorsoAction::Scroll] {
        draw_torso_action(pixmap, layout, action);
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

/// Provider + health for the tucked peek chip — mirrors the connection card's route row.
pub fn tucked_connection_chip<'a>(view: &BodyView<'a>) -> (Option<&'a str>, Option<&'a str>) {
    match &view.torso_output {
        TorsoOutput::Passport(card) => (card.provider, card.route_health),
        _ => (None, view.route_health),
    }
}

/// Route-row health disc — palette pixel via Source blend (F4 precedent).
fn draw_route_health_dot(pixmap: &mut Pixmap, cx: f32, cy: f32, health: &str) {
    let Some([r, g, b, a]) = route_health_ring_rgba(health) else {
        return;
    };
    if let Some(path) = ellipse_path(cx, cy, 3.0, 3.0) {
        let mut paint = solid(Color::from_rgba8(r, g, b, a));
        paint.blend_mode = tiny_skia::BlendMode::Source;
        pixmap.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
    }
}

/// Connection-card status: Working while the F5 activity bracket is open, else the session note.
pub fn connection_status_line(activity: bool, provider: Option<&str>, preview: Option<&str>) -> String {
    if activity {
        match provider {
            Some(p) => format!("Working — talking to {p}…"),
            None => "Working…".to_string(),
        }
    } else {
        preview
            .filter(|s| !s.is_empty())
            .unwrap_or("Idle — text, image, and file output land here.")
            .to_string()
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
        TorsoAction::Expand => draw_expand_glyph(pixmap, rect),
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

/// Speech bubble: auto-sizes its height to the wrapped text (≤ bubble_line_budget lines) and faces
/// inward, with the tail pointing at the head.
fn draw_bubble(pixmap: &mut Pixmap, font: &Font, layout: &Layout, text: &str) {
    let max = layout.bubble_rect();
    let pad_x = 14.0;
    let pad_top = 28.0;
    let budget = bubble_line_budget();
    let plain = markdown_plain_projection(text);
    let (lines, hidden) = budgeted_lines(font, &plain, TEXT_PX, max.w - pad_x * 2.0, budget);
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
    for (i, line) in lines.iter().enumerate() {
        let color = if hidden > 0 && i + 1 == lines.len() {
            [130, 122, 114]
        } else {
            [16, 24, 44]
        };
        draw_line(pixmap, font, line, rect.x + pad_x, baseline, TEXT_PX, color);
        baseline += LINE_H;
    }
    draw_outer_resize_grip(pixmap, rect, layout.facing);
    if !text.is_empty() {
        let expand = expand_glyph_rect(rect);
        draw_copy_glyph(pixmap, copy_glyph_beside(expand));
        draw_expand_glyph(pixmap, expand);
    }
}

fn draw_outer_resize_grip(pixmap: &mut Pixmap, card: Rect, facing: Facing) {
    let rect = bubble_outer_resize_rect(card, facing);
    if rect.w <= 0.0 || rect.h <= 0.0 {
        return;
    }
    draw_round_rect(pixmap, rect, Color::from_rgba8(0, 0, 0, 55));
    let icon = solid(Color::from_rgba8(255, 255, 255, 170));
    let cx = rect.x + rect.w * 0.5;
    let mut stroke = Stroke::default();
    stroke.width = 1.2;
    stroke.line_cap = tiny_skia::LineCap::Round;
    for offset in [-10.0_f32, 0.0, 10.0] {
        let cy = rect.y + rect.h * 0.5 + offset;
        let mut pb = PathBuilder::new();
        pb.move_to(cx - 2.0, cy);
        pb.line_to(cx + 2.0, cy);
        if let Some(path) = pb.finish() {
            pixmap.stroke_path(&path, &icon, &stroke, Transform::identity(), None);
        }
    }
}

fn draw_copy_glyph(pixmap: &mut Pixmap, rect: Rect) {
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
    let inset = 4.5;
    let off = 3.5;
    let mut back = PathBuilder::new();
    back.move_to(rect.x + inset + off, rect.y + inset);
    back.line_to(rect.x + rect.w - inset, rect.y + inset);
    back.line_to(rect.x + rect.w - inset, rect.y + rect.h - inset - off);
    back.line_to(rect.x + inset + off, rect.y + rect.h - inset - off);
    back.close();
    if let Some(path) = back.finish() {
        let mut stroke = Stroke::default();
        stroke.width = 1.2;
        stroke.line_cap = tiny_skia::LineCap::Round;
        stroke.line_join = tiny_skia::LineJoin::Round;
        pixmap.stroke_path(&path, &icon, &stroke, Transform::identity(), None);
    }
    let mut front = PathBuilder::new();
    front.move_to(rect.x + inset, rect.y + inset + off);
    front.line_to(rect.x + rect.w - inset - off, rect.y + inset + off);
    front.line_to(rect.x + rect.w - inset - off, rect.y + rect.h - inset);
    front.line_to(rect.x + inset, rect.y + rect.h - inset);
    front.close();
    if let Some(path) = front.finish() {
        let mut stroke = Stroke::default();
        stroke.width = 1.2;
        stroke.line_cap = tiny_skia::LineCap::Round;
        stroke.line_join = tiny_skia::LineJoin::Round;
        pixmap.stroke_path(&path, &icon, &stroke, Transform::identity(), None);
    }
}

fn draw_expand_glyph(pixmap: &mut Pixmap, rect: Rect) {
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

/// Full-surface reader takeover — scrolls the whole wrapped reply; footer reports position.
fn line_highlight_range(
    start: ReaderPos,
    end: ReaderPos,
    line_idx: usize,
    line_len: usize,
) -> Option<(usize, usize)> {
    if line_idx < start.line || line_idx > end.line {
        return None;
    }
    let (from, to) = if start.line == end.line {
        (start.ch.min(line_len), end.ch.min(line_len))
    } else if line_idx == start.line {
        (start.ch.min(line_len), line_len)
    } else if line_idx == end.line {
        (0, end.ch.min(line_len))
    } else {
        (0, line_len)
    };
    if from >= to {
        return None;
    }
    Some((from, to))
}

fn draw_reader(
    pixmap: &mut Pixmap,
    font: &Font,
    text: &str,
    surface_w: f32,
    surface_h: u32,
    scroll: usize,
    copied: bool,
    selection: Option<(ReaderPos, ReaderPos)>,
    instance_color: [u8; 3],
) {
    let card = reader_card_rect(surface_w, surface_h);
    let bg = Color::from_rgba8(247, 251, 255, 245);
    let border = solid(Color::from_rgba8(0, 0, 0, 175));
    draw_round_rect(pixmap, card, bg);
    if let Some(path) = round_rect_path(card, 14.0) {
        let mut stroke = Stroke::default();
        stroke.width = 1.0;
        pixmap.stroke_path(&path, &border, &stroke, Transform::identity(), None);
    }
    draw_copy_glyph(pixmap, reader_copy_rect(surface_w, surface_h));
    draw_expand_glyph(pixmap, reader_collapse_rect(surface_w, surface_h));
    let pad_x = 14.0;
    let mut baseline = card.y + 30.0;
    draw_line(pixmap, font, "Latest output", card.x + pad_x, baseline, PANEL_LABEL_PX, [102, 88, 76]);
    baseline += PANEL_LABEL_PX + 8.0;
    let wrapped = reader_wrapped_md_lines(font, text, card.w);
    let body_budget = reader_body_budget(surface_h, wrapped.len());
    let scroll = clamp_reader_scroll(scroll, wrapped.len(), body_budget);
    let end = (scroll + body_budget).min(wrapped.len());
    let visible = &wrapped[scroll..end];
    let norm_sel = selection.map(normalize_reader_selection);
    let highlight = Color::from_rgba8(instance_color[0], instance_color[1], instance_color[2], 70);
    for (row, line) in visible.iter().enumerate() {
        let line_idx = scroll + row;
        let plain = plain_wrapped_line(line);
        let x0 = card.x + pad_x + line.indent;
        if let Some((start, end)) = norm_sel {
            if let Some((from, to)) = line_highlight_range(start, end, line_idx, plain.chars().count()) {
                let prefix: String = plain.chars().take(from).collect();
                let slice: String = plain.chars().skip(from).take(to - from).collect();
                let hx = x0 + measure(font, &prefix, TEXT_PX);
                let hw = measure(font, &slice, TEXT_PX).max(1.0);
                let hy = baseline - TEXT_PX;
                draw_round_rect(
                    pixmap,
                    Rect { x: hx, y: hy, w: hw, h: TEXT_PX + 2.0 },
                    highlight,
                );
            }
        }
        let force_bold = line.kind == MdLineKind::Heading;
        draw_spanned_line(
            pixmap,
            font,
            &line.spans,
            x0,
            baseline,
            TEXT_PX,
            [16, 24, 44],
            force_bold,
        );
        baseline += LINE_H;
    }
    let footer = reader_footer_text(scroll, visible.len(), wrapped.len(), copied);
    if !footer.is_empty() {
        let footer_y = card.y + card.h - 10.0;
        draw_line(pixmap, font, &footer, card.x + pad_x, footer_y, PANEL_LABEL_PX, [130, 122, 114]);
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
    let max_w = layout.bubble_w - pad * 2.0;

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

/// The tucked peek bubble: a fixed-size rounded card carrying the latest speech, wrapped and
/// truncated to fit (no dynamic growth, so the drawn box matches `tucked_bubble_rect` exactly).
/// A one-line connection chip (provider + health dot) rides inside the bubble when route truth exists.
#[allow(clippy::too_many_arguments)]
fn draw_tucked_bubble(
    pixmap: &mut Pixmap,
    font: &Font,
    edge: BumpEdge,
    w: u32,
    h: u32,
    text: &str,
    route_provider: Option<&str>,
    route_health: Option<&str>,
) {
    let rect = tucked_bubble_rect(edge, w, h);
    let pad_x = 12.0;
    let mut pad_top = 18.0;
    let bg = Color::from_rgba8(247, 251, 255, 245);
    let border = solid(Color::from_rgba8(0, 0, 0, 175));
    draw_round_rect(pixmap, rect, bg);
    if let Some(path) = round_rect_path(rect, 14.0) {
        let mut stroke = Stroke::default();
        stroke.width = 1.0;
        pixmap.stroke_path(&path, &border, &stroke, Transform::identity(), None);
    }
    let show_chip = route_provider.is_some() || route_health.is_some();
    if show_chip {
        let chip_y = rect.y + 8.0;
        let mut trail_x = rect.x + pad_x;
        if let Some(provider) = route_provider {
            let fitted = fit_line(font, provider, 8.5, rect.w - pad_x * 2.0 - 12.0);
            draw_line(pixmap, font, &fitted, trail_x, chip_y + 9.0, 8.5, [63, 56, 52]);
            trail_x += measure(font, &fitted, 8.5) + 6.0;
        }
        if let Some(health) = route_health {
            draw_route_health_dot(pixmap, trail_x + 3.0, chip_y + 6.0, health);
        }
        pad_top = 28.0;
    }
    let max_lines = (((rect.h - pad_top - 6.0) / LINE_H).floor() as i32).max(1) as usize;
    let (lines, hidden) = if text.is_empty() {
        (Vec::new(), 0)
    } else {
        let plain = markdown_plain_projection(text);
        budgeted_lines(font, &plain, TEXT_PX, rect.w - pad_x * 2.0, max_lines)
    };
    let mut baseline = rect.y + pad_top;
    if lines.is_empty() {
        draw_line(pixmap, font, "…", rect.x + pad_x, baseline, TEXT_PX, [130, 138, 150]);
    } else {
        for (i, line) in lines.iter().enumerate() {
            let color = if hidden > 0 && i + 1 == lines.len() {
                [130, 122, 114]
            } else {
                [16, 24, 44]
            };
            draw_line(pixmap, font, line, rect.x + pad_x, baseline, TEXT_PX, color);
            baseline += LINE_H;
        }
    }
    if !text.is_empty() {
        let expand = tucked_bubble_expand_rect(edge, w, h);
        draw_copy_glyph(pixmap, tucked_bubble_copy_rect(edge, w, h));
        draw_expand_glyph(pixmap, expand);
    }
}

/// The tucked peek input: a single-line rounded field, focusable and submittable like the
/// full-figure chat input, so the user can talk to a tucked buddy without summoning it.
#[allow(clippy::too_many_arguments)]
fn draw_tucked_input(
    pixmap: &mut Pixmap,
    font: &Font,
    edge: BumpEdge,
    w: u32,
    h: u32,
    text: &str,
    placeholder: &str,
    focused: bool,
    t: f32,
) {
    let rect = tucked_input_rect(edge, w, h);
    let pad = 12.0;
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
    let baseline = rect.y + rect.h / 2.0 + TEXT_PX / 2.0 - 2.0;
    let is_placeholder = text.is_empty();
    let display = if is_placeholder { placeholder } else { text };
    let shown = fit_line(font, display, TEXT_PX, rect.w - pad * 2.0);
    if is_placeholder {
        if !focused {
            draw_line(pixmap, font, &shown, rect.x + pad, baseline, TEXT_PX, [130, 122, 114]);
        }
    } else {
        draw_line(pixmap, font, &shown, rect.x + pad, baseline, TEXT_PX, [30, 22, 16]);
    }
    if focused && ((t * 1.4) as i32) % 2 == 0 {
        let measured = if is_placeholder { 0.0 } else { measure(font, &shown, TEXT_PX) };
        let caret_x = rect.x + pad + measured + 2.0;
        let caret = Rect { x: caret_x, y: baseline - TEXT_PX, w: 2.0, h: TEXT_PX + 4.0 };
        draw_round_rect(pixmap, caret, Color::from_rgba8(201, 109, 60, 255));
    }
}

// --- primitives ---------------------------------------------------------------------

fn draw_round_rect(pixmap: &mut Pixmap, rect: Rect, color: Color) {
    let r = 14.0_f32.min(rect.w / 2.0).min(rect.h / 2.0);
    fill_round_rect(pixmap, rect, r, &solid(color));
}

pub fn inset_rect(rect: Rect, dx: f32, dy: f32) -> Rect {
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

// --- markdown-lite (honest projection + reader formatting) ---------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MdLineKind {
    Body,
    Heading,
    Bullet,
    Numbered(u32),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MdSpan {
    pub text: String,
    pub bold: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MdLine {
    pub kind: MdLineKind,
    pub spans: Vec<MdSpan>,
}

#[derive(Clone, Debug, PartialEq)]
struct WrappedMdLine {
    kind: MdLineKind,
    spans: Vec<MdSpan>,
    indent: f32,
}

impl MdLine {
    fn plain_prefix(&self) -> String {
        match self.kind {
            MdLineKind::Body | MdLineKind::Heading => String::new(),
            MdLineKind::Bullet => "• ".to_string(),
            MdLineKind::Numbered(n) => format!("{n}. "),
        }
    }

    pub fn plain_text(&self) -> String {
        let body: String = self.spans.iter().map(|s| s.text.as_str()).collect();
        format!("{}{}", self.plain_prefix(), body)
    }
}

pub fn markdown_lite(text: &str) -> Vec<MdLine> {
    if text.is_empty() {
        return Vec::new();
    }
    text.lines().map(parse_md_source_line).collect()
}

pub fn markdown_plain_projection(text: &str) -> String {
    markdown_lite(text)
        .into_iter()
        .map(|line| line.plain_text())
        .collect::<Vec<_>>()
        .join("\n")
}

fn parse_md_source_line(line: &str) -> MdLine {
    let hash_count = line.chars().take_while(|&c| c == '#').count();
    if hash_count > 0 {
        let rest = line[hash_count..].trim_start();
        return MdLine {
            kind: MdLineKind::Heading,
            spans: parse_inline_spans(rest),
        };
    }
    if let Some(rest) = line.strip_prefix("- ") {
        return MdLine {
            kind: MdLineKind::Bullet,
            spans: parse_inline_spans(rest),
        };
    }
    if let Some(rest) = line.strip_prefix("* ") {
        return MdLine {
            kind: MdLineKind::Bullet,
            spans: parse_inline_spans(rest),
        };
    }
    if let Some(dot) = line.find(". ") {
        let head = &line[..dot];
        if !head.is_empty() && head.chars().all(|c| c.is_ascii_digit()) {
            if let Ok(n) = head.parse::<u32>() {
                return MdLine {
                    kind: MdLineKind::Numbered(n),
                    spans: parse_inline_spans(&line[dot + 2..]),
                };
            }
        }
    }
    MdLine {
        kind: MdLineKind::Body,
        spans: parse_inline_spans(line),
    }
}

fn parse_inline_spans(text: &str) -> Vec<MdSpan> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut plain = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '*' && i + 1 < chars.len() && chars[i + 1] == '*' {
            if !plain.is_empty() {
                out.push(MdSpan { text: std::mem::take(&mut plain), bold: false });
            }
            i += 2;
            let start = i;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '*') {
                i += 1;
            }
            let inner: String = chars[start..i].iter().collect();
            out.push(MdSpan { text: inner, bold: true });
            if i + 1 < chars.len() {
                i += 2;
            }
            continue;
        }
        if chars[i] == '*' || chars[i] == '`' {
            let marker = chars[i];
            i += 1;
            let start = i;
            while i < chars.len() && chars[i] != marker {
                i += 1;
            }
            plain.extend(chars[start..i].iter());
            if i < chars.len() {
                i += 1;
            }
            continue;
        }
        plain.push(chars[i]);
        i += 1;
    }
    if !plain.is_empty() {
        out.push(MdSpan { text: plain, bold: false });
    }
    if out.is_empty() {
        out.push(MdSpan { text: String::new(), bold: false });
    }
    out
}

fn plain_with_bold_chars(spans: &[MdSpan]) -> Vec<(char, bool)> {
    spans
        .iter()
        .flat_map(|s| s.text.chars().map(|c| (c, s.bold)))
        .collect()
}

fn spans_from_chars(chars: &[(char, bool)]) -> Vec<MdSpan> {
    if chars.is_empty() {
        return vec![MdSpan { text: String::new(), bold: false }];
    }
    let mut out = Vec::new();
    let mut text = String::new();
    let mut bold = chars[0].1;
    for &(ch, is_bold) in chars {
        if is_bold == bold {
            text.push(ch);
        } else {
            out.push(MdSpan { text: std::mem::take(&mut text), bold });
            bold = is_bold;
            text.push(ch);
        }
    }
    out.push(MdSpan { text, bold });
    out
}

fn wrap_logical_md_line(font: &Font, line: &MdLine, px: f32, max_w: f32) -> Vec<WrappedMdLine> {
    let prefix = line.plain_prefix();
    let heading = line.kind == MdLineKind::Heading;
    let char_map: Vec<(char, bool)> = prefix
        .chars()
        .map(|c| (c, heading))
        .chain(plain_with_bold_chars(&line.spans).into_iter().map(|(c, b)| (c, heading || b)))
        .collect();
    let full_plain: String = char_map.iter().map(|(c, _)| *c).collect();
    if full_plain.is_empty() {
        return Vec::new();
    }
    let physical = wrap(font, &full_plain, px, max_w, usize::MAX);
    let marker_w = measure(font, &prefix, px);
    let mut offset = 0;
    physical
        .into_iter()
        .enumerate()
        .map(|(idx, phys)| {
            let len = phys.chars().count();
            let slice = &char_map[offset..offset + len];
            offset += len;
            WrappedMdLine {
                kind: line.kind,
                spans: spans_from_chars(slice),
                indent: if idx == 0 { 0.0 } else { marker_w },
            }
        })
        .collect()
}

pub fn normalize_reader_selection(sel: (ReaderPos, ReaderPos)) -> (ReaderPos, ReaderPos) {
    if sel.0.line < sel.1.line || (sel.0.line == sel.1.line && sel.0.ch <= sel.1.ch) {
        sel
    } else {
        (sel.1, sel.0)
    }
}

fn plain_wrapped_line(line: &WrappedMdLine) -> String {
    line.spans.iter().map(|s| s.text.as_str()).collect()
}

pub fn reader_hit_pos(
    font: &Font,
    text: &str,
    surface_w: f32,
    surface_h: u32,
    scroll: usize,
    x: f32,
    y: f32,
) -> Option<ReaderPos> {
    let area = reader_text_rect(surface_w, surface_h);
    if x < area.x || y < area.y || x >= area.x + area.w || y >= area.y + area.h {
        return None;
    }
    let card = reader_card_rect(surface_w, surface_h);
    let wrapped = reader_wrapped_md_lines(font, text, card.w);
    let body_budget = reader_body_budget(surface_h, wrapped.len());
    let scroll = clamp_reader_scroll(scroll, wrapped.len(), body_budget);
    let row = ((y - area.y) / LINE_H).floor() as usize;
    if row >= body_budget || scroll + row >= wrapped.len() {
        return None;
    }
    let line_idx = scroll + row;
    let wrapped_line = &wrapped[line_idx];
    let plain = plain_wrapped_line(wrapped_line);
    let x_in_line = x - area.x - wrapped_line.indent;
    let ch = hit_char_index(font, &plain, TEXT_PX, x_in_line);
    Some(ReaderPos { line: line_idx, ch })
}

pub fn reader_selection_plain(font: &Font, text: &str, card_w: f32, sel: (ReaderPos, ReaderPos)) -> String {
    let wrapped = reader_wrapped_md_lines(font, text, card_w);
    let (start, end) = normalize_reader_selection(sel);
    if wrapped.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    for line_idx in start.line..=end.line.min(wrapped.len().saturating_sub(1)) {
        let plain = plain_wrapped_line(&wrapped[line_idx]);
        let chars: Vec<char> = plain.chars().collect();
        let (from, to) = if start.line == end.line {
            (start.ch.min(chars.len()), end.ch.min(chars.len()))
        } else if line_idx == start.line {
            (start.ch.min(chars.len()), chars.len())
        } else if line_idx == end.line {
            (0, end.ch.min(chars.len()))
        } else {
            (0, chars.len())
        };
        if from < to {
            out.extend(chars[from..to].iter());
        }
        if line_idx < end.line {
            out.push('\n');
        }
    }
    out
}

fn reader_wrapped_md_lines(font: &Font, text: &str, card_w: f32) -> Vec<WrappedMdLine> {
    let text_w = card_w - 28.0;
    markdown_lite(text)
        .iter()
        .flat_map(|line| wrap_logical_md_line(font, line, TEXT_PX, text_w))
        .collect()
}

fn draw_spanned_line(
    pixmap: &mut Pixmap,
    font: &Font,
    spans: &[MdSpan],
    x: f32,
    baseline: f32,
    px: f32,
    color: [u8; 3],
    force_bold: bool,
) {
    let pw = pixmap.width();
    let w = pw as i32;
    let h = pixmap.height() as i32;
    let data = pixmap.data_mut();
    let mut pen = x;
    for span in spans {
        if span.text.is_empty() {
            continue;
        }
        let bold = force_bold || span.bold;
        for pass in 0..if bold { 2 } else { 1 } {
            let offset = if bold && pass == 1 { 0.6 } else { 0.0 };
            let mut p = pen + offset;
            for ch in span.text.chars() {
                let (metrics, bitmap) = font.rasterize(ch, px);
                let gx = p + metrics.xmin as f32;
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
                        let idx = ((px_y as u32 * pw + px_x as u32) * 4) as usize;
                        let a = cov;
                        data[idx] = ((1.0 - a) * data[idx] as f32 + a * color[2] as f32) as u8;
                        data[idx + 1] = ((1.0 - a) * data[idx + 1] as f32 + a * color[1] as f32) as u8;
                        data[idx + 2] = ((1.0 - a) * data[idx + 2] as f32 + a * color[0] as f32) as u8;
                        data[idx + 3] = 255;
                    }
                }
                p += font.metrics(ch, px).advance_width;
            }
        }
        pen += measure(font, &span.text, px);
    }
}

/// Wrap `text` into at most `budget` lines; when more would fit, reserve the last line for a
/// `+N more` marker and return how many lines were hidden.
fn budgeted_lines(font: &Font, text: &str, px: f32, max_w: f32, budget: usize) -> (Vec<String>, usize) {
    let all = wrap(font, text, px, max_w, usize::MAX);
    if all.len() <= budget {
        return (all, 0);
    }
    let content_slots = budget.saturating_sub(1);
    let hidden = all.len() - content_slots;
    let mut shown: Vec<String> = all[..content_slots].to_vec();
    shown.push(format!("+{hidden} more"));
    (shown, hidden)
}

fn wrap(font: &Font, text: &str, px: f32, max_w: f32, max_lines: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut last_break: Option<usize> = None;
    let mut truncated = false;

    for ch in text.chars() {
        if ch == '\n' {
            push_wrapped_line(&mut lines, &mut current, &mut last_break, max_lines, &mut truncated);
            if lines.len() == max_lines {
                truncated = true;
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
            push_wrapped_line(&mut lines, &mut current, &mut last_break, max_lines, &mut truncated);
            if lines.len() == max_lines {
                truncated = true;
                break;
            }
            current = std::mem::take(&mut overflow);
            last_break = find_break_idx(&current);
            while measure(font, &current, px) > max_w && !current.is_empty() {
                hard_wrap_current(font, px, max_w, &mut lines, &mut current, &mut last_break, max_lines, &mut truncated);
                if lines.len() == max_lines {
                    truncated = true;
                    break;
                }
            }
        } else {
            hard_wrap_current(font, px, max_w, &mut lines, &mut current, &mut last_break, max_lines, &mut truncated);
            if lines.len() == max_lines {
                truncated = true;
                break;
            }
        }
    }

    if lines.len() < max_lines && !current.is_empty() {
        trim_line_end(&mut current);
        lines.push(current);
    } else if !current.is_empty() {
        truncated = true;
    }
    if lines.len() == max_lines {
        if let Some(last) = lines.last_mut() {
            if truncated || measure(font, last, px) > max_w {
                ellipsize_line_in_place(font, last, px, max_w);
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
    truncated: &mut bool,
) {
    let mut carry_rev = String::new();
    while measure(font, current, px) > max_w {
        let Some(ch) = current.pop() else { break };
        carry_rev.push(ch);
    }
    trim_line_end(current);
    push_wrapped_line(lines, current, last_break, max_lines, truncated);
    if lines.len() == max_lines {
        if !carry_rev.is_empty() {
            *truncated = true;
        }
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
    truncated: &mut bool,
) {
    trim_line_end(current);
    if !current.is_empty() {
        if lines.len() < max_lines {
            lines.push(std::mem::take(current));
        } else {
            *truncated = true;
            current.clear();
        }
    } else {
        current.clear();
    }
    *last_break = None;
}

/// Trim a line until `…` fits `max_w`, then append the ellipsis.
fn ellipsize_line_in_place(font: &Font, line: &mut String, px: f32, max_w: f32) {
    trim_line_end(line);
    if line.is_empty() {
        if measure(font, "…", px) <= max_w {
            line.push('…');
        }
        return;
    }
    while measure(font, &format!("{line}…"), px) > max_w && line.pop().is_some() {}
    line.push('…');
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

/// Longest prefix of `text` (with an ellipsis) that fits in `max_w` at `px`. Returns the input
/// unchanged if it already fits, or `""` if even one char + ellipsis won't fit.
fn truncate_to_width(font: &Font, text: &str, px: f32, max_w: f32) -> String {
    if measure(font, text, px) <= max_w {
        return text.to_string();
    }
    let mut out = String::new();
    for ch in text.chars() {
        let candidate = format!("{out}{ch}…");
        if measure(font, &candidate, px) > max_w {
            break;
        }
        out.push(ch);
    }
    if !out.is_empty() {
        out.push('…');
    }
    out
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
        let layout = Layout::new(Facing::Right, BODY_LEN_MIN, BUBBLE_W_DEFAULT);
        let rect = layout.output_panel_rect();
        let mut pixmap = Pixmap::new(SURFACE_W, layout.surface_h()).unwrap();
        draw_image_card(&mut pixmap, rect, &ImageCard { image: Some(&image) });
        // And the empty-frame branch (bytes still arriving / undecodable) is safe too.
        draw_image_card(&mut pixmap, rect, &ImageCard { image: None });
    }

    #[test]
    fn surface_grows_with_body_stretch() {
        let short = Layout::new(Facing::Right, BODY_LEN_MIN, BUBBLE_W_DEFAULT);
        let tall = Layout::new(Facing::Right, BODY_LEN_MAX, BUBBLE_W_DEFAULT);
        assert!(tall.surface_h() > short.surface_h());
        // Even fully squashed, the UI column still fits.
        assert!(short.surface_h() >= UI_MIN_H as u32);
    }

    #[test]
    fn receipt_ledger_is_expanded_mode_only() {
        assert!(!receipt_ledger_visible_for_body_len(BODY_LEN_MAX - 0.1));
        assert!(receipt_ledger_visible_for_body_len(BODY_LEN_MAX));
    }

    #[test]
    fn receipt_ledger_hit_maps_cards_inside_torso_panel() {
        let layout = Layout::new(Facing::Right, BODY_LEN_MAX, BUBBLE_W_DEFAULT);
        let first = layout.receipt_ledger_row_rect(0);
        assert_eq!(
            receipt_ledger_card_index(&layout, 0, (first.x + 4.0) as f64, (first.y + 4.0) as f64, 2),
            Some(0)
        );
        let second = layout.receipt_ledger_row_rect(1);
        assert_eq!(
            receipt_ledger_card_index(&layout, 0, (second.x + 4.0) as f64, (second.y + 4.0) as f64, 2),
            Some(1)
        );
        assert_eq!(receipt_ledger_card_index(&layout, 0, 0.0, 0.0, 2), None);
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
        let layout = Layout::new(Facing::Right, BODY_LEN_MAX, BUBBLE_W_DEFAULT);
        let mut pixmap = Pixmap::new(SURFACE_W, layout.surface_h()).expect("pixmap");
        draw_torso_receipt_ledger(&mut pixmap, &font, &layout, &[card(true), card(false)], 0);

        let first = layout.receipt_ledger_row_rect(0);
        let accent = |px: u32, py: u32| {
            let p = pixmap.pixel(px, py).expect("pixel in bounds");
            // Accent is (58,122,200) opaque — blue-dominant. The rail bg (36,42,48) is dark.
            p.blue() as i32 > p.red() as i32 + 40 && p.blue() > 140
        };
        assert!(
            accent((first.x - 1.0) as u32, (first.y + 8.0) as u32),
            "selected card 0 must show the accent ring in its left margin"
        );
        let second = layout.receipt_ledger_row_rect(1);
        assert!(
            !accent((second.x - 1.0) as u32, (second.y + 8.0) as u32),
            "unselected card 1 must not show an accent ring"
        );
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
        let right = Layout::new(Facing::Right, BODY_LEN_DEFAULT, BUBBLE_W_DEFAULT);
        let left = Layout::new(Facing::Left, BODY_LEN_DEFAULT, BUBBLE_W_DEFAULT);
        // Facing right: UI sits right of the figure; facing left: entirely left of it.
        assert!(right.bubble_rect().x > FIG_CX);
        assert!(left.bubble_rect().x + left.bubble_rect().w < FIG_CX);
        assert!(right.input_rect(1).x > FIG_CX);
        assert!(left.input_rect(1).x + left.input_rect(1).w < FIG_CX);
        assert!(right.bubble_rect().x + right.bubble_rect().w <= SURFACE_W as f32);
        assert!(left.bubble_rect().x >= 0.0);
    }

    #[test]
    fn figure_bbox_contains_head_torso_arms_and_feet() {
        // The drag-clamp relies on this bbox covering every grabbable part of the figure,
        // so a buddy dragged by any limb can never slip fully off-screen.
        let bbox = figure_bbox(BODY_LEN_DEFAULT);
        // Head circle sits inside the bbox horizontally and vertically.
        let head = head_rect();
        assert!(bbox.x <= head.x && bbox.x + bbox.w >= head.x + head.w);
        assert!(bbox.y <= head.y && bbox.y + bbox.h >= head.y + head.h);
        // Torso fits inside, and the bbox reaches past the arms-at-reach flank on each side.
        let torso = Layout::new(Facing::Right, BODY_LEN_DEFAULT, BUBBLE_W_DEFAULT).torso_rect();
        assert!(bbox.x <= torso.x && bbox.x + bbox.w >= torso.x + torso.w);
        let arm_reach = ARM_UPPER + ARM_FORE;
        assert!(bbox.w >= TORSO_W + arm_reach * 2.0);
        // Bottom of the bbox reaches the feet (hips + leg + foot), so a feet-grab is covered.
        let hips = TORSO_TOP + BODY_LEN_DEFAULT;
        assert!(bbox.y + bbox.h >= hips + LEG_H + FOOT_H);
    }

    #[test]
    fn figure_bbox_grows_with_body_stretch() {
        let short = figure_bbox(BODY_LEN_MIN);
        let tall = figure_bbox(BODY_LEN_MAX);
        assert!(tall.h > short.h, "a stretched torso must produce a taller figure bbox");
        // Width is independent of stretch.
        assert_eq!(short.w, tall.w);
    }

    #[test]
    fn figure_and_receipt_ledger_fit_within_surface_at_max_stretch() {
        let sw = surface_w_for_body_len(BODY_LEN_MAX);
        let w = bubble_max_w(Facing::Right, sw);
        let layout = Layout::new(Facing::Right, BODY_LEN_MAX, w);
        let bbox = figure_bbox(BODY_LEN_MAX);
        assert!(receipt_ledger_visible_for_body_len(BODY_LEN_MAX));
        assert!(bbox.x >= 0.0 && bbox.x + bbox.w <= SURFACE_W as f32,
            "figure bbox must fit inside the core SURFACE_W band");
        let panel = layout.output_panel_rect();
        assert!(panel.x >= bbox.x && panel.x + panel.w <= bbox.x + bbox.w,
            "receipt ledger panel must live inside the stretchable torso");
        assert!(
            layout.bubble_rect().w > BUBBLE_W_DEFAULT + 100.0,
            "full stretch must widen the speech column into the reclaimed rail width"
        );
        assert!(layout.bubble_rect().x + layout.bubble_rect().w <= sw - 4.0);
    }

    #[test]
    fn expanded_mode_reclaims_retired_rail_width_for_speech() {
        let sw = surface_w_for_body_len(BODY_LEN_MAX);
        assert_eq!(sw, SURFACE_W as f32 + EXPANDED_SURFACE_EXTRA as f32);
        let max_w = bubble_max_w(Facing::Right, sw);
        assert!((max_w - 349.0).abs() < 1.0, "facing right at full stretch: expected ~349px");
        assert!(bubble_max_w(Facing::Right, SURFACE_W as f32) < max_w);
    }

    #[test]
    fn draggable_body_covers_the_hand_circles_at_full_arm_reach() {
        // The hands are the part a user actually tries to grab to pull a buddy back on-screen,
        // so the move handle must cover the hand circle at the arm's full swing — not just the
        // upper+fore limb length. Compute the worst-case hand position the renderer can draw
        // (shoulder 90°, elbow 0°) and assert the hand center AND its outer edge are grabbable.
        let layout = Layout::initial();
        let shoulder_x = TORSO_W / 2.0 - 4.0;
        let max_hand_offset = shoulder_x + ARM_UPPER + ARM_FORE; // shoulder out, arm straight
        let hand_right = FIG_CX + max_hand_offset;
        let hand_left = FIG_CX - max_hand_offset;
        let shoulder_y = TORSO_TOP + 14.0;
        // Hand center at full extension (shoulder 90° → arm horizontal): same y as the shoulder.
        assert!(point_in_draggable_body(&layout, hand_right as f64, shoulder_y as f64),
            "right hand center at full reach must be grabbable");
        assert!(point_in_draggable_body(&layout, hand_left as f64, shoulder_y as f64),
            "left hand center at full reach must be grabbable");
        // The outer edge of the hand circle (HAND_R past the center) must also be inside the
        // handle — this is the "grab area on the hands is too small" regression guard.
        assert!(point_in_draggable_body(&layout, (hand_right + HAND_R) as f64, shoulder_y as f64),
            "right hand outer edge must be grabbable");
        assert!(point_in_draggable_body(&layout, (hand_left - HAND_R) as f64, shoulder_y as f64),
            "left hand outer edge must be grabbable");
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
            let l = Layout::new(facing, BODY_LEN_MIN, BUBBLE_W_DEFAULT);
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
    fn interior_rows_fit_inside_the_torso_panel_and_stack_in_order() {
        let l = Layout::new(Facing::Right, BODY_LEN_DEFAULT, BUBBLE_W_DEFAULT);
        let panel = l.output_panel_rect();
        let rows = l.interior_rows();
        // Seven perimeter controls fold into the interior list.
        assert_eq!(rows.len(), 7, "ArrowN, Quick0..3, Add, ArrowS");
        let mut prev_bottom = -f32::INFINITY;
        for (_id, rect) in &rows {
            // Every row lives inside the output panel.
            assert!(rect.x >= panel.x - 0.5 && rect.x + rect.w <= panel.x + panel.w + 0.5,
                "row x span {}..{} outside panel {}..{}", rect.x, rect.x + rect.w, panel.x, panel.x + panel.w);
            assert!(rect.y >= panel.y - 0.5 && rect.y + rect.h <= panel.y + panel.h + 0.5,
                "row y span {}..{} outside panel {}..{}", rect.y, rect.y + rect.h, panel.y, panel.y + panel.h);
            // Rows stack top-to-bottom without overlap.
            assert!(rect.y >= prev_bottom - 0.5, "row at y={} overlaps previous bottom {}", rect.y, prev_bottom);
            prev_bottom = rect.y + rect.h;
        }
        // Order is the muscle-memory order from the perimeter ring.
        let ids: Vec<_> = rows.iter().map(|(id, _)| *id).collect();
        assert_eq!(ids, vec![
            PerimeterId::ArrowN,
            PerimeterId::Quick0,
            PerimeterId::Quick1,
            PerimeterId::Quick2,
            PerimeterId::Quick3,
            PerimeterId::Add,
            PerimeterId::ArrowS,
        ]);
    }

    #[test]
    fn interior_rows_empty_when_torso_too_short_to_fit_a_row() {
        // A near-zero body length can't legibly fit even one row — fail closed rather than draw
        // a cramped, unreadable list.
        let l = Layout::new(Facing::Right, BODY_LEN_MIN, BUBBLE_W_DEFAULT);
        let rows = l.interior_rows();
        // BODY_LEN_MIN is small enough that row_h clamps below the 10px legibility floor.
        assert!(rows.is_empty(), "expected no interior rows at BODY_LEN_MIN, got {}", rows.len());
    }

    #[test]
    fn interior_rows_for_fills_the_panel_evenly_and_caps_row_height() {
        let l = Layout::new(Facing::Right, BODY_LEN_DEFAULT, BUBBLE_W_DEFAULT);
        let panel = l.output_panel_rect();
        // 10 rows (the full chat-open set) should still fit a default torso and stay in-panel.
        let rects = l.interior_rows_for(10);
        assert_eq!(rects.len(), 10);
        for r in &rects {
            assert!(r.x >= panel.x - 0.5 && r.x + r.w <= panel.x + panel.w + 0.5);
            assert!(r.y >= panel.y - 0.5 && r.y + r.h <= panel.y + panel.h + 0.5);
            assert!(r.h <= 20.5, "row height should cap at 20px, got {}", r.h);
        }
        // Rows are evenly spaced (equal height + equal gaps).
        let heights: Vec<f32> = rects.iter().map(|r| r.h).collect();
        assert!(heights.iter().all(|h| (h - heights[0]).abs() < 0.01));
    }

    #[test]
    fn interior_rows_for_zero_count_returns_empty() {
        let l = Layout::new(Facing::Right, BODY_LEN_DEFAULT, BUBBLE_W_DEFAULT);
        assert!(l.interior_rows_for(0).is_empty());
    }

    #[test]
    fn onboarding_layout_degenerate_when_content_too_short() {
        // onboarding_layout returns empty interactive rects when content.h < 24; use a torso
        // short enough that output_panel_rect().h - 16 < 24 (body_len < ~48).
        let l = Layout::new(Facing::Right, 40.0, BUBBLE_W_DEFAULT);
        let panel = l.output_panel_rect();
        assert!(panel.h < 32.0, "test fixture must exercise content.h < 24, got panel.h {}", panel.h);
        let layout = l.onboarding_layout(3, 2, true, true);
        assert!(layout.options.is_empty());
        assert!(layout.fields.is_empty());
        assert!(layout.primary.is_none());
    }

    #[test]
    fn onboarding_layout_fits_inside_torso_and_hit_test_matches_primary() {
        let l = Layout::new(Facing::Right, BODY_LEN_DEFAULT, BUBBLE_W_DEFAULT);
        let panel = l.output_panel_rect();
        let layout = l.onboarding_layout(3, 2, true, true);
        assert!(layout.card.x >= panel.x);
        assert!(layout.card.y >= panel.y);
        assert_eq!(layout.options.len(), 3);
        assert_eq!(layout.fields.len(), 2);
        let primary = layout.primary.expect("primary button");
        assert_eq!(
            onboarding_hit_at(&layout, f64::from(primary.x + 2.0), f64::from(primary.y + 2.0)),
            Some(OnboardingHit::Primary),
        );
        let opt = layout.options[1];
        assert_eq!(
            onboarding_hit_at(&layout, f64::from(opt.x + 2.0), f64::from(opt.y + 2.0)),
            Some(OnboardingHit::Option(1)),
        );
    }

    #[test]
    fn connect_panel_layout_fits_at_recommended_body_len() {
        let body_len = Layout::min_body_len_for_onboarding(4, 2, true, true);
        assert!(
            body_len > BODY_LEN_DEFAULT,
            "connect needs more than default stretch, got {body_len}"
        );
        let l = Layout::new(Facing::Right, body_len, BUBBLE_W_DEFAULT);
        let layout = l.onboarding_layout(4, 2, true, true);
        assert_eq!(layout.options.len(), 4);
        assert_eq!(layout.fields.len(), 2);
        assert!(layout.primary.is_some());
        for (i, rect) in layout.options.iter().enumerate() {
            assert!(rect.h >= 12.0, "option row {i} too short: {}", rect.h);
        }
        for (i, rect) in layout.fields.iter().enumerate() {
            assert!(rect.h >= 14.0, "field row {i} too short: {}", rect.h);
        }
        if let Some(primary) = layout.primary {
            for field in &layout.fields {
                assert!(field.y + field.h <= primary.y - 2.0, "field overlaps primary");
            }
        }
    }

    #[test]
    fn interior_view_replaces_torso_output_and_hides_the_perimeter_ring() {
        // The whole point of the interior view: outside buttons come inside, so the outside
        // ring must not render, and the interior rows must paint into the torso panel.
        let layout = Layout::initial();
        let w = SURFACE_W;
        let h = layout.surface_h();
        let sprite = Sprite::new();

        let paint = |interior: &[InteriorRow]| -> Vec<u8> {
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
                    note: "should not appear",
                }),
                chat_open: false,
                tucked: None,
                tucked_show_bubble: false,
                tucked_show_input: false,
                input_text: "",
                input_placeholder: "",
                input_focused: false,
                review_pending: false,
                edit_pending: false,
                posture_badge: None,
                surface_bloom: &[],
                route_health: None,
                route_flash: false,
                alert_level: None,
                activity: false,
                receipt_rail: &[],
                receipt_scroll: 0,
                show_receipt_ledger: false,
                interior_rows: interior,
                settings: &[],
                onboarding: None,
                layout,
                pinned: None,
                frame: None,
                color: [255, 107, 107],
                dock_show: DockShow::Both,
                skin: Skin::Clay,
                reader: None,
                reader_scroll: 0,
                reader_copied: false,
                reader_selection: None,
            };
            sprite.paint(&mut canvas, w, h, &view);
            canvas
        };

        let closed = paint(&[]);
        let open_rows: Vec<InteriorRow> = layout.interior_rows().into_iter().map(|(id, _)| InteriorRow {
            id,
            glyph: "X",
            text: "Row label",
            dim: false,
        }).collect();
        let open = paint(&open_rows);

        // The external perimeter ring is RETIRED — both the closed and open views must leave
        // the space just outside the torso transparent. (Previously the ring painted there.)
        let torso = layout.torso_rect();
        let sample = |canvas: &[u8], x: f32, y: f32| {
            let xi = x as usize;
            let yi = y as usize;
            let i = (yi * (w as usize) + xi) * 4;
            (canvas[i], canvas[i + 1], canvas[i + 2], canvas[i + 3])
        };
        let perimeter_pt = (torso.x - PERIMETER_SIZE - PERIMETER_GAP + 2.0, torso.y - PERIMETER_SIZE - PERIMETER_GAP + 2.0);
        assert_eq!(sample(&closed, perimeter_pt.0, perimeter_pt.1), (0, 0, 0, 0),
            "closed view must NOT render the retired perimeter ring");
        assert_eq!(sample(&open, perimeter_pt.0, perimeter_pt.1), (0, 0, 0, 0),
            "open view must NOT render the retired perimeter ring");

        // An interior row paints opaque ink inside the torso panel.
        let first_row = layout.interior_rows()[0].1;
        let ink_pt = (first_row.x + 8.0, first_row.y + first_row.h / 2.0);
        let open_ink = sample(&open, ink_pt.0, ink_pt.1);
        assert!(open_ink.3 > 0, "interior row should paint ink inside the torso");
    }

    #[test]
    fn unwired_interior_row_renders_fainter_than_wired() {
        // The external perimeter ring is gone; the unwired-wired dim contrast now lives in the
        // interior list. An unwired quick-surface row paints lower alpha than a wired one.
        let layout = Layout::initial();
        let w = SURFACE_W;
        let h = layout.surface_h();
        let sprite = Sprite::new();

        let paint = |dim_q0: bool| -> Vec<u8> {
            let mut canvas = vec![0_u8; (w * h * 4) as usize];
            // Build the full 7-row interior so Quick0 lands in its real slot (row index 1).
            let interior: Vec<InteriorRow> = layout
                .interior_rows()
                .iter()
                .map(|(id, _)| InteriorRow {
                    id: *id,
                    glyph: match *id {
                        PerimeterId::Quick0 => "1",
                        PerimeterId::Quick1 => "2",
                        PerimeterId::Quick2 => "3",
                        PerimeterId::Quick3 => "4",
                        PerimeterId::Add => "+",
                        PerimeterId::ArrowN => "<",
                        PerimeterId::ArrowS => ">",
                        _ => "",
                    },
                    text: "Session",
                    dim: *id == PerimeterId::Quick0 && dim_q0,
                })
                .collect();
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
                tucked_show_bubble: false,
                tucked_show_input: false,
                input_text: "",
                input_placeholder: "",
                input_focused: false,
                review_pending: false,
                edit_pending: false,
                posture_badge: None,
                surface_bloom: &[],
                route_health: None,
                route_flash: false,
                alert_level: None,
                activity: false,
                receipt_rail: &[],
                receipt_scroll: 0,
                show_receipt_ledger: false,
                interior_rows: &interior,
                settings: &[],
                onboarding: None,
                layout,
                pinned: None,
                frame: None,
                color: CLAY_DEFAULT,
                dock_show: DockShow::Both,
                skin: Skin::Clay,
                reader: None,
                reader_scroll: 0,
                reader_copied: false,
                reader_selection: None,
            };
            sprite.paint(&mut canvas, w, h, &view);
            canvas
        };

        let bright = paint(false);
        let dimmed = paint(true);

        let rect = layout
            .interior_rows()
            .into_iter()
            .find(|(id, _)| *id == PerimeterId::Quick0)
            .unwrap()
            .1;
        // The contrast now lives in the leading glyph chip: a wired row paints a bright
        // pale-clay chip (high R) while an unwired row paints it at low alpha so the dark
        // recessed screen shows through (lower R). Sample the chip, just inside its left edge
        // at the row's vertical center.
        let mid_y = (rect.y + rect.h / 2.0) as u32;
        let chip_x = (rect.x + 6.0) as u32;
        let red_at = |canvas: &[u8]| -> u32 {
            canvas[((mid_y * w as u32 + chip_x) * 4) as usize] as u32
        };
        let bright_red = red_at(&bright);
        let dimmed_red = red_at(&dimmed);
        assert!(
            dimmed_red < bright_red,
            "an unwired interior Quick0 row should paint a fainter chip (lower R) than a wired one (bright={bright_red}, dimmed={dimmed_red})",
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
    fn alert_level_ring_hues_are_total_and_distinct() {
        use std::collections::HashSet;
        // The five governance states — the body end of `decision → alertLevel → hue`. The soul
        // side (`decision → alertLevel`) is pinned by the vitest trace harness; this pins the hue.
        let all = [
            AlertLevel::Quiet,
            AlertLevel::Ready,
            AlertLevel::Confirm,
            AlertLevel::Blocked,
            AlertLevel::Critical,
        ];
        // The palette is borrowed from the trust vocabulary — assert the load-bearing hues so a
        // silent swap (e.g. confirm turning green) fails loud.
        assert_eq!(alert_level_ring_rgba(AlertLevel::Quiet), [122, 138, 168, 150]);
        assert_eq!(alert_level_ring_rgba(AlertLevel::Ready), [52, 168, 96, 180]);
        assert_eq!(alert_level_ring_rgba(AlertLevel::Confirm), [218, 147, 45, 205]);
        assert_eq!(alert_level_ring_rgba(AlertLevel::Blocked), [210, 63, 60, 215]);
        assert_eq!(alert_level_ring_rgba(AlertLevel::Critical), [138, 79, 214, 220]);
        // Every state reads as its own hue — no two governance tiers collapse to one colour.
        let distinct: HashSet<[u8; 4]> = all.iter().map(|&l| alert_level_ring_rgba(l)).collect();
        assert_eq!(distinct.len(), all.len());
    }

    #[test]
    fn alert_level_is_the_ring_hue_over_route_health() {
        let layout = Layout::initial();
        let blank = Pixmap::new(SURFACE_W, layout.surface_h()).unwrap();

        // (a) alert=Confirm(amber) over route=ready(green): the governance tier wins the ring.
        let mut with_alert = Pixmap::new(SURFACE_W, layout.surface_h()).unwrap();
        draw_route_boundary_chrome(&mut with_alert, &layout, Some(AlertLevel::Confirm), Some("ready"), false);
        assert_ne!(with_alert.data(), blank.data(), "the ring must stroke the figure boundary");

        // (b) no alert tier, same route: the route-health ring is the fallback (no regression).
        let mut route_only = Pixmap::new(SURFACE_W, layout.surface_h()).unwrap();
        draw_route_boundary_chrome(&mut route_only, &layout, None, Some("ready"), false);
        assert_ne!(route_only.data(), blank.data(), "route health is still the fallback ring");

        // Precedence proof: (a) and (b) share the same "ready" route, so if route health won the
        // ring they'd be identical. They differ → the Confirm tier (amber) overrode ready (green).
        assert_ne!(
            with_alert.data(),
            route_only.data(),
            "alert_level must take the ring over route_health when both are present",
        );
    }

    // --- R3: the ring detached into a standalone primitive (BB_SKIN=ring) --------------

    /// Render just the standalone halo into a fresh canvas.
    fn ring_only(alert: Option<AlertLevel>, route: Option<&str>, flash: bool) -> Vec<u8> {
        let mut pixmap = Pixmap::new(SURFACE_W, Layout::initial().surface_h()).unwrap();
        draw_ring(&mut pixmap, alert, route, flash);
        pixmap.data().to_vec()
    }

    #[test]
    fn standalone_ring_reads_all_five_states_distinctly() {
        use std::collections::HashSet;
        let blank = vec![0_u8; ring_only(None, None, false).len()];
        // The gate: with the figure absent, the ring alone must render every one of the five
        // governance states, and no two may collapse to the same pixels.
        let renders: Vec<Vec<u8>> = [
            AlertLevel::Quiet,
            AlertLevel::Ready,
            AlertLevel::Confirm,
            AlertLevel::Blocked,
            AlertLevel::Critical,
        ]
        .iter()
        .map(|&l| ring_only(Some(l), None, false))
        .collect();
        for (i, r) in renders.iter().enumerate() {
            assert_ne!(r, &blank, "state {i} must paint a visible ring with the figure absent");
        }
        let distinct: HashSet<&Vec<u8>> = renders.iter().collect();
        assert_eq!(distinct.len(), renders.len(), "each state must read as its own ring");
    }

    #[test]
    fn standalone_ring_never_vanishes_absent_tier_rests_at_quiet() {
        // In ring skin the halo *is* the buddy — it can never be blank. With no tier and no route
        // health, it resolves to the Quiet resting hue rather than nothing (the idle-decay stance:
        // idle rests at Quiet; it does not disappear).
        let idle = ring_only(None, None, false);
        let blank = vec![0_u8; idle.len()];
        assert_ne!(idle, blank, "an idle ring (no tier) must still be visible");
        assert_eq!(idle, ring_only(Some(AlertLevel::Quiet), None, false), "absent tier === Quiet");
        // R2's precedence survives the detachment: no tier + route 'ready' falls back to the route
        // hue (identical to the Ready tier's) and differs from the Quiet rest.
        assert_eq!(
            ring_only(None, Some("ready"), false),
            ring_only(Some(AlertLevel::Ready), None, false),
            "route health is still the fallback when no tier is set",
        );
        assert_ne!(ring_only(None, Some("ready"), false), idle, "the route fallback is not the Quiet rest");
    }

    #[test]
    fn skin_selects_ring_or_figure_through_the_paint_path() {
        // The skin field actually routes draw_body_content: same view, different skin ⇒ different
        // pixels. Proves BB_SKIN is live end-to-end, not just a parsed-and-ignored flag.
        let layout = Layout::initial();
        let w = SURFACE_W;
        let h = layout.surface_h();
        let sprite = Sprite::new();

        let paint = |skin: Skin| -> Vec<u8> {
            let mut canvas = vec![0_u8; (w * h * 4) as usize];
            let view = BodyView {
                t: 0.0,
                emotion: Emotion::Neutral,
                speech: None,
                torso_output: TorsoOutput::Text(TextCard { title: "", body: "" }),
                chat_open: false,
                tucked: None,
                tucked_show_bubble: false,
                tucked_show_input: false,
                input_text: "",
                input_placeholder: "",
                input_focused: false,
                review_pending: false,
                edit_pending: false,
                posture_badge: None,
                surface_bloom: &[],
                route_health: None,
                route_flash: false,
                alert_level: Some(AlertLevel::Confirm),
                activity: false,
                receipt_rail: &[],
                receipt_scroll: 0,
                show_receipt_ledger: false,
                interior_rows: &[],
                settings: &[],
                onboarding: None,
                layout,
                pinned: None,
                frame: None,
                color: CLAY_DEFAULT,
                dock_show: DockShow::Both,
                skin,
                reader: None,
                reader_scroll: 0,
                reader_copied: false,
                reader_selection: None,
            };
            sprite.paint(&mut canvas, w, h, &view);
            canvas
        };

        assert_ne!(paint(Skin::Ring), paint(Skin::Clay), "the skin must change what the body paints");
    }

    // --- R4: the tucked edge light bar (BB_SKIN=ring, tucked) -------------------------

    /// Render just the edge bar into a fresh canvas (mirror of `ring_only`).
    fn bar_render(edge: BumpEdge, color: [u8; 3], activity: bool, tier: Option<AlertLevel>) -> Vec<u8> {
        const BW: u32 = 200;
        const BH: u32 = 120;
        let along = bump_along_edge(edge, BW, BH);
        let mut pixmap = Pixmap::new(BW, BH).unwrap();
        draw_edge_bar(&mut pixmap, edge, BW, BH, along, color, activity, tier);
        pixmap.data().to_vec()
    }

    /// Sample the center pixel of the bar (clean fill, away from anti-aliased edges), returned
    /// as premultiplied RGBA bytes — the form tiny-skia's `Pixmap::data()` stores (RGBA; the
    /// BGRA swap happens later in `blit_premultiplied_bgra` for the Wayland SHM canvas).
    fn sample_bar_center_rgba(edge: BumpEdge, color: [u8; 3], activity: bool, tier: Option<AlertLevel>) -> [u8; 4] {
        const BW: u32 = 200;
        const BH: u32 = 120;
        let along = bump_along_edge(edge, BW, BH);
        let mut pixmap = Pixmap::new(BW, BH).unwrap();
        draw_edge_bar(&mut pixmap, edge, BW, BH, along, color, activity, tier);
        let rect = bar_rect(edge, BW, BH, along);
        let half_t = (BAR_THICKNESS as u32) / 2;
        let (sx, sy) = match edge {
            BumpEdge::Left => (half_t, (rect.y + rect.h / 2.0) as u32),
            BumpEdge::Right => (BW - half_t, (rect.y + rect.h / 2.0) as u32),
            BumpEdge::Top => ((rect.x + rect.w / 2.0) as u32, half_t),
            BumpEdge::Bottom => ((rect.x + rect.w / 2.0) as u32, BH - half_t),
        };
        let idx = ((sy * BW + sx) * 4) as usize;
        let d = pixmap.data();
        [d[idx], d[idx + 1], d[idx + 2], d[idx + 3]]
    }

    /// Demultiply a premultiplied RGBA sample back to linear RGBA (round-to-nearest, matching
    /// tiny-skia's premultiply rounding). Used to compare a painted pixel against the palette.
    fn demultiply_rgba(rgba: [u8; 4]) -> [u8; 4] {
        let [r, g, b, a] = rgba;
        if a == 0 {
            return [0, 0, 0, 0];
        }
        let div = a as u32;
        let r_lin = ((r as u32 * 255 + div / 2) / div).min(255) as u8;
        let g_lin = ((g as u32 * 255 + div / 2) / div).min(255) as u8;
        let b_lin = ((b as u32 * 255 + div / 2) / div).min(255) as u8;
        [r_lin, g_lin, b_lin, a]
    }

    fn rgba_close(a: [u8; 4], b: [u8; 4], tol: i32) -> bool {
        (0..4).all(|i| (a[i] as i32 - b[i] as i32).abs() <= tol)
    }

    #[test]
    fn bar_body_wears_instance_color_for_all_tiers() {
        let color = [180, 90, 60];
        for &level in &[Some(AlertLevel::Quiet), Some(AlertLevel::Ready), Some(AlertLevel::Confirm), Some(AlertLevel::Blocked), Some(AlertLevel::Critical), None] {
            for &edge in &[BumpEdge::Left, BumpEdge::Right, BumpEdge::Top, BumpEdge::Bottom] {
                let sampled = sample_bar_center_rgba(edge, color, false, level);
                let dem = demultiply_rgba(sampled);
                // after demul, should be close to color (alpha preserved)
                assert!(rgba_close([dem[0], dem[1], dem[2], dem[3]], [color[0], color[1], color[2], BAR_BODY_ALPHA], 10), "bar body must use instance color");
            }
        }
    }

    #[test]
    fn bar_tips_carry_the_tier_hue() {
        let color = [180, 90, 60];
        for &level in &[AlertLevel::Confirm, AlertLevel::Blocked, AlertLevel::Critical] {
            let expected = alert_level_ring_rgba(level);
            for &edge in &[BumpEdge::Left, BumpEdge::Right, BumpEdge::Top, BumpEdge::Bottom] {
                let buf = bar_render(edge, color, false, Some(level));
                // sample inside tip (near end)
                const BW: u32 = 200; const BH: u32 = 120;
                let along = bump_along_edge(edge, BW, BH);
                let rect = bar_rect(edge, BW, BH, along);
                let tip = bar_tip_rects(&rect, edge)[0];
                let sx = (tip.x + tip.w * 0.5) as u32;
                let sy = (tip.y + tip.h * 0.5) as u32;
                let idx = ((sy * BW + sx) * 4) as usize;
                let tip_px = [buf[idx], buf[idx+1], buf[idx+2], buf[idx+3]];
                let tdem = demultiply_rgba(tip_px);
                assert!(rgba_close(tdem, expected, 2), "tip pixel must be the palette hue exactly (Source blend), got {:?} want {:?}", tdem, expected);
                // bar center still color
                let cidx = (( (rect.y + rect.h/2.0) as u32 * BW + (rect.x + rect.w/2.0) as u32 ) * 4) as usize;
                let cpx = [buf[cidx], buf[cidx+1], buf[cidx+2], buf[cidx+3]];
                let cdem = demultiply_rgba(cpx);
                assert!(rgba_close([cdem[0], cdem[1], cdem[2], BAR_BODY_ALPHA], [color[0], color[1], color[2], BAR_BODY_ALPHA], 10));
            }
        }
    }

    #[test]
    fn bar_rests_clean_no_tips_on_quiet() {
        let color = [180, 90, 60];
        let buf = bar_render(BumpEdge::Left, color, false, None);
        const BW: u32 = 200; const BH: u32 = 120;
        let along = bump_along_edge(BumpEdge::Left, BW, BH);
        let rect = bar_rect(BumpEdge::Left, BW, BH, along);
        // center
        let cx = (rect.x + rect.w / 2.0) as u32;
        let cy = (rect.y + rect.h / 2.0) as u32;
        let cidx = ((cy * BW + cx) * 4) as usize;
        let cdem = demultiply_rgba([buf[cidx], buf[cidx+1], buf[cidx+2], buf[cidx+3]]);
        assert!(rgba_close([cdem[0], cdem[1], cdem[2], BAR_BODY_ALPHA], [color[0], color[1], color[2], BAR_BODY_ALPHA], 10));
        // tip zone also color
        let tip = bar_tip_rects(&rect, BumpEdge::Left)[0];
        let tx = (tip.x + tip.w / 2.0) as u32;
        let ty = (tip.y + tip.h / 2.0) as u32;
        let tidx = ((ty * BW + tx) * 4) as usize;
        let tdem = demultiply_rgba([buf[tidx], buf[tidx+1], buf[tidx+2], buf[tidx+3]]);
        assert!(rgba_close([tdem[0], tdem[1], tdem[2], BAR_BODY_ALPHA], [color[0], color[1], color[2], BAR_BODY_ALPHA], 10));
    }

    #[test]
    fn route_health_paints_neither_bar_nor_halo() {
        // `draw_edge_bar` and `draw_bump_halo` no longer take route_health at all — the type
        // signature enforces the law for those two. The remaining leak path is the untucked
        // clay figure's boundary chrome (`draw_figure` still accepts route for signature
        // freeze), so pin it through the FULL paint path: same view, route "ready" vs None,
        // no tier, no activity ⇒ byte-identical clay canvases (route paints nothing).
        let layout = Layout::initial();
        let w = SURFACE_W;
        let h = layout.surface_h();
        let sprite = Sprite::new();
        let paint = |route_health: Option<&str>| -> Vec<u8> {
            let mut canvas = vec![0_u8; (w * h * 4) as usize];
            let view = BodyView {
                t: 0.0,
                emotion: Emotion::Neutral,
                speech: None,
                torso_output: TorsoOutput::Text(TextCard { title: "", body: "" }),
                chat_open: false,
                tucked: None,
                tucked_show_bubble: false,
                tucked_show_input: false,
                input_text: "",
                input_placeholder: "",
                input_focused: false,
                review_pending: false,
                edit_pending: false,
                posture_badge: None,
                surface_bloom: &[],
                route_health,
                route_flash: false,
                alert_level: None,
                activity: false,
                receipt_rail: &[],
                receipt_scroll: 0,
                show_receipt_ledger: false,
                interior_rows: &[],
                settings: &[],
                onboarding: None,
                layout,
                pinned: None,
                frame: None,
                color: CLAY_DEFAULT,
                dock_show: DockShow::Both,
                skin: Skin::Clay,
                reader: None,
                reader_scroll: 0,
                reader_copied: false,
                reader_selection: None,
            };
            sprite.paint(&mut canvas, w, h, &view);
            canvas
        };
        assert_eq!(
            paint(Some("ready")),
            paint(None),
            "route health must not paint any clay chrome (figure ring, bar, halo)",
        );
        // And the resting bar law: no tier === Quiet === pure identity color.
        let color = [180, 90, 60];
        let bar = bar_render(BumpEdge::Left, color, false, None);
        let quiet_bar = bar_render(BumpEdge::Left, color, false, Some(AlertLevel::Quiet));
        assert_eq!(bar, quiet_bar);
        // Halo: absent tier rests at Quiet (route can't even be passed — no parameter).
        let hquiet = bump_halo_only(BumpEdge::Left, false, Some(AlertLevel::Quiet));
        let hidle = bump_halo_only(BumpEdge::Left, false, None);
        assert_eq!(hquiet, hidle);
    }

    #[test]
    fn activity_green_tips_and_eyes_without_soul_tier() {
        // activity true, tier None: green tips, eyes open
        let color = [180, 90, 60];
        let buf_act = bar_render(BumpEdge::Left, color, true, None);
        // tips: activity presents Ready — tip-center pixel must be the Ready palette hue,
        // and it must vanish (back to identity color) once activity clears (result side).
        const BW: u32 = 200;
        const BH: u32 = 120;
        let along = bump_along_edge(BumpEdge::Left, BW, BH);
        let rect = bar_rect(BumpEdge::Left, BW, BH, along);
        let tip = bar_tip_rects(&rect, BumpEdge::Left)[0];
        let tidx = (((tip.y + tip.h * 0.5) as u32 * BW + (tip.x + tip.w * 0.5) as u32) * 4) as usize;
        let tip_act = demultiply_rgba([buf_act[tidx], buf_act[tidx + 1], buf_act[tidx + 2], buf_act[tidx + 3]]);
        assert!(
            rgba_close(tip_act, alert_level_ring_rgba(AlertLevel::Ready), 2),
            "activity with no soul tier must green the tips",
        );
        let buf_done = bar_render(BumpEdge::Left, color, false, None);
        let tip_done = demultiply_rgba([buf_done[tidx], buf_done[tidx + 1], buf_done[tidx + 2], buf_done[tidx + 3]]);
        assert!(
            rgba_close([tip_done[0], tip_done[1], tip_done[2], tip_done[3]], [color[0], color[1], color[2], BAR_BODY_ALPHA], 10),
            "activity cleared: tips must return to the identity color",
        );
        // for eyes, use bump compose
        let mut b = Pixmap::new(200, 120).unwrap();
        draw_bump(&mut b, BumpEdge::Left, 200, 120, color);
        if bump_eyes_awake(true) { draw_bump_eyes_awake(&mut b, BumpEdge::Left, 200, 120, 0.0); }
        let act_b = b.data().to_vec();
        let mut b2 = Pixmap::new(200, 120).unwrap();
        draw_bump(&mut b2, BumpEdge::Left, 200, 120, color);
        if bump_eyes_awake(false) { draw_bump_eyes_awake(&mut b2, BumpEdge::Left, 200, 120, 0.0); }
        let no_b = b2.data().to_vec();
        assert_ne!(act_b, no_b);
    }

    #[test]
    fn soul_ready_tier_greens_tips_but_never_opens_eyes() {
        let color = [180, 90, 60];
        // activity false, tier Ready: tips green, eyes closed
        let buf = bar_render(BumpEdge::Left, color, false, Some(AlertLevel::Ready));
        const BW: u32 = 200;
        const BH: u32 = 120;
        let along = bump_along_edge(BumpEdge::Left, BW, BH);
        let rect = bar_rect(BumpEdge::Left, BW, BH, along);
        let tip = bar_tip_rects(&rect, BumpEdge::Left)[0];
        let tidx = (((tip.y + tip.h * 0.5) as u32 * BW + (tip.x + tip.w * 0.5) as u32) * 4) as usize;
        let tip_px = demultiply_rgba([buf[tidx], buf[tidx + 1], buf[tidx + 2], buf[tidx + 3]]);
        assert!(
            rgba_close(tip_px, alert_level_ring_rgba(AlertLevel::Ready), 2),
            "a soul-emitted Ready tier must green the tips",
        );
        // eyes closed means bump + no eyes == bump + eyes false
        let mut b = Pixmap::new(200, 120).unwrap();
        draw_bump(&mut b, BumpEdge::Left, 200, 120, color);
        let plain = b.data().to_vec();
        let mut be = Pixmap::new(200, 120).unwrap();
        draw_bump(&mut be, BumpEdge::Left, 200, 120, color);
        if bump_eyes_awake(false) { draw_bump_eyes_awake(&mut be, BumpEdge::Left, 200, 120, 0.0); }
        assert_eq!(be.data().to_vec(), plain);
    }

    #[test]
    fn skin_default_is_clay() {
        assert_eq!(Skin::default(), Skin::Clay, "BB_SKIN unset must resolve to the clay figure");
    }

    #[test]
    fn ring_tuck_half_bar_is_hittable_within_bounds() {
        const W: u32 = 200;
        const H: u32 = 120;
        let edge = BumpEdge::Left;
        let along = bump_along_edge(edge, W, H);
        let bar = bar_rect(edge, W, H, along);
        let bar_x = (BAR_THICKNESS as f64) / 2.0;
        let inside_y = (bar.y + bar.h / 2.0) as f64;
        assert!(point_in_bar(edge, W, H, along, bar_x, inside_y), "ring tuck: bar centre must hit");
        let beyond_end = (bar.y + bar.h + 4.0) as f64;
        assert!(
            !point_in_bar(edge, W, H, along, bar_x, beyond_end),
            "ring tuck: beyond the half-bar must miss",
        );
    }

    #[test]
    fn bar_full_length_when_anchor_clear_of_edges() {
        // Top/bottom: along-edge length matches the left/right bar at the head anchor.
        const W: u32 = 560;
        const H: u32 = 120;
        let along = bump_along_edge(BumpEdge::Top, W, H);
        let top = bar_rect(BumpEdge::Top, W, H, along);
        let left = bar_rect(BumpEdge::Left, W, H, bump_along_edge(BumpEdge::Left, W, H));
        assert!((top.w - left.h).abs() < 0.5, "top bar width === left bar height");
        assert!((top.x + top.w / 2.0 - along).abs() < 0.5);

        // Left/right near-edge: shrinks symmetrically; centre stays on the anchor.
        let anchor = 40.0_f32;
        let extent = 560.0_f32;
        let shrunk = bar_rect(BumpEdge::Left, 40, extent as u32, anchor);
        assert!((shrunk.y + shrunk.h / 2.0 - anchor).abs() < 0.5);
        assert!((shrunk.h - 80.0).abs() < 0.5, "len = 2*anchor when room allows");
        assert!((shrunk.y - 0.0).abs() < 0.5);
        assert!(shrunk.y + shrunk.h <= extent);
    }

    #[test]
    fn top_bottom_bar_length_matches_left_right() {
        const W: u32 = 560;
        const H: u32 = 312;
        let lr_len = bar_rect(
            BumpEdge::Left,
            W,
            H,
            bump_along_edge(BumpEdge::Left, W, H),
        )
        .h;
        for &edge in &[BumpEdge::Top, BumpEdge::Bottom] {
            let along = bump_along_edge(edge, W, H);
            let rect = bar_rect(edge, W, H, along);
            assert!(
                (rect.w - lr_len).abs() < 0.5,
                "{edge:?} along-edge length must match left/right bar",
            );
        }
    }

    #[test]
    fn bar_shrinks_symmetric_near_edge_left_right() {
        let extent = 560.0_f32;
        let along = HEAD_CY;
        let rect = bar_rect(BumpEdge::Left, 40, extent as u32, along);
        assert!((rect.y + rect.h / 2.0 - along).abs() < 0.5, "bar centre must track the head anchor");
        assert!(rect.y >= 0.0);
        assert!(rect.y + rect.h <= extent);
        assert!(rect.h >= 2.0 * BUMP_R, "never shrinks below the bump diameter");
    }

    #[test]
    fn bar_hit_matches_bar_paint() {
        const W: u32 = 200;
        const H: u32 = 120;
        let edge = BumpEdge::Left;
        let along = bump_along_edge(edge, W, H);
        let bar = bar_rect(edge, W, H, along);
        let inside_x = (BAR_THICKNESS / 2.0) as f64;
        let inside_y = (bar.y + bar.h / 2.0) as f64;
        assert!(point_in_bar(edge, W, H, along, inside_x, inside_y));
        let outside_y = (bar.y - 2.0) as f64;
        assert!(!point_in_bar(edge, W, H, along, inside_x, outside_y));
    }

    #[test]
    fn dock_head_only_bump_hits_bar_misses() {
        // Left edge: bar endpoint along y clears the bump circle (top/bottom bar is shorter now).
        const W: u32 = 40;
        const H: u32 = 312;
        let edge = BumpEdge::Left;
        let along = bump_along_edge(edge, W, H);
        let (cx, cy) = bump_center(edge, W, H);
        let bar = bar_rect(edge, W, H, along);
        let bar_x = (BAR_THICKNESS / 2.0) as f64;
        let bar_endpoint_outside_bump = [bar.y + 2.0, bar.y + bar.h - 2.0]
            .into_iter()
            .map(|y| (bar_x, y as f64))
            .find(|(x, y)| !point_in_bump(edge, W, H, *x, *y))
            .expect("bar endpoint outside bump");
        assert!(point_in_tucked_summon(Skin::Clay, DockShow::Head, edge, W, H, cx as f64, cy as f64));
        assert!(!point_in_tucked_summon(
            Skin::Clay,
            DockShow::Head,
            edge,
            W,
            H,
            bar_endpoint_outside_bump.0,
            bar_endpoint_outside_bump.1,
        ));
    }

    #[test]
    fn dock_bar_only_bar_hits_bump_misses() {
        const W: u32 = 200;
        const H: u32 = 120;
        let edge = BumpEdge::Top;
        let along = bump_along_edge(edge, W, H);
        let (cx, cy) = bump_center(edge, W, H);
        let bar = bar_rect(edge, W, H, along);
        let bar_mid_x = (bar.x + bar.w / 2.0) as f64;
        let bar_mid_y = (BAR_THICKNESS / 2.0) as f64;
        let bump_face_x = cx as f64;
        let bump_face_y = (BUMP_R * 0.7) as f64;
        assert!(point_in_bump(edge, W, H, bump_face_x, bump_face_y));
        assert!(!point_in_bar(edge, W, H, along, bump_face_x, bump_face_y));
        assert!(point_in_tucked_summon(Skin::Clay, DockShow::Bar, edge, W, H, bar_mid_x, bar_mid_y));
        assert!(!point_in_tucked_summon(Skin::Clay, DockShow::Bar, edge, W, H, bump_face_x, bump_face_y));
    }

    #[test]
    fn ring_skin_coerces_dock_to_bar() {
        const W: u32 = 200;
        const H: u32 = 120;
        let edge = BumpEdge::Left;
        let along = bump_along_edge(edge, W, H);
        let bar = bar_rect(edge, W, H, along);
        let bar_mid_x = (BAR_THICKNESS / 2.0) as f64;
        let bar_mid_y = (bar.y + bar.h / 2.0) as f64;
        let (cx, cy) = bump_center(edge, W, H);
        let bump_face_x = (BUMP_R * 0.7) as f64;
        assert!(point_in_tucked_summon(Skin::Ring, DockShow::Head, edge, W, H, bar_mid_x, bar_mid_y));
        assert!(!point_in_tucked_summon(Skin::Ring, DockShow::Head, edge, W, H, bump_face_x, cy as f64));
        assert_eq!(effective_dock_show(Skin::Ring, DockShow::Head), DockShow::Bar);
    }

    #[test]
    fn skin_gates_tucked_path_ring_vs_clay() {
        // The R3 `skin_selects_ring_or_figure_through_the_paint_path` shape, tucked variant:
        // the same tucked BodyView, Ring vs Clay, must paint different pixels. Ring paints the
        // edge bar; Clay paints the sleeping bump. Proves the skin gate is live on the tucked
        // path, not just the open path — closing the hole where the figure leaked through.
        let layout = Layout::initial();
        let w = SURFACE_W;
        let h = layout.surface_h();
        let sprite = Sprite::new();

        let paint = |skin: Skin| -> Vec<u8> {
            let mut canvas = vec![0_u8; (w * h * 4) as usize];
            let view = BodyView {
                t: 0.0,
                emotion: Emotion::Neutral,
                speech: None,
                torso_output: TorsoOutput::Text(TextCard { title: "", body: "" }),
                chat_open: false,
                tucked: Some(BumpEdge::Left),
                tucked_show_bubble: false,
                tucked_show_input: false,
                input_text: "",
                input_placeholder: "",
                input_focused: false,
                review_pending: false,
                edit_pending: false,
                posture_badge: None,
                surface_bloom: &[],
                route_health: None,
                route_flash: false,
                alert_level: Some(AlertLevel::Confirm),
                activity: false,
                receipt_rail: &[],
                receipt_scroll: 0,
                show_receipt_ledger: false,
                interior_rows: &[],
                settings: &[],
                onboarding: None,
                layout,
                pinned: None,
                frame: None,
                color: CLAY_DEFAULT,
                dock_show: DockShow::Both,
                skin,
                reader: None,
                reader_scroll: 0,
                reader_copied: false,
                reader_selection: None,
            };
            sprite.paint(&mut canvas, w, h, &view);
            canvas
        };

        assert_ne!(
            paint(Skin::Ring),
            paint(Skin::Clay),
            "a tucked ring-skin buddy must paint the bar, not the clay bump",
        );
    }

    #[test]
    fn tucked_head_paints_over_bar_in_both_dock() {
        // Owner ruling 2026-07-04: in Both mode the head renders ON TOP of the bar — the
        // face is never cut by the bar stripe. Pin it through the real paint path: at a
        // pixel inside BOTH the bump circle and the bar strip, Both must equal Head-only
        // (the bar contributes nothing under the head), while Bar-only proves the bar
        // genuinely paints that pixel when the head is absent.
        let layout = Layout::initial();
        let w = SURFACE_W;
        let h = layout.surface_h();
        let sprite = Sprite::new();
        let edge = BumpEdge::Left;

        let paint = |dock_show: DockShow| -> Vec<u8> {
            let mut canvas = vec![0_u8; (w * h * 4) as usize];
            let view = BodyView {
                t: 0.0,
                emotion: Emotion::Neutral,
                speech: None,
                torso_output: TorsoOutput::Text(TextCard { title: "", body: "" }),
                chat_open: false,
                tucked: Some(edge),
                tucked_show_bubble: false,
                tucked_show_input: false,
                input_text: "",
                input_placeholder: "",
                input_focused: false,
                review_pending: false,
                edit_pending: false,
                posture_badge: None,
                surface_bloom: &[],
                route_health: None,
                route_flash: false,
                alert_level: None,
                activity: false,
                receipt_rail: &[],
                receipt_scroll: 0,
                show_receipt_ledger: false,
                interior_rows: &[],
                settings: &[],
                onboarding: None,
                layout,
                pinned: None,
                frame: None,
                color: CLAY_DEFAULT,
                dock_show,
                skin: Skin::Clay,
                reader: None,
                reader_scroll: 0,
                reader_copied: false,
                reader_selection: None,
            };
            sprite.paint(&mut canvas, w, h, &view);
            canvas
        };

        let (cx, cy) = bump_center(edge, w, h);
        let px = (BAR_THICKNESS / 2.0) as u32; // inside the bar strip on the left edge
        let py = cy as u32;
        assert!(
            point_in_bump(edge, w, h, px as f64, py as f64),
            "sample pixel must sit inside the bump circle (cx={cx})",
        );
        let idx = ((py * w + px) * 4) as usize;
        let both = paint(DockShow::Both);
        let head = paint(DockShow::Head);
        let bar = paint(DockShow::Bar);
        assert_eq!(
            &both[idx..idx + 4],
            &head[idx..idx + 4],
            "Both dock: the head must fully cover the bar where they overlap",
        );
        assert_ne!(
            &bar[idx..idx + 4],
            &head[idx..idx + 4],
            "guard: the bar really paints this pixel when the head is absent",
        );
    }

    // --- H2: the tucked clay bump wears the alert hue ---------------------------------

    fn bump_halo_only(edge: BumpEdge, activity: bool, alert: Option<AlertLevel>) -> Vec<u8> {
        const BW: u32 = 200;
        const BH: u32 = 120;
        let mut pixmap = Pixmap::new(BW, BH).unwrap();
        draw_bump_halo(&mut pixmap, edge, BW, BH, activity, alert);
        pixmap.data().to_vec()
    }

    fn sample_bump_halo_rgba(edge: BumpEdge, alert: Option<AlertLevel>) -> [u8; 4] {
        const BW: u32 = 200;
        const BH: u32 = 120;
        let mut pixmap = Pixmap::new(BW, BH).unwrap();
        draw_bump_halo(&mut pixmap, edge, BW, BH, false, alert);
        let (cx, cy) = bump_center(edge, BW, BH);
        let r = BUMP_R + BUMP_HALO_OUTSET;
        let (sx, sy) = match edge {
            BumpEdge::Left => ((cx + r) as u32, cy as u32),
            BumpEdge::Right => ((cx - r).max(0.0) as u32, cy as u32),
            BumpEdge::Top => (cx as u32, (cy + r) as u32),
            BumpEdge::Bottom => (cx as u32, (cy - r).max(0.0) as u32),
        };
        let idx = ((sy * BW + sx) * 4) as usize;
        let d = pixmap.data();
        [d[idx], d[idx + 1], d[idx + 2], d[idx + 3]]
    }

    #[test]
    fn bump_halo_reads_all_five_states_distinctly() {
        use std::collections::HashSet;
        let blank = vec![0_u8; bump_halo_only(BumpEdge::Left, false, None).len()];
        let renders: Vec<Vec<u8>> = [
            AlertLevel::Quiet,
            AlertLevel::Ready,
            AlertLevel::Confirm,
            AlertLevel::Blocked,
            AlertLevel::Critical,
        ]
        .iter()
        .map(|&l| bump_halo_only(BumpEdge::Left, false, Some(l)))
        .collect();
        for (i, r) in renders.iter().enumerate() {
            assert_ne!(r, &blank, "state {i} must paint a visible bump halo");
        }
        let distinct: HashSet<&Vec<u8>> = renders.iter().collect();
        assert_eq!(distinct.len(), renders.len(), "each state must read as its own halo");
    }

    #[test]
    fn bump_halo_hue_equals_palette_exactly() {
        for &level in &[
            AlertLevel::Quiet,
            AlertLevel::Ready,
            AlertLevel::Confirm,
            AlertLevel::Blocked,
            AlertLevel::Critical,
        ] {
            let expected = alert_level_ring_rgba(level);
            for &edge in &[BumpEdge::Left, BumpEdge::Right, BumpEdge::Top, BumpEdge::Bottom] {
                let sampled = demultiply_rgba(sample_bump_halo_rgba(edge, Some(level)));
                assert!(
                    rgba_close(sampled, expected, 1),
                    "level {:?} edge {:?}: bump halo hue {:?} != palette {:?}",
                    level,
                    edge,
                    sampled,
                    expected,
                );
            }
        }
    }

    #[test]
    fn bump_halo_precedence_activity_over_tier() {
        // F4: the precedence that used to be alert-over-route is now activity-over-tier.
        // In flight, the halo presents Ready green no matter what the soul tier says —
        // and it is exactly the Ready halo, not a blend.
        let in_flight_confirm = bump_halo_only(BumpEdge::Left, true, Some(AlertLevel::Confirm));
        let confirm_only = bump_halo_only(BumpEdge::Left, false, Some(AlertLevel::Confirm));
        assert_ne!(
            in_flight_confirm, confirm_only,
            "activity must take the bump halo over the soul tier while in flight",
        );
        let ready_only = bump_halo_only(BumpEdge::Left, false, Some(AlertLevel::Ready));
        assert_eq!(
            in_flight_confirm, ready_only,
            "the activity halo is exactly the Ready halo, not a blend with the tier",
        );
    }

    #[test]
    fn bump_halo_never_vanishes_absent_rests_at_quiet() {
        let idle = bump_halo_only(BumpEdge::Left, false, None);
        let blank = vec![0_u8; idle.len()];
        assert_ne!(&idle, &blank, "an idle bump halo (no tier) must still be visible");
        assert_eq!(
            idle,
            bump_halo_only(BumpEdge::Left, false, Some(AlertLevel::Quiet)),
            "absent tier === Quiet on the bump halo",
        );
    }

    #[test]
    fn bump_eyes_awake_only_on_activity() {
        assert!(bump_eyes_awake(true));
        assert!(!bump_eyes_awake(false));
    }

    #[test]
    fn bump_eye_centers_ride_the_sleeping_face_anchor() {
        const BW: u32 = 200;
        const BH: u32 = 120;
        for &edge in &[BumpEdge::Left, BumpEdge::Right, BumpEdge::Top, BumpEdge::Bottom] {
            let centers = bump_eye_centers(edge, BW, BH);
            let (cx, cy) = bump_center(edge, BW, BH);
            // compute same anchor as draw_bump / draw_closed_eyes
            let (dx, dy) = match edge {
                BumpEdge::Left => (BUMP_R * BUMP_FACE_NUDGE, 0.0),
                BumpEdge::Right => (-BUMP_R * BUMP_FACE_NUDGE, 0.0),
                BumpEdge::Top => (0.0, BUMP_R * BUMP_FACE_NUDGE),
                BumpEdge::Bottom => (0.0, -BUMP_R * BUMP_FACE_NUDGE),
            };
            let ax = cx + dx;
            let ay = cy + dy - 2.0;
            // pair is screen-horizontal (same y)
            assert!((centers[0].1 - centers[1].1).abs() < 0.001);
            // x positions at anchor ± DX
            let left_x = ax - BUMP_EYE_DX;
            let right_x = ax + BUMP_EYE_DX;
            assert!((centers[0].0 - left_x).abs() < 0.001 || (centers[0].0 - right_x).abs() < 0.001);
            assert!((centers[1].0 - left_x).abs() < 0.001 || (centers[1].0 - right_x).abs() < 0.001);
            // each white fully inside bump
            for &(ex, ey) in &centers {
                let dist = ((ex - cx) * (ex - cx) + (ey - cy) * (ey - cy)).sqrt();
                assert!(dist + BUMP_EYE_WHITE_R <= BUMP_R + 0.001);
            }
        }
    }

    #[test]
    fn awake_eyes_cover_the_sleeping_lids() {
        const BW: u32 = 200;
        const BH: u32 = 120;
        let edge = BumpEdge::Left;
        let mut pix = Pixmap::new(BW, BH).unwrap();
        draw_bump(&mut pix, edge, BW, BH, [180, 100, 60]);
        draw_bump_eyes_awake(&mut pix, edge, BW, BH, 0.0);
        // compute anchor
        let (cx, cy) = bump_center(edge, BW, BH);
        let (dx, _dy) = (BUMP_R * BUMP_FACE_NUDGE, 0.0);
        let ax = cx + dx;
        let ay = cy - 2.0;
        // sample near left lid arc endpoint (should be overwritten by white)
        let sx = (ax - BUMP_EYE_DX - 5.0) as u32;
        let sy = ay as u32;
        let idx = ((sy * BW + sx) * 4) as usize;
        let d = pix.data();
        let sampled = [d[idx], d[idx + 1], d[idx + 2], d[idx + 3]];
        // near white (lid gone)
        let white = BUMP_EYE_WHITE;
        assert!((sampled[0] as i32 - white[0] as i32).abs() <= 2);
        // sample a white center, should have pupil ink
        let centers = bump_eye_centers(edge, BW, BH);
        let (ex, ey) = centers[0];
        let idx2 = (((ey as u32) * BW + (ex as u32)) * 4) as usize;
        let eye_center = [d[idx2], d[idx2+1], d[idx2+2], d[idx2+3]];
        assert!((eye_center[0] as i32 - EYE_INK[0] as i32).abs() <= 2);
    }

    #[test]
    fn route_green_head_stays_asleep() {
        const BW: u32 = 200;
        const BH: u32 = 120;
        let edge = BumpEdge::Left;
        // plain sleeping (no eyes)
        let mut plain = Pixmap::new(BW, BH).unwrap();
        draw_bump(&mut plain, edge, BW, BH, [180, 100, 60]);
        let plain_buf = plain.data().to_vec();
        // simulate gate with alert=None (route ignored for eyes)
        let mut route = Pixmap::new(BW, BH).unwrap();
        draw_bump(&mut route, edge, BW, BH, [180, 100, 60]);
        if bump_eyes_awake(false) {
            draw_bump_eyes_awake(&mut route, edge, BW, BH, 0.0);
        }
        assert_eq!(route.data().to_vec(), plain_buf, "route green must leave head asleep");
        // activity does wake
        let mut active = Pixmap::new(BW, BH).unwrap();
        draw_bump(&mut active, edge, BW, BH, [180, 100, 60]);
        if bump_eyes_awake(true) {
            draw_bump_eyes_awake(&mut active, edge, BW, BH, 0.0);
        }
        assert_ne!(active.data().to_vec(), plain_buf, "activity green must wake the head");
    }

    #[test]
    fn surface_bloom_blooms_two_columns_outside_the_torso() {
        let layout = Layout::initial();
        let torso = layout.torso_rect();
        let center_y = torso.y + torso.h / 2.0;

        // Ten items split five-a-side into two columns flanking the torso.
        let rects = layout.surface_bloom_rects(10);
        assert_eq!(rects.len(), 10);
        let (left, right) = rects.split_at(5);

        // The left column sits entirely left of the torso; the right column entirely right of it —
        // so the torso (and its interior list) stays readable while the dial is active.
        for r in left {
            assert!(r.x + r.w <= torso.x, "left column must render outside (left of) the torso");
        }
        for r in right {
            assert!(r.x >= torso.x + torso.w, "right column must render outside (right of) the torso");
        }

        // Each column is evenly spaced (constant pitch) and vertically centered on the torso.
        for col in [left, right] {
            let pitch = col[1].y - col[0].y;
            assert!((pitch - (SURFACE_BLOOM_H + SURFACE_BLOOM_GAP)).abs() < 0.01, "pills evenly spaced");
            for win in col.windows(2) {
                assert!((win[1].y - win[0].y - pitch).abs() < 0.01, "even spacing down the column");
            }
            let col_mid = (col.first().unwrap().y + col.last().unwrap().y + SURFACE_BLOOM_H) / 2.0;
            assert!((col_mid - center_y).abs() < 0.5, "column centered on the torso midline");
        }
    }

    #[test]
    fn surface_bloom_balances_an_odd_count_across_the_two_columns() {
        let layout = Layout::initial();
        // Five items → three on the left (ceil), two on the right (floor).
        let rects = layout.surface_bloom_rects(5);
        assert_eq!(rects.len(), 5);
        let torso = layout.torso_rect();
        let left = rects.iter().filter(|r| r.x + r.w <= torso.x).count();
        let right = rects.iter().filter(|r| r.x >= torso.x + torso.w).count();
        assert_eq!((left, right), (3, 2));
    }

    #[test]
    fn surface_bloom_hit_maps_each_pill_to_its_index() {
        let layout = Layout::initial();
        // Exercise the full two-column spread.
        let rects = layout.surface_bloom_rects(10);
        for (idx, rect) in rects.iter().enumerate() {
            let x = (rect.x + rect.w / 2.0) as f64;
            let y = (rect.y + rect.h / 2.0) as f64;
            assert_eq!(surface_bloom_hit(&layout, 10, x, y), Some(idx));
        }
        assert_eq!(surface_bloom_hit(&layout, 10, 1.0, 1.0), None);
    }

    #[test]
    fn output_panel_lives_inside_stretchable_torso() {
        let short = Layout::new(Facing::Right, BODY_LEN_MIN, BUBBLE_W_DEFAULT);
        let tall = Layout::new(Facing::Right, BODY_LEN_MAX, BUBBLE_W_DEFAULT);
        let torso = tall.torso_rect();
        let panel = tall.output_panel_rect();

        assert!(panel.x > torso.x);
        assert!(panel.y > torso.y);
        assert!(panel.x + panel.w < torso.x + torso.w);
        assert!(panel.y + panel.h < torso.y + torso.h);
        assert!(tall.output_panel_rect().h > short.output_panel_rect().h);
    }

    #[test]
    fn torso_scroll_cycles_out_of_receipt_view() {
        let budget = 3usize;
        let state = TorsoScrollState {
            at_max_stretch: true,
            interior_open: false,
            show_receipt_ledger: true,
            receipt_scroll: 0,
            receipt_count: 2,
            receipt_budget: budget,
        };
        let out = advance_torso_scroll(state);
        assert!(!out.show_receipt_ledger, "single-page receipt ledger must yield to output on scroll");
        assert!(!out.interior_open);
        assert_eq!(out.receipt_scroll, 0);
    }

    #[test]
    fn torso_scroll_pages_receipts_before_cycling_views() {
        let budget = 2usize;
        let mut state = TorsoScrollState {
            at_max_stretch: true,
            interior_open: false,
            show_receipt_ledger: true,
            receipt_scroll: 0,
            receipt_count: 5,
            receipt_budget: budget,
        };
        let max_scroll = state.receipt_count.saturating_sub(budget.max(1));
        while state.receipt_scroll < max_scroll {
            let next = advance_torso_scroll(state);
            assert!(next.show_receipt_ledger);
            assert_eq!(next.receipt_scroll, state.receipt_scroll + 1);
            state.receipt_scroll = next.receipt_scroll;
        }
        let exit = advance_torso_scroll(state);
        assert!(!exit.show_receipt_ledger);
    }

    #[test]
    fn torso_scroll_toggles_interior_below_max_stretch() {
        let idle = TorsoScrollState {
            at_max_stretch: false,
            interior_open: false,
            show_receipt_ledger: false,
            receipt_scroll: 0,
            receipt_count: 0,
            receipt_budget: 1,
        };
        let open = advance_torso_scroll(idle);
        assert!(open.interior_open);
        let closed = advance_torso_scroll(TorsoScrollState {
            interior_open: open.interior_open,
            ..idle
        });
        assert!(!closed.interior_open);
    }

    #[test]
    fn interior_view_draws_torso_scroll_actions() {
        let font = load_font().expect("font");
        let layout = Layout::new(Facing::Right, BODY_LEN_DEFAULT, BUBBLE_W_DEFAULT);
        let rows = [InteriorRow {
            id: PerimeterId::Quick0,
            glyph: "1",
            text: "chat",
            dim: false,
        }];
        let mut pixmap = Pixmap::new(SURFACE_W, layout.surface_h()).unwrap();
        draw_interior_view(&mut pixmap, &font, &layout, &rows, CLAY_DEFAULT);
        let scroll = layout.torso_action_rect(TorsoAction::Scroll);
        let cx = (scroll.x + scroll.w / 2.0) as u32;
        let cy = (scroll.y + scroll.h / 2.0) as u32;
        assert!(
            pixmap.pixel(cx, cy).is_some_and(|p| p.alpha() > 0),
            "scroll affordance must paint on the interior view",
        );
    }

    #[test]
    fn torso_actions_live_inside_output_panel() {
        let layout = Layout::new(Facing::Right, BODY_LEN_DEFAULT, BUBBLE_W_DEFAULT);
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
    fn bloom_launcher_pill_is_visually_distinct_from_surface_pill() {
        // A launcher pill must read differently from a surface-switch pill — the reach metaphor
        // (open an external tool) is a different action than switching the active surface, so
        // identical rendering would be a trust-legibility bug. We assert it two ways: the
        // launcher pill's background tint differs, and the `→` glyph adds pixels a surface pill
        // never draws.
        let Some(font) = load_font() else { return };
        let layout = Layout::initial();
        // Draw each pill alone into its own pixmap — with one item, both land at rects[0], so
        // we sample the same slot for a clean apples-to-apples comparison.
        let rects = layout.surface_bloom_rects(1);
        let pill_rect = rects[0];

        let surface_item = SurfaceDialItem { label: "Session", availability: "available", active: false, kind: "surface" };
        let launcher_item = SurfaceDialItem { label: "Open in Cursor", availability: "gated", active: false, kind: "launcher" };

        let mut surface_px = Pixmap::new(SURFACE_W, layout.surface_h()).unwrap();
        draw_surface_bloom(&mut surface_px, &font, &layout, &[surface_item]);
        let mut launcher_px = Pixmap::new(SURFACE_W, layout.surface_h()).unwrap();
        draw_surface_bloom(&mut launcher_px, &font, &layout, &[launcher_item]);

        // Sample the background tint along the pill's top edge (y = pill top + 2) — text sits
        // on a baseline lower in the pill, so this row reads the fill, not glyphs or label.
        let cx = (pill_rect.x + pill_rect.w / 2.0) as i32;
        let cy = (pill_rect.y + 2.0) as i32;
        let s = sample_argb(&surface_px, cx, cy);
        let l = sample_argb(&launcher_px, cx, cy);
        assert!(s != l, "launcher pill background ({:?}) must differ from surface pill ({:?})", l, s);
        // The launcher wash is warm (R > B); the surface wash is cool (B >= R).
        assert!(l.0 > l.2, "launcher pill should be warm-tinted (r>b), got {:?}", l);
        assert!(s.2 >= s.0, "surface pill should be cool-tinted (b>=r), got {:?}", s);

        // The `→` glyph: the launcher pill has dark glyph pixels near its left that the surface
        // pill (label-only, centered) does not. Count pixels in the leftmost band that are
        // significantly darker than the pill fill — those are glyph strokes, not background.
        let lp = count_dark_in_rect(&launcher_px, (pill_rect.x + 6.0) as i32, (pill_rect.y + 3.0) as i32, 22, 18);
        let sp = count_dark_in_rect(&surface_px, (pill_rect.x + 6.0) as i32, (pill_rect.y + 3.0) as i32, 22, 18);
        assert!(lp > sp, "launcher pill should draw a leading glyph the surface pill lacks ({} vs {} dark px)", lp, sp);
    }

    #[test]
    fn unwired_bloom_pill_renders_label_and_soon_tag_not_blank() {
        // The regression guard for the empty-grey-slot bug: an unwired pill must draw its label
        // AND a right-aligned "soon" tag, so it never reads as a blank pill. The unwired ink is
        // muted slate (sum ≈ 306–390), well above count_dark's near-black threshold but clearly
        // darker than the opaque pill fill (sum ≈ 707), so count pixels below a mid threshold.
        let Some(font) = load_font() else { return };
        let layout = Layout::initial();
        let rect = layout.surface_bloom_rects(1)[0];

        let item = SurfaceDialItem { label: "Live Hermes", availability: "unwired", active: false, kind: "surface" };
        let mut px = Pixmap::new(SURFACE_W, layout.surface_h()).unwrap();
        draw_surface_bloom(&mut px, &font, &layout, &[item]);

        let count_ink = |x0: i32, w: i32| -> usize {
            let pw = px.width() as i32;
            let data = px.data();
            let mut n = 0;
            for dy in 0..18 {
                for dx in 0..w {
                    let (x, y) = (x0 + dx, rect.y as i32 + 3 + dy);
                    if x < 0 || y < 0 || x >= pw || y >= px.height() as i32 {
                        continue;
                    }
                    let i = ((y * pw + x) * 4) as usize;
                    if data[i + 3] > 80
                        && (data[i] as i32 + data[i + 1] as i32 + data[i + 2] as i32) < 560
                    {
                        n += 1;
                    }
                }
            }
            n
        };
        // Label ink on the left of the pill.
        let label_ink = count_ink(rect.x as i32 + 4, 60);
        assert!(label_ink > 0, "unwired pill must draw its label, not a blank slot ({label_ink} ink px)");
        // "soon" tag ink in the right band of the pill.
        let tag_ink = count_ink((rect.x + rect.w) as i32 - 34, 30);
        assert!(tag_ink > 0, "unwired pill must draw a 'soon' tag ({tag_ink} ink px)");
    }

    fn sample_argb(p: &Pixmap, x: i32, y: i32) -> (u8, u8, u8, u8) {
        let w = p.width() as i32;
        let data = p.data();
        let i = ((y * w + x) * 4) as usize;
        (data[i], data[i + 1], data[i + 2], data[i + 3])
    }

    /// Count pixels darker than the pill fill — glyph strokes (the `→` and the label) are dark
    /// ink, so this isolates text/glyph pixels from the warm/cool background wash.
    fn count_dark_in_rect(p: &Pixmap, x0: i32, y0: i32, w: i32, h: i32) -> usize {
        let pw = p.width() as i32;
        let ph = p.height() as i32;
        let data = p.data();
        let mut n = 0;
        for dy in 0..h {
            for dx in 0..w {
                let x = x0 + dx;
                let y = y0 + dy;
                if x < 0 || y < 0 || x >= pw || y >= ph {
                    continue;
                }
                let i = ((y * pw + x) * 4) as usize;
                let a = data[i + 3] as i32;
                // Only count where ink has actually landed (alpha) AND the channel values are
                // dark — the pill fills are bright (≥200), glyph strokes drop well below 90.
                if a > 80 && (data[i] as i32 + data[i + 1] as i32 + data[i + 2] as i32) < 270 {
                    n += 1;
                }
            }
        }
        n
    }

    #[test]
    fn passport_rows_stay_within_the_142px_torso() {
        // The whole point of the passport: overflowing persona/provider/preview must NOT spill
        // past the torso column the way the old six-field SessionCard did.
        let font = load_font().expect("system font available for passport layout test");
        let layout = Layout::new(Facing::Right, BODY_LEN_MIN, BUBBLE_W_DEFAULT);
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
            activity: false,
            pills: &[],
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

    fn sample_passport_card(card: &PassportCard) -> (Pixmap, Rect, Font) {
        let font = load_font().expect("system font available for passport test");
        let layout = Layout::new(Facing::Right, BODY_LEN_DEFAULT, BUBBLE_W_DEFAULT);
        let content = inset_rect(layout.output_panel_rect(), 5.0, 5.0);
        let mut pixmap = Pixmap::new(SURFACE_W, layout.surface_h()).unwrap();
        draw_passport_card(&mut pixmap, &font, content, card);
        (pixmap, content, font)
    }

    fn route_health_dot_center(content: Rect, font: &Font, card: &PassportCard) -> (i32, i32) {
        let x = content.x + 8.0;
        let row1 = passport_route_baseline(content);
        let mut trail_x = if let Some(provider) = card.provider {
            let prov = fit_line(font, provider, 10.0, content.w - 32.0);
            x + measure(font, &prov, 10.0)
        } else {
            x + measure(font, "No route yet", 10.0)
        };
        if card.locality.is_some() {
            trail_x += 13.0;
        }
        let cx = (trail_x + 3.0).round() as i32;
        let cy = (row1 - 3.0).round() as i32;
        (cx, cy)
    }

    #[test]
    fn connection_card_health_dot_matches_route_hue() {
        for health in ["ready", "degraded", "unavailable"] {
            let expected = route_health_ring_rgba(health).expect("closed health set");
            let card = PassportCard {
                persona_label: "Forge",
                posture: "work",
                provider: Some("local-ollama"),
                locality: Some("local"),
                route_health: Some(health),
                activity: false,
                pills: &[],
                output_preview: Some("idle"),
            };
            let (pixmap, content, font) = sample_passport_card(&card);
            let (cx, cy) = route_health_dot_center(content, &font, &card);
            let p = pixmap.pixel(cx as u32, cy as u32).expect("dot centre in bounds");
            let sampled = demultiply_rgba([p.red(), p.green(), p.blue(), p.alpha()]);
            assert!(
                rgba_close(sampled, expected, 2),
                "health {health}: dot {sampled:?} != palette {expected:?}",
            );
        }
    }

    #[test]
    fn connection_card_no_route_reads_honest() {
        let base = PassportCard {
            persona_label: "Forge",
            posture: "work",
            provider: None,
            locality: None,
            route_health: None,
            activity: false,
            pills: &[],
            output_preview: Some("idle note"),
        };
        let with_route = PassportCard {
            provider: Some("ollama"),
            ..base
        };
        let (none_px, _, _) = sample_passport_card(&base);
        let (some_px, _, _) = sample_passport_card(&with_route);
        assert_ne!(none_px.data(), some_px.data(), "None vs Some provider must paint differently");
        let row_y = passport_route_baseline(inset_rect(
            Layout::new(Facing::Right, BODY_LEN_DEFAULT, BUBBLE_W_DEFAULT).output_panel_rect(),
            5.0,
            5.0,
        )) as i32;
        let font = load_font().expect("font");
        let text_x = (inset_rect(
            Layout::new(Facing::Right, BODY_LEN_DEFAULT, BUBBLE_W_DEFAULT).output_panel_rect(),
            5.0,
            5.0,
        )
        .x
            + 8.0) as i32;
        let mut saw_ink = false;
        for dx in 0..80 {
            let idx = ((row_y * none_px.width() as i32 + text_x + dx) * 4) as usize;
            if none_px.data()[idx + 3] > 0 {
                saw_ink = true;
                break;
            }
        }
        assert!(saw_ink, "No route yet row must not be blank");
        let no_route_w = measure(&font, "No route yet", 10.0);
        assert!(no_route_w > 20.0);
    }

    #[test]
    fn connection_status_line_brackets_activity() {
        assert_eq!(
            connection_status_line(true, Some("ollama"), Some("note")),
            "Working — talking to ollama…",
        );
        assert_eq!(connection_status_line(true, None, Some("note")), "Working…");
        assert_eq!(connection_status_line(false, Some("ollama"), Some("note")), "note");
        assert_eq!(
            connection_status_line(false, None, None),
            "Idle — text, image, and file output land here.",
        );
    }

    #[test]
    fn connection_card_activity_swaps_preview_for_working() {
        let idle = PassportCard {
            persona_label: "Forge",
            posture: "work",
            provider: Some("ollama"),
            locality: None,
            route_health: None,
            activity: false,
            pills: &[],
            output_preview: Some("Session note preview"),
        };
        let working = PassportCard {
            activity: true,
            ..idle
        };
        let (idle_px, _, _) = sample_passport_card(&idle);
        let (work_px, _, _) = sample_passport_card(&working);
        assert_ne!(idle_px.data(), work_px.data(), "activity must swap the status line");
    }

    #[test]
    fn connection_card_fits_torso_rect() {
        let font = load_font().expect("font");
        let layout = Layout::new(Facing::Right, BODY_LEN_MIN, BUBBLE_W_DEFAULT);
        let panel = layout.output_panel_rect();
        let content = inset_rect(panel, 5.0, 5.0);
        let pills = [
            SurfacePill { label: "chat", active: true, wired: true },
            SurfacePill { label: "code", active: false, wired: true },
        ];
        let card = PassportCard {
            persona_label: "A Long Persona Name For Min Stretch",
            posture: "private",
            provider: Some("provider"),
            locality: Some("cloud"),
            route_health: Some("ready"),
            activity: false,
            pills: &pills,
            output_preview: Some("Preview line"),
        };
        let mut pixmap = Pixmap::new(SURFACE_W, layout.surface_h()).unwrap();
        draw_passport_card(&mut pixmap, &font, content, &card);
        // Only the card paints on this pixmap, so ANY ink outside the content rect is
        // overflow — scan the whole surface (a loop bounded at `bottom` can never see
        // the vertical overflow it claims to reject).
        let left = content.x.floor() as i32 - 1;
        let right = (content.x + content.w).ceil() as i32 + 1;
        let top = content.y.floor() as i32 - 1;
        let bottom = (content.y + content.h).ceil() as i32 + 1;
        let data = pixmap.data();
        let w = pixmap.width() as i32;
        let h = pixmap.height() as i32;
        let mut saw_ink = false;
        for y in 0..h {
            for x in 0..w {
                let idx = ((y * w + x) * 4) as usize;
                if data[idx + 3] > 0 {
                    saw_ink = true;
                    assert!(x >= left && x <= right, "ink at x={x} outside panel [{left},{right}]");
                    assert!(y >= top && y <= bottom, "ink at y={y} outside panel [{top},{bottom}]");
                }
            }
        }
        assert!(saw_ink, "card must actually paint at min stretch");
    }

    #[test]
    fn pill_row_registered_in_input_region() {
        let font = load_font().expect("font");
        let layout = Layout::new(Facing::Right, BODY_LEN_DEFAULT, BUBBLE_W_DEFAULT);
        let content = inset_rect(layout.output_panel_rect(), 5.0, 5.0);
        let labels = ["chat", "code"];
        let rects = passport_pill_hit_rects(&font, content, &labels);
        assert!(!rects.is_empty(), "pill row must produce hit rects for input-region registration");
        let panel = layout.output_panel_rect();
        for rect in &rects {
            assert!(rect.x >= panel.x);
            assert!(rect.y >= panel.y);
            assert!(rect.x + rect.w <= panel.x + panel.w + 1.0);
            assert!(rect.y + rect.h <= panel.y + panel.h + 1.0);
        }
    }

    #[test]
    fn surface_pills_single_source_paint_and_hit() {
        let font = load_font().expect("font");
        let layout = Layout::new(Facing::Right, BODY_LEN_DEFAULT, BUBBLE_W_DEFAULT);
        let content = inset_rect(layout.output_panel_rect(), 5.0, 5.0);
        let labels = ["chat", "code"];
        let rects = passport_pill_hit_rects(&font, content, &labels);
        assert_eq!(rects.len(), 2);
        for (i, rect) in rects.iter().enumerate() {
            let cx = rect.x + rect.w / 2.0;
            let cy = rect.y + rect.h / 2.0;
            assert_eq!(passport_pill_hit(&font, content, &labels, cx as f64, cy as f64), Some(i));
        }
    }

    fn passport_fixture<'a>(pills: &'a [SurfacePill<'a>]) -> PassportCard<'a> {
        PassportCard {
            persona_label: "F",
            posture: "work",
            provider: Some("p"),
            locality: None,
            route_health: None,
            activity: false,
            pills,
            output_preview: None,
        }
    }

    #[test]
    fn unwired_pill_dims_not_hides() {
        let font = load_font().expect("font");
        let layout = Layout::new(Facing::Right, BODY_LEN_DEFAULT, BUBBLE_W_DEFAULT);
        let content = inset_rect(layout.output_panel_rect(), 5.0, 5.0);
        // Same two labels, only the wired flag flips — the pixel diff can only come
        // from the dim styling, not from a different pill count.
        let all_wired = [
            SurfacePill { label: "chat", active: false, wired: true },
            SurfacePill { label: "soon", active: false, wired: true },
        ];
        let one_unwired = [
            SurfacePill { label: "chat", active: false, wired: true },
            SurfacePill { label: "soon", active: false, wired: false },
        ];
        let (wired_px, _, _) = sample_passport_card(&passport_fixture(&all_wired));
        let (dimmed_px, _, _) = sample_passport_card(&passport_fixture(&one_unwired));
        assert_ne!(wired_px.data(), dimmed_px.data(), "dim styling must change pixels");
        // Not hidden: the unwired pill still lays out AND its rect carries ink.
        let rects = passport_pill_hit_rects(&font, content, &["chat", "soon"]);
        assert_eq!(rects.len(), 2, "both pills painted");
        let unwired_rect = &rects[1];
        let mut ink = false;
        for dy in 0..unwired_rect.h as i32 {
            for dx in 0..unwired_rect.w as i32 {
                let x = (unwired_rect.x as i32 + dx) as u32;
                let y = (unwired_rect.y as i32 + dy) as u32;
                if dimmed_px.pixel(x, y).is_some_and(|p| p.alpha() > 0) {
                    ink = true;
                }
            }
        }
        assert!(ink, "unwired pill must still paint (dim, not hidden)");
    }

    #[test]
    fn active_pill_follows_surface_active() {
        let pills_a = [SurfacePill { label: "chat", active: true, wired: true }];
        let pills_b = [SurfacePill { label: "chat", active: false, wired: true }];
        let (px_a, _, _) = sample_passport_card(&passport_fixture(&pills_a));
        let (px_b, _, _) = sample_passport_card(&passport_fixture(&pills_b));
        assert_ne!(px_a.data(), px_b.data(), "active pill highlight must change pixels");
    }

    #[test]
    fn tucked_peek_chip_mirrors_route_truth() {
        let font = load_font().expect("font");
        let edge = BumpEdge::Right;
        let w = SURFACE_W;
        let h = 200u32;
        let mut pixmap = Pixmap::new(w, h).unwrap();
        draw_tucked_bubble(
            &mut pixmap,
            &font,
            edge,
            w,
            h,
            "Hello",
            Some("ollama"),
            Some("ready"),
        );
        let rect = tucked_bubble_rect(edge, w, h);
        let chip_cx = (rect.x + 14.0) as u32;
        let chip_cy = (rect.y + 14.0) as u32;
        let p = pixmap.pixel(chip_cx, chip_cy).expect("chip ink");
        assert!(p.alpha() > 0, "provider label must paint");
        let expected = route_health_ring_rgba("ready").unwrap();
        let dot_cx = (rect.x + 12.0 + measure(&font, "ollama", 8.5) + 9.0) as u32;
        let dot_cy = (rect.y + 14.0) as u32;
        let dot = pixmap.pixel(dot_cx, dot_cy).expect("health dot");
        let sampled = demultiply_rgba([dot.red(), dot.green(), dot.blue(), dot.alpha()]);
        assert!(rgba_close(sampled, expected, 4), "chip dot {sampled:?} != {expected:?}");
    }

    #[test]
    fn tucked_bar_language_untouched() {
        let layout = Layout::initial();
        let w = SURFACE_W;
        let h = layout.surface_h();
        let sprite = Sprite::new();
        let edge = BumpEdge::Left;
        let paint = |route_health: Option<&str>| -> Vec<u8> {
            let mut canvas = vec![0_u8; (w * h * 4) as usize];
            let card = PassportCard {
                persona_label: "F",
                posture: "work",
                provider: Some("p"),
                locality: Some("local"),
                route_health,
                activity: false,
                pills: &[],
                output_preview: None,
            };
            let view = BodyView {
                t: 0.0,
                emotion: Emotion::Neutral,
                speech: Some("hi"),
                torso_output: TorsoOutput::Passport(card),
                chat_open: false,
                tucked: Some(edge),
                tucked_show_bubble: true,
                tucked_show_input: false,
                input_text: "",
                input_placeholder: "",
                input_focused: false,
                review_pending: false,
                edit_pending: false,
                posture_badge: None,
                surface_bloom: &[],
                route_health,
                route_flash: false,
                alert_level: None,
                activity: false,
                receipt_rail: &[],
                receipt_scroll: 0,
                show_receipt_ledger: false,
                interior_rows: &[],
                settings: &[],
                onboarding: None,
                layout,
                pinned: None,
                frame: None,
                color: CLAY_DEFAULT,
                skin: Skin::Clay,
                dock_show: DockShow::Bar,
                reader: None,
                reader_scroll: 0,
                reader_copied: false,
                reader_selection: None,
            };
            sprite.paint(&mut canvas, w, h, &view);
            canvas
        };
        // Sweep the FULL bar rect (body AND tip regions) — route health must not move
        // a single bar pixel; only the peek-bubble chip may react to it.
        let along = tuck_bar_along_length(w, h);
        let bar = bar_rect(edge, w, h, along);
        let ready = paint(Some("ready"));
        let degraded = paint(Some("degraded"));
        for y in bar.y as u32..(bar.y + bar.h) as u32 {
            for x in bar.x as u32..(bar.x + bar.w) as u32 {
                let idx = ((y * w + x) * 4) as usize;
                assert_eq!(
                    &ready[idx..idx + 4],
                    &degraded[idx..idx + 4],
                    "bar pixel ({x},{y}) must not change with route health",
                );
            }
        }
    }

    #[test]
    fn pills_hit_correctly_in_both_dock() {
        let font = load_font().expect("font");
        for facing in [Facing::Right, Facing::Left] {
            let layout = Layout::new(facing, BODY_LEN_DEFAULT, BUBBLE_W_DEFAULT);
            let content = inset_rect(layout.output_panel_rect(), 5.0, 5.0);
            let labels = ["alpha", "beta"];
            let rects = passport_pill_hit_rects(&font, content, &labels);
            for (i, rect) in rects.iter().enumerate() {
                let hit = passport_pill_hit(
                    &font,
                    content,
                    &labels,
                    (rect.x + 2.0) as f64,
                    (rect.y + 2.0) as f64,
                );
                assert_eq!(hit, Some(i), "facing {facing:?} pill {i}");
            }
        }
    }

    const EIGHT_LABELS: [&str; 8] = ["one", "two", "three", "four", "five", "six", "seven", "eight"];

    #[test]
    fn min_stretch_hides_pill_row_entirely() {
        // A min-stretch torso has no vertical room for the pill row: the guarded
        // geometry returns no rects (no invisible click targets), and paint —
        // consuming the same rects — draws no pill ink either.
        let font = load_font().expect("font");
        let layout = Layout::new(Facing::Right, BODY_LEN_MIN, BUBBLE_W_DEFAULT);
        let content = inset_rect(layout.output_panel_rect(), 5.0, 5.0);
        assert!(
            passport_pill_hit_rects(&font, content, &EIGHT_LABELS).is_empty(),
            "no hit rects may exist where no row can paint",
        );
        let pills: Vec<SurfacePill> = EIGHT_LABELS
            .iter()
            .map(|l| SurfacePill { label: l, active: false, wired: true })
            .collect();
        let with_pills = passport_fixture(&pills);
        let without = passport_fixture(&[]);
        let mut px_a = Pixmap::new(SURFACE_W, layout.surface_h()).unwrap();
        let mut px_b = Pixmap::new(SURFACE_W, layout.surface_h()).unwrap();
        draw_passport_card(&mut px_a, &font, content, &with_pills);
        draw_passport_card(&mut px_b, &font, content, &without);
        assert_eq!(px_a.data(), px_b.data(), "skipped pill row must paint nothing");
    }

    #[test]
    fn pill_overflow_collapses_to_marker() {
        // Where the row fits vertically (default stretch) but the labels exceed the
        // card width, the extras collapse to a painted +N marker.
        let font = load_font().expect("font");
        let layout = Layout::new(Facing::Right, BODY_LEN_DEFAULT, BUBBLE_W_DEFAULT);
        let content = inset_rect(layout.output_panel_rect(), 5.0, 5.0);
        let (rel, hidden) = surface_pill_rects(&font, content.w - 16.0, &EIGHT_LABELS);
        assert!(hidden > 0, "eight labels must overflow the card width");
        assert!(rel.len() < EIGHT_LABELS.len());
        assert!(
            !passport_pill_hit_rects(&font, content, &EIGHT_LABELS).is_empty(),
            "row fits vertically at default stretch",
        );
        let pills: Vec<SurfacePill> = EIGHT_LABELS
            .iter()
            .map(|l| SurfacePill { label: l, active: false, wired: true })
            .collect();
        let card = passport_fixture(&pills);
        let mut pixmap = Pixmap::new(SURFACE_W, layout.surface_h()).unwrap();
        draw_passport_card(&mut pixmap, &font, content, &card);
        let (origin_x, origin_y) = passport_pill_row_origin(content);
        let mut saw_plus = false;
        for dx in 0..30 {
            let x = (origin_x + rel.last().map(|r| r.x + r.w + 4.0).unwrap_or(0.0) + dx as f32) as u32;
            let y = (origin_y + 10.0) as u32;
            if pixmap.pixel(x, y).is_some_and(|p| p.alpha() > 0) {
                saw_plus = true;
            }
        }
        assert!(saw_plus, "+N overflow marker must paint");
    }

    #[test]
    fn wrap_ellipsizes_when_line_budget_exhausted() {
        let font = load_font().expect("system font available for wrap test");
        let text = "Edit repository needs a longer explanation than one line allows";
        let lines = wrap(&font, text, TEXT_PX, 188.0, 2);
        assert_eq!(lines.len(), 2);
        let last = lines.last().expect("budgeted wrap should produce lines");
        assert!(last.ends_with('…'), "exhausted budget should ellipsize, got {last:?}");
        assert!(measure(&font, last, TEXT_PX) <= 188.0);
    }

    #[test]
    fn wrap_unlimited_budget_never_ellipsizes() {
        let font = load_font().expect("system font available for wrap test");
        let text = "Edit repository needs a longer explanation than one line allows";
        let lines = wrap(&font, text, TEXT_PX, 188.0, usize::MAX);
        let joined: String = lines.join("");
        assert!(!joined.contains('…'));
        assert!(joined.contains("Edit repository"));
        assert!(joined.contains("allows"));
    }

    #[test]
    fn tucked_bubble_budget_is_three_lines() {
        let pad_top = 18.0;
        let max_lines =
            (((TUCK_PEEK_BUBBLE_H - pad_top - 6.0) / LINE_H).floor() as i32).max(1) as usize;
        assert_eq!(max_lines, 3);
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
    fn tucked_peek_input_sits_below_the_bubble_and_clear_of_the_bump() {
        // Full-figure surface (the size a tuck keeps), bump on the right edge.
        let (w, h) = (SURFACE_W, 320);
        let bubble = tucked_bubble_rect(BumpEdge::Right, w, h);
        let input = tucked_input_rect(BumpEdge::Right, w, h);
        // Input is stacked directly below the bubble, no overlap.
        assert!(input.y >= bubble.y + bubble.h);
        // Both share the same column and stay on the surface.
        assert_eq!(bubble.x, input.x);
        assert!(bubble.x >= 0.0 && bubble.x + bubble.w <= w as f32);
        assert!(input.y + input.h <= h as f32);
        // Clear of the bump: the right-edge bump occupies the rightmost BUMP_R; the peek sits left of it.
        assert!(bubble.x + bubble.w <= w as f32 - BUMP_R);
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
            tucked_show_bubble: false,
            tucked_show_input: false,
            input_text: "",
            input_placeholder: "Ask Border Wizard...",
            input_focused: false,
            review_pending: false,
            edit_pending: false,
            posture_badge: None,
            surface_bloom: &[],
            route_health: None,
            route_flash: false,
            alert_level: None,
            activity: false,
            receipt_rail: &[],
            receipt_scroll: 0,
            show_receipt_ledger: false,
            interior_rows: &[],
            settings: &[],
            onboarding: None,
            layout: Layout::initial(),
            pinned: None,
            frame: Some(frame),
            color: CLAY_DEFAULT,
            dock_show: DockShow::Both,
            skin: Skin::Clay,
            reader: None,
            reader_scroll: 0,
            reader_copied: false,
            reader_selection: None,
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

    #[test]
    fn gaze_rests_centered_when_idle() {
        for t in [0.0, 0.65, 1.0, 1.95, 2.6, 4.0, 7.8] {
            assert_eq!(activity_gaze_dx(false, t), 0.0, "idle gaze must stay centered at t={t}");
        }
    }

    #[test]
    fn gaze_sweep_stays_inside_the_eye_white() {
        assert!(
            2.0 + GAZE_SWEEP_DX + 4.5 < 11.0,
            "pupil ink must stay inside the eye white at max sweep",
        );
        for t in [0.0, 0.65, 1.3, 1.95, 2.6, 3.25, 4.0, 5.2, 7.8] {
            let dx = activity_gaze_dx(true, t);
            assert!(
                dx.abs() <= GAZE_SWEEP_DX + 0.001,
                "sweep amplitude must not exceed GAZE_SWEEP_DX at t={t}",
            );
        }
    }

    fn untucked_gaze_sample_pixel(activity: bool, alert_level: Option<AlertLevel>, t: f32) -> [u8; 4] {
        let layout = Layout::initial();
        let w = SURFACE_W;
        let h = layout.surface_h();
        let mut canvas = vec![0_u8; (w * h * 4) as usize];
        let view = BodyView {
            t,
            emotion: Emotion::Neutral,
            speech: None,
            torso_output: TorsoOutput::Text(TextCard { title: "", body: "" }),
            chat_open: false,
            tucked: None,
            tucked_show_bubble: false,
            tucked_show_input: false,
            input_text: "",
            input_placeholder: "",
            input_focused: false,
            review_pending: false,
            edit_pending: false,
            posture_badge: None,
            surface_bloom: &[],
            route_health: None,
            route_flash: false,
            alert_level,
            activity,
            receipt_rail: &[],
            receipt_scroll: 0,
            show_receipt_ledger: false,
            interior_rows: &[],
            settings: &[],
            onboarding: None,
            layout,
            pinned: None,
            frame: None,
            color: CLAY_DEFAULT,
            dock_show: DockShow::Both,
            skin: Skin::Clay,
            reader: None,
            reader_scroll: 0,
            reader_copied: false,
            reader_selection: None,
        };
        Sprite::new().paint(&mut canvas, w, h, &view);
        let bob = (t * std::f32::consts::TAU / 3.6).sin() * 3.0;
        let ex = FIG_CX + 18.0;
        let ey = HEAD_CY - 10.0 + bob;
        let qx = (ex + 2.0 + 6.0) as u32;
        let qy = ey as u32;
        let idx = ((qy * w + qx) * 4) as usize;
        [canvas[idx], canvas[idx + 1], canvas[idx + 2], canvas[idx + 3]]
    }

    const EYE_INK_BGRA: [u8; 4] = [EYE_INK[2], EYE_INK[1], EYE_INK[0], EYE_INK[3]];

    #[test]
    fn activity_sweeps_the_untucked_pupils() {
        const T: f32 = 0.65;
        let idle = untucked_gaze_sample_pixel(false, None, T);
        assert_ne!(idle, EYE_INK_BGRA, "idle gaze: sample point must sit on eye white, not pupil ink");
        let active = untucked_gaze_sample_pixel(true, None, T);
        assert_eq!(active, EYE_INK_BGRA, "activity gaze: sample point must be solid pupil ink");
        let cleared = untucked_gaze_sample_pixel(false, None, T);
        assert_ne!(cleared, EYE_INK_BGRA, "result side: gaze must rest centered again");
    }

    #[test]
    fn soul_ready_tier_never_moves_the_gaze() {
        const T: f32 = 0.65;
        assert_eq!(activity_gaze_dx(false, T), 0.0);
        let sample = untucked_gaze_sample_pixel(false, Some(AlertLevel::Ready), T);
        assert_ne!(
            sample,
            EYE_INK_BGRA,
            "a soul-emitted Ready tier must not move the untucked gaze",
        );
    }

    #[test]
    fn bubble_budget_never_crosses_input_top() {
        assert_eq!(bubble_line_budget(), 7);
        let layout = Layout::initial();
        let bubble = layout.bubble_rect();
        assert!(bubble.y + bubble.h <= INPUT_Y);
    }

    #[test]
    fn bubble_overflow_draws_more_marker() {
        let font = load_font().expect("system font available for bubble test");
        let long = (0..80).map(|i| format!("word{i}")).collect::<Vec<_>>().join(" ");
        let (lines, hidden) = budgeted_lines(&font, &long, TEXT_PX, BUBBLE_W - 28.0, bubble_line_budget());
        assert!(hidden > 0);
        assert_eq!(lines.len(), bubble_line_budget());
        assert!(lines.last().unwrap().starts_with('+'));
        assert!(lines.last().unwrap().ends_with(" more"));
    }

    #[test]
    fn bubble_marker_absent_when_text_fits() {
        let font = load_font().expect("system font available for bubble test");
        let (lines, hidden) = budgeted_lines(&font, "Short reply.", TEXT_PX, BUBBLE_W - 28.0, bubble_line_budget());
        assert_eq!(hidden, 0);
        assert!(!lines.iter().any(|line| line.contains(" more")));
    }

    #[test]
    fn reader_takeover_suppresses_figure() {
        let layout = Layout::initial();
        let w = SURFACE_W;
        let h = 400_u32;
        let sprite = Sprite::new();
        let text = "A long reply the reader shows in full.";
        let paint = |reader: Option<&str>| -> Vec<u8> {
            let mut canvas = vec![0_u8; (w * h * 4) as usize];
            let view = BodyView {
                t: 0.0,
                emotion: Emotion::Neutral,
                speech: Some(text),
                torso_output: TorsoOutput::Text(TextCard { title: "", body: "" }),
                chat_open: false,
                tucked: None,
                tucked_show_bubble: false,
                tucked_show_input: false,
                input_text: "",
                input_placeholder: "",
                input_focused: false,
                review_pending: false,
                edit_pending: false,
                posture_badge: None,
                surface_bloom: &[],
                route_health: None,
                route_flash: false,
                alert_level: None,
                activity: false,
                receipt_rail: &[],
                receipt_scroll: 0,
                show_receipt_ledger: false,
                interior_rows: &[],
                settings: &[],
                onboarding: None,
                layout,
                pinned: None,
                frame: None,
                color: CLAY_DEFAULT,
                dock_show: DockShow::Both,
                skin: Skin::Clay,
                reader,
                reader_scroll: 0,
                reader_copied: false,
                reader_selection: None,
            };
            sprite.paint(&mut canvas, w, h, &view);
            canvas
        };
        let body = paint(None);
        let reader = paint(Some(text));
        assert_ne!(body, reader, "reader open must paint a different surface than the figure");
        let card = reader_card_rect(w as f32, h);
        let sample_x = (card.x + card.w * 0.5) as u32;
        let sample_y = (card.y + 40.0) as u32;
        let card_idx = ((sample_y * w + sample_x) * 4) as usize;
        assert_ne!(
            &body[card_idx..card_idx + 3],
            &reader[card_idx..card_idx + 3],
            "reader takeover must replace pixels inside the reader card",
        );
        assert!(
            reader[card_idx] > 200 && reader[card_idx + 1] > 240,
            "reader card must paint bubble background, not clay",
        );
    }

    #[test]
    fn reader_card_spans_surface() {
        let w = SURFACE_W;
        let h = 720_u32;
        let card = reader_card_rect(w as f32, h);
        assert!((card.w - (w as f32 - 16.0)).abs() < 0.01);
        assert!((card.h - (h as f32 - 16.0)).abs() < 0.01);
    }

    #[test]
    fn reader_drag_squashes_against_right_edge() {
        let pref = 720.0;
        let sw = 1920.0;
        let (left, w) = reader_drag_layout(500.0, pref, pref, 100.0, sw);
        assert!((left - 600.0).abs() < 0.5);
        assert!((w - pref).abs() < 0.5);
        let (left, w) = reader_drag_layout(1200.0, pref, pref, 400.0, sw);
        assert!((left - 1600.0).abs() < 0.5);
        assert!((w - (sw - 1600.0)).abs() < 0.5);
        assert!(left + w <= sw + 0.5);
    }

    #[test]
    fn reader_drag_squashes_against_left_edge() {
        let pref = 720.0;
        let sw = 1920.0;
        let (left, w) = reader_drag_layout(0.0, pref, pref, -40.0, sw);
        assert_eq!(left, 0.0);
        assert!((w - (pref - 40.0)).abs() < 0.5);

        let (left, w) = reader_drag_layout(100.0, pref, pref, -250.0, sw);
        assert_eq!(left, 0.0);
        assert!((w - (pref - 150.0)).abs() < 0.5);
    }

    #[test]
    fn reader_drag_unsquashes_when_pulling_back_from_either_edge() {
        let pref = 720.0;
        let sw = 1920.0;
        let (left, w) = reader_drag_layout(0.0, 680.0, pref, 60.0, sw);
        assert!((left - 60.0).abs() < 0.5);
        assert!((w - pref).abs() < 0.5);

        let (left, w) = reader_drag_layout(1500.0, sw - 1500.0, pref, 0.0, sw);
        assert!((w - (sw - 1500.0)).abs() < 0.5);
        let (left, w) = reader_drag_layout(left, w, pref, -200.0, sw);
        assert!((left - 1300.0).abs() < 0.5);
        assert!(w > sw - 1500.0);
    }

    #[test]
    fn reader_drag_never_uses_negative_left_margin() {
        let (left, _) = reader_drag_layout(-500.0, 720.0, 720.0, 0.0, 1920.0);
        assert_eq!(left, 0.0);
    }

    #[test]
    fn reader_move_drag_strip_is_top_of_card() {
        let w = SURFACE_W;
        let h = 720_u32;
        let card = reader_card_rect(w as f32, h);
        let strip = reader_move_drag_rect(w as f32, h);
        assert_eq!(strip.x, card.x);
        assert_eq!(strip.y, card.y);
        assert_eq!(strip.w, card.w);
        assert!((strip.h - READER_MOVE_DRAG_H).abs() < 0.01);
        let collapse = reader_collapse_rect(w as f32, h);
        let copy = reader_copy_rect(w as f32, h);
        assert!(strip.contains(
            f64::from(collapse.x + collapse.w * 0.5),
            f64::from(collapse.y + collapse.h * 0.5),
        ));
        assert!(strip.contains(
            f64::from(copy.x + copy.w * 0.5),
            f64::from(copy.y + copy.h * 0.5),
        ));
    }

    #[test]
    fn reader_budget_exceeds_bubble_budget() {
        assert!(reader_line_budget(720) > bubble_line_budget());
    }

    #[test]
    fn expand_glyph_sits_inside_bubble_rect() {
        let layout = Layout::initial();
        let bubble = layout.bubble_rect();
        let expand = layout.bubble_expand_rect();
        assert!(expand.x >= bubble.x);
        assert!(expand.y >= bubble.y);
        assert!(expand.x + expand.w <= bubble.x + bubble.w);
        assert!(expand.y + expand.h <= bubble.y + bubble.h);
    }

    #[test]
    fn reader_overflow_is_honest() {
        let font = load_font().expect("system font available for reader test");
        let long = (0..400).map(|i| format!("word{i}")).collect::<Vec<_>>().join(" ");
        let budget = reader_line_budget(720);
        let (lines, hidden) = budgeted_lines(&font, &long, TEXT_PX, SURFACE_W as f32 - 44.0, budget);
        assert!(hidden > 0);
        assert!(lines.last().unwrap().starts_with('+'));
    }

    #[test]
    fn tucked_bubble_overflow_draws_more_marker() {
        let font = load_font().expect("system font available for tucked bubble test");
        let pad_top = 18.0;
        let max_lines =
            (((TUCK_PEEK_BUBBLE_H - pad_top - 6.0) / LINE_H).floor() as i32).max(1) as usize;
        let long = (0..60).map(|i| format!("word{i}")).collect::<Vec<_>>().join(" ");
        let (lines, hidden) = budgeted_lines(&font, &long, TEXT_PX, TUCK_PEEK_W - 24.0, max_lines);
        assert!(hidden > 0);
        assert!(lines.last().unwrap().starts_with('+'));
    }

    #[test]
    fn tucked_expand_glyph_inside_peek_bubble() {
        const W: u32 = 200;
        const H: u32 = 120;
        let edge = BumpEdge::Left;
        let bubble = tucked_bubble_rect(edge, W, H);
        let expand = tucked_bubble_expand_rect(edge, W, H);
        assert!(expand.x >= bubble.x);
        assert!(expand.y >= bubble.y);
        assert!(expand.x + expand.w <= bubble.x + bubble.w);
        assert!(expand.y + expand.h <= bubble.y + bubble.h);
    }

    #[test]
    fn reader_paints_identically_tucked_and_untucked() {
        let w = SURFACE_W;
        let h = 480_u32;
        let sprite = Sprite::new();
        let text = "Same reader card whether the buddy was tucked or open.";
        let paint = |tucked: Option<BumpEdge>| -> Vec<u8> {
            let mut canvas = vec![0_u8; (w * h * 4) as usize];
            let view = BodyView {
                t: 0.0,
                emotion: Emotion::Neutral,
                speech: Some(text),
                torso_output: TorsoOutput::Text(TextCard { title: "", body: "" }),
                chat_open: false,
                tucked,
                tucked_show_bubble: tucked.is_some(),
                tucked_show_input: false,
                input_text: "",
                input_placeholder: "",
                input_focused: false,
                review_pending: false,
                edit_pending: false,
                posture_badge: None,
                surface_bloom: &[],
                route_health: None,
                route_flash: false,
                alert_level: None,
                activity: false,
                receipt_rail: &[],
                receipt_scroll: 0,
                show_receipt_ledger: false,
                interior_rows: &[],
                settings: &[],
                onboarding: None,
                layout: Layout::initial(),
                pinned: None,
                frame: None,
                color: CLAY_DEFAULT,
                dock_show: DockShow::Both,
                skin: Skin::Clay,
                reader: Some(text),
                reader_scroll: 0,
                reader_copied: false,
                reader_selection: None,
            };
            sprite.paint(&mut canvas, w, h, &view);
            canvas
        };
        assert_eq!(paint(Some(BumpEdge::Left)), paint(None));
    }

    #[test]
    fn reader_scroll_clamps_to_text() {
        assert_eq!(clamp_reader_scroll(0, 10, 4), 0);
        assert_eq!(clamp_reader_scroll(99, 10, 4), 6);
        assert_eq!(clamp_reader_scroll(3, 4, 4), 0);
    }

    #[test]
    fn reader_window_draws_the_scrolled_lines() {
        let font = load_font().expect("system font available for reader scroll test");
        let text = (0..60).map(|i| format!("line{i}")).collect::<Vec<_>>().join("\n");
        let all = reader_wrapped_md_lines(&font, &text, BUBBLE_W_DEFAULT);
        let h = 480_u32;
        let budget = reader_body_budget(h, all.len());
        let scroll = 5;
        let end = (scroll + budget).min(all.len());
        assert_eq!(plain_wrapped_line(&all[scroll]), "line5");
        assert!(end < all.len(), "scrolled window must still overflow at h=480");
        let sprite = Sprite::new();
        let paint = |scroll: usize| -> Vec<u8> {
            let mut canvas = vec![0_u8; (SURFACE_W * h * 4) as usize];
            let view = BodyView {
                t: 0.0,
                emotion: Emotion::Neutral,
                speech: Some(&text),
                torso_output: TorsoOutput::Text(TextCard { title: "", body: "" }),
                chat_open: false,
                tucked: None,
                tucked_show_bubble: false,
                tucked_show_input: false,
                input_text: "",
                input_placeholder: "",
                input_focused: false,
                review_pending: false,
                edit_pending: false,
                posture_badge: None,
                surface_bloom: &[],
                route_health: None,
                route_flash: false,
                alert_level: None,
                activity: false,
                receipt_rail: &[],
                receipt_scroll: 0,
                show_receipt_ledger: false,
                interior_rows: &[],
                settings: &[],
                onboarding: None,
                layout: Layout::initial(),
                pinned: None,
                frame: None,
                color: CLAY_DEFAULT,
                dock_show: DockShow::Both,
                skin: Skin::Clay,
                reader: Some(&text),
                reader_scroll: scroll,
                reader_copied: false,
                reader_selection: None,
            };
            sprite.paint(&mut canvas, SURFACE_W, h, &view);
            canvas
        };
        assert_ne!(paint(0), paint(scroll), "scrolling must change reader pixels");
    }

    #[test]
    fn reader_footer_reports_position() {
        assert_eq!(reader_footer_text(0, 4, 12, false), "lines 1–4 of 12");
        assert_eq!(reader_footer_text(8, 4, 12, false), "lines 9–12 of 12");
        assert_eq!(reader_footer_text(0, 12, 12, false), "");
        assert_eq!(reader_footer_text(0, 4, 12, true), "Copied ✓");
    }

    #[test]
    fn copy_glyphs_sit_inside_their_cards() {
        let layout = Layout::initial();
        let bubble = layout.bubble_rect();
        let bubble_copy = layout.bubble_copy_rect();
        assert!(bubble_copy.x >= bubble.x);
        assert!(bubble_copy.y >= bubble.y);
        assert!(bubble_copy.x + bubble_copy.w <= bubble.x + bubble.w);
        assert!(bubble_copy.y + bubble_copy.h <= bubble.y + bubble.h);

        let h = 720_u32;
        let card = reader_card_rect(SURFACE_W as f32, h);
        let reader_copy = reader_copy_rect(SURFACE_W as f32, h);
        assert!(reader_copy.x >= card.x);
        assert!(reader_copy.y >= card.y);
        assert!(reader_copy.x + reader_copy.w <= card.x + card.w);
        assert!(reader_copy.y + reader_copy.h <= card.y + card.h);

        const TW: u32 = 200;
        const TH: u32 = 120;
        let edge = BumpEdge::Left;
        let tucked = tucked_bubble_rect(edge, TW, TH);
        let tucked_copy = tucked_bubble_copy_rect(edge, TW, TH);
        assert!(tucked_copy.x >= tucked.x);
        assert!(tucked_copy.y >= tucked.y);
        assert!(tucked_copy.x + tucked_copy.w <= tucked.x + tucked.w);
        assert!(tucked_copy.y + tucked_copy.h <= tucked.y + tucked.h);
    }

    #[test]
    fn markdown_lite_strips_markers_to_plain() {
        let lines = markdown_lite("**bold** and `code`");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].plain_text(), "bold and code");
        let plain = markdown_plain_projection("* item\n# Title");
        assert!(!plain.contains('*') || plain.starts_with('•'));
        assert!(!plain.contains('#'));
        assert!(plain.contains('•'));
    }

    #[test]
    fn markdown_lite_marks_bold_spans() {
        let lines = markdown_lite("Say **hello** there");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].spans.len(), 3);
        assert!(!lines[0].spans[0].bold);
        assert!(lines[0].spans[1].bold);
        assert_eq!(lines[0].spans[1].text, "hello");
    }

    #[test]
    fn markdown_lite_maps_bullets_and_headings() {
        let lines = markdown_lite("# Heading\n- bullet\n2. second");
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].kind, MdLineKind::Heading);
        assert_eq!(lines[1].kind, MdLineKind::Bullet);
        assert_eq!(lines[2].kind, MdLineKind::Numbered(2));
        assert_eq!(lines[1].plain_text(), "• bullet");
        assert_eq!(lines[2].plain_text(), "2. second");
    }

    #[test]
    fn bubble_renders_plain_projection() {
        let raw = "**Hello**\n- one";
        let plain = markdown_plain_projection(raw);
        assert!(!plain.contains("**"));
        assert!(plain.contains("Hello"));
        assert!(plain.contains("• one"));
        let font = load_font().expect("system font available for bubble projection test");
        let (lines, _) = budgeted_lines(&font, &plain, TEXT_PX, BUBBLE_W - 28.0, bubble_line_budget());
        assert!(lines.iter().any(|line| line.contains('•')));
    }

    #[test]
    fn hit_maps_x_to_char_index() {
        let font = load_font().expect("system font available for hit index test");
        let line = "Hello";
        assert_eq!(hit_char_index(&font, line, TEXT_PX, 0.0), 0);
        let mid = hit_char_index(&font, line, TEXT_PX, measure(&font, "He", TEXT_PX) + 1.0);
        assert!(mid >= 2);
        assert_eq!(hit_char_index(&font, line, TEXT_PX, 999.0), line.chars().count());
    }

    #[test]
    fn selection_highlight_paints_behind_text() {
        let w = SURFACE_W;
        let h = 480_u32;
        let sprite = Sprite::new();
        let text = "Select this sentence in the reader.";
        let sel = (
            ReaderPos { line: 0, ch: 0 },
            ReaderPos { line: 0, ch: 6 },
        );
        let paint = |selection: Option<(ReaderPos, ReaderPos)>| -> Vec<u8> {
            let mut canvas = vec![0_u8; (w * h * 4) as usize];
            let view = BodyView {
                t: 0.0,
                emotion: Emotion::Neutral,
                speech: Some(text),
                torso_output: TorsoOutput::Text(TextCard { title: "", body: "" }),
                chat_open: false,
                tucked: None,
                tucked_show_bubble: false,
                tucked_show_input: false,
                input_text: "",
                input_placeholder: "",
                input_focused: false,
                review_pending: false,
                edit_pending: false,
                posture_badge: None,
                surface_bloom: &[],
                route_health: None,
                route_flash: false,
                alert_level: None,
                activity: false,
                receipt_rail: &[],
                receipt_scroll: 0,
                show_receipt_ledger: false,
                interior_rows: &[],
                settings: &[],
                onboarding: None,
                layout: Layout::initial(),
                pinned: None,
                frame: None,
                color: CLAY_DEFAULT,
                dock_show: DockShow::Both,
                skin: Skin::Clay,
                reader: Some(text),
                reader_scroll: 0,
                reader_copied: false,
                reader_selection: selection,
            };
            sprite.paint(&mut canvas, w, h, &view);
            canvas
        };
        let without = paint(None);
        let with = paint(Some(sel));
        assert_ne!(without, with, "selection highlight must change reader pixels");
    }

    #[test]
    fn selection_copy_yields_the_plain_span() {
        let font = load_font().expect("system font available for selection copy test");
        let text = "Alpha line\nBeta line";
        let sel = (
            ReaderPos { line: 0, ch: 6 },
            ReaderPos { line: 1, ch: 4 },
        );
        let plain = reader_selection_plain(&font, text, BUBBLE_W_DEFAULT, sel);
        assert_eq!(plain, "line\nBeta");
    }

    #[test]
    fn selection_clears_on_reader_close() {
        // With the reader closed, a stale selection must have no pixel effect —
        // the highlight lives only inside the reader takeover.
        let w = SURFACE_W;
        let h = 480_u32;
        let sprite = Sprite::new();
        let text = "Select this sentence in the reader.";
        let stale = Some((
            ReaderPos { line: 0, ch: 0 },
            ReaderPos { line: 0, ch: 6 },
        ));
        let paint = |selection: Option<(ReaderPos, ReaderPos)>| -> Vec<u8> {
            let mut canvas = vec![0_u8; (w * h * 4) as usize];
            let view = BodyView {
                t: 0.0,
                emotion: Emotion::Neutral,
                speech: Some(text),
                torso_output: TorsoOutput::Text(TextCard { title: "", body: "" }),
                chat_open: false,
                tucked: None,
                tucked_show_bubble: false,
                tucked_show_input: false,
                input_text: "",
                input_placeholder: "",
                input_focused: false,
                review_pending: false,
                edit_pending: false,
                posture_badge: None,
                surface_bloom: &[],
                route_health: None,
                route_flash: false,
                alert_level: None,
                activity: false,
                receipt_rail: &[],
                receipt_scroll: 0,
                show_receipt_ledger: false,
                interior_rows: &[],
                settings: &[],
                onboarding: None,
                layout: Layout::initial(),
                pinned: None,
                frame: None,
                color: CLAY_DEFAULT,
                dock_show: DockShow::Both,
                skin: Skin::Clay,
                reader: None,
                reader_scroll: 0,
                reader_copied: false,
                reader_selection: selection,
            };
            sprite.paint(&mut canvas, w, h, &view);
            canvas
        };
        assert_eq!(
            paint(stale),
            paint(None),
            "closed reader must ignore any stale selection"
        );
    }
}
