use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{
    AppHandle, Manager, PhysicalPosition, PhysicalSize, Position, Size, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

static BB_LOG_MUTEX: Mutex<()> = Mutex::new(());

const MAX_HITBOX_DIMENSION: i32 = 8192;

fn bb_events_log(line: &str) {
    eprintln!("{line}");

    let Ok(_guard) = BB_LOG_MUTEX.lock() else {
        return;
    };

    if let Ok(path) = std::env::var("BB_LOG_EVENTS") {
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(file, "{line}");
        }
    }
}

fn bb_events_timestamp() -> String {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_millis().to_string(),
        Err(_) => "0".to_string(),
    }
}

fn bb_env_flag_enabled(name: &str) -> bool {
    std::env::var(name)
        .map(|value| matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false)
}



fn init_bb_diagnostics() {
    std::panic::set_hook(Box::new(|info| {
        bb_events_log(&format!(
            "[rust panic {}] {info}",
            bb_events_timestamp()
        ));
    }));

    bb_events_log(&format!(
        "[rust {}] diagnostics initialized (RUST_BACKTRACE={})",
        bb_events_timestamp(),
        std::env::var("RUST_BACKTRACE").unwrap_or_else(|_| "unset".to_string())
    ));
}

#[cfg(target_os = "linux")]
fn configure_linux_webkit_environment() {
    // Native Wayland forbids clients from positioning their own toplevel
    // windows, so per-buddy windows stack uselessly and set_position is a
    // silent no-op. Running through XWayland restores client positioning,
    // always-on-top, and X11 input shapes. Opt out by setting GDK_BACKEND.
    if std::env::var("GDK_BACKEND").is_err() {
        std::env::set_var("GDK_BACKEND", "x11");
        bb_events_log(&format!(
            "[rust {}] GDK_BACKEND=x11 set: forcing XWayland so overlay windows can self-position",
            bb_events_timestamp()
        ));
    }

    if std::env::var("WEBKIT_DISABLE_DMABUF_RENDERER").is_err() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        bb_events_log(&format!(
            "[rust {}] WEBKIT_DISABLE_DMABUF_RENDERER=1 set unconditionally for transparent-window repaint stability",
            bb_events_timestamp()
        ));
    }

    if std::env::var("GSK_RENDERER").is_err() {
        std::env::set_var("GSK_RENDERER", "gl");
    }
}

fn sanitize_hitbox(hitbox: &Hitbox) -> Option<(i32, i32, i32, i32)> {
    if hitbox.w <= 0 || hitbox.h <= 0 {
        return None;
    }

    let width = hitbox.w.clamp(1, MAX_HITBOX_DIMENSION);
    let height = hitbox.h.clamp(1, MAX_HITBOX_DIMENSION);
    let x = hitbox.x.clamp(-4096, 16384);
    let y = hitbox.y.clamp(-4096, 16384);

    Some((x, y, width, height))
}

#[cfg(target_os = "linux")]
fn configure_linux_transparent_window(window: &WebviewWindow) -> Result<(), String> {
    use gtk::prelude::*;

    let gtk_win = window.gtk_window().map_err(|error| error.to_string())?;
    gtk_win.set_app_paintable(true);

    if let Some(screen) = gtk::prelude::WidgetExt::screen(&gtk_win) {
        if let Some(visual) = screen.rgba_visual() {
            gtk_win.set_visual(Some(&visual));
        }
    }

    request_linux_repaint(&gtk_win);
    Ok(())
}

#[cfg(target_os = "linux")]
fn request_linux_repaint<W>(gtk_win: &W)
where
    W: gtk::prelude::IsA<gtk::Widget>,
{
    use gtk::prelude::*;

    gtk_win.queue_draw();

    if let Some(gdk_win) = gtk_win.window() {
        gdk_win.invalidate_rect(None, true);
    }
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct MonitorFrame {
    id: String,
    name: Option<String>,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    scale_factor: f64,
    primary: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DockBounds {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DockLayout {
    monitors: Vec<MonitorFrame>,
    active_monitor_ids: Vec<String>,
    bounds: DockBounds,
    multi_monitor: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BuddyWindowState {
    buddy_id: String,
    edge: String,
    state: String,
    slot: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BuddySnapRequest {
    buddy_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BuddySnapResult {
    buddy_id: String,
    snapped: bool,
    edge: Option<String>,
    slot: Option<f64>,
    bounds: DockBounds,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BuddyWindowLayout {
    buddy_id: String,
    monitor: MonitorFrame,
    bounds: DockBounds,
    interactive: bool,
}

#[derive(Clone, Copy)]
struct BuddyWindowSpec {
    id: &'static str,
    label: &'static str,
    title: &'static str,
    edge: &'static str,
    slot: f64,
}

#[derive(Clone, Copy)]
struct DockZone {
    edge: &'static str,
    slot: f64,
}

const BUDDY_WINDOWS: &[BuddyWindowSpec] = &[
    BuddyWindowSpec {
        id: "hermes",
        label: "buddy-hermes",
        title: "Hermes · Border Buddies",
        edge: "right",
        slot: 0.58,
    },
    BuddyWindowSpec {
        id: "crab",
        label: "buddy-crab",
        title: "Claw · Border Buddies",
        edge: "left",
        slot: 0.72,
    },
    BuddyWindowSpec {
        id: "owl",
        label: "buddy-owl",
        title: "Veritas · Border Buddies",
        edge: "top",
        slot: 0.24,
    },
    BuddyWindowSpec {
        id: "fox",
        label: "buddy-fox",
        title: "Nexus · Border Buddies",
        edge: "bottom",
        slot: 0.68,
    },
];
const DEFAULT_BUDDY_IDS: &[&str] = &["hermes", "crab", "owl", "fox"];

const DOCK_ZONES: &[DockZone] = &[
    DockZone {
        edge: "left",
        slot: 0.22,
    },
    DockZone {
        edge: "left",
        slot: 0.50,
    },
    DockZone {
        edge: "left",
        slot: 0.78,
    },
    DockZone {
        edge: "right",
        slot: 0.22,
    },
    DockZone {
        edge: "right",
        slot: 0.50,
    },
    DockZone {
        edge: "right",
        slot: 0.78,
    },
    DockZone {
        edge: "top",
        slot: 0.24,
    },
    DockZone {
        edge: "top",
        slot: 0.50,
    },
    DockZone {
        edge: "top",
        slot: 0.76,
    },
    DockZone {
        edge: "bottom",
        slot: 0.24,
    },
    DockZone {
        edge: "bottom",
        slot: 0.50,
    },
    DockZone {
        edge: "bottom",
        slot: 0.76,
    },
];

#[tauri::command]
fn configure_border_dock(
    app: AppHandle,
    window: WebviewWindow,
    multi_monitor: Option<bool>,
    custom_bounds: Option<DockBounds>,
) -> Result<DockLayout, String> {
    let use_multi_monitor = multi_monitor.unwrap_or(false);
    let monitors = collect_monitors(&app)?;
    let selected_monitors = select_monitors(&monitors, use_multi_monitor);
    let bounds = if let Some(cb) = custom_bounds {
        cb
    } else {
        calculate_bounds(&selected_monitors)?
    };

    window
        .set_always_on_top(true)
        .map_err(|error| error.to_string())?;
    window
        .set_decorations(false)
        .map_err(|error| error.to_string())?;
    window
        .set_shadow(false)
        .map_err(|error| error.to_string())?;
    window
        .set_skip_taskbar(true)
        .map_err(|error| error.to_string())?;
    window
        .set_resizable(true)
        .map_err(|error| error.to_string())?;
    window
        .set_position(Position::Physical(PhysicalPosition::new(
            bounds.x, bounds.y,
        )))
        .map_err(|error| error.to_string())?;
    window
        .set_size(Size::Physical(PhysicalSize::new(
            bounds.width,
            bounds.height,
        )))
        .map_err(|error| error.to_string())?;

    #[cfg(target_os = "linux")]
    configure_linux_transparent_window(&window)?;

    let active_monitor_ids = selected_monitors
        .iter()
        .map(|monitor| monitor.id.clone())
        .collect();

    Ok(DockLayout {
        monitors,
        active_monitor_ids,
        bounds,
        multi_monitor: use_multi_monitor,
    })
}

#[tauri::command]
fn configure_buddy_window(
    app: AppHandle,
    window: WebviewWindow,
    request: BuddyWindowState,
) -> Result<BuddyWindowLayout, String> {
    let monitors = collect_monitors(&app)?;
    let monitor = select_monitors(&monitors, false)
        .into_iter()
        .next()
        .ok_or_else(|| "No active monitor selected for the buddy window".to_string())?;
    let envelope = calculate_buddy_envelope(&request.edge);
    let slot = request
        .slot
        .unwrap_or_else(|| default_buddy_slot(&request.buddy_id, &request.edge));
    let bounds = calculate_buddy_window_bounds(&monitor, &request.edge, slot, envelope);

    configure_overlay_window(&window)?;

    // Only reposition for tucked state — window size stays fixed to prevent
    // ghosting on Linux transparent windows (no resize operations).
    if request.state == "tucked" {
        window
            .set_position(Position::Physical(PhysicalPosition::new(
                bounds.x, bounds.y,
            )))
            .map_err(|error| error.to_string())?;

        #[cfg(target_os = "linux")]
        repaint_linux_overlay_window(&window)?;
    }

    Ok(BuddyWindowLayout {
        buddy_id: request.buddy_id,
        monitor,
        bounds,
        interactive: true,
    })
}

#[tauri::command]
fn current_buddy_id(window: WebviewWindow) -> Option<String> {
    let label = window.label();

    BUDDY_WINDOWS
        .iter()
        .find(|spec| spec.label == label)
        .map(|spec| spec.id.to_string())
}

#[derive(serde::Deserialize, Clone, Debug)]
struct Hitbox {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

#[tauri::command]
fn bb_append_log(line: String) -> Result<(), String> {
    bb_events_log(&line);
    Ok(())
}

#[tauri::command]
fn set_input_hitboxes(window: WebviewWindow, boxes: Vec<Hitbox>) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        use gtk::prelude::*;

        let gtk_win = window.gtk_window().map_err(|e| e.to_string())?;
        let gdk_win = gtk_win.window().ok_or("Window not realized")?;

        let scale = gtk_win.scale_factor();
        let region = cairo::Region::create();
        let mut applied = 0usize;

        for hitbox in &boxes {
            let Some((x, y, width, height)) = sanitize_hitbox(hitbox) else {
                continue;
            };

            region
                .union_rectangle(&cairo::RectangleInt::new(
                    x * scale,
                    y * scale,
                    width * scale,
                    height * scale,
                ))
                .map_err(|error| {
                    let message = format!(
                        "union_rectangle failed for hitbox {:?}: {error}",
                        hitbox
                    );
                    bb_events_log(&format!(
                        "[rust {}] ERROR: {}",
                        bb_events_timestamp(),
                        message
                    ));
                    message
                })?;
            applied += 1;
        }

        gdk_win.input_shape_combine_region(&region, 0, 0);
        request_linux_repaint(&gtk_win);

        if applied == 0 && !boxes.is_empty() {
            bb_events_log(&format!(
                "[rust {}] WARN set_input_hitboxes produced empty region (requested={} scale={scale})",
                bb_events_timestamp(),
                boxes.len()
            ));
        }

        // Opt-in diagnostics for the Hermes first-connection clickability work.
        // Enable with `BB_LOG_HITBOXES=1` to confirm the clickable region the
        // dock is actually applying to the border-dock window.
        if bb_env_flag_enabled("BB_LOG_HITBOXES") {
            bb_events_log(&format!(
                "[rust {}] set_input_hitboxes window={} applied={}/{} scale={} boxes={:?}",
                bb_events_timestamp(),
                window.label(),
                applied,
                boxes.len(),
                scale,
                boxes
            ));
        }


    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (window, boxes);
    }

    Ok(())
}

/// Per-window state for the resize-jiggle repaint (see `jiggle_overlay_size`).
#[cfg(target_os = "linux")]
struct JiggleState {
    base: PhysicalSize<u32>,
    shrunk: bool,
    last_shrink: std::time::Instant,
}

#[cfg(target_os = "linux")]
static OVERLAY_JIGGLE: Mutex<Vec<(String, JiggleState)>> = Mutex::new(Vec::new());

/// Minimum interval between shrink pulses while a drag streams repaint calls.
#[cfg(target_os = "linux")]
const JIGGLE_MIN_INTERVAL_MS: u128 = 90;

/// On WebKitGTK >= 2.46 (Skia) the accelerated-compositing scene ignores
/// GDK-level invalidation (`queue_draw`/`invalidate_rect`), so stale pixels
/// from moved buddies survive every conventional repaint. The only operation
/// that reliably forces WebKit to reallocate and redraw its compositing
/// buffers is an actual window resize. This alternates the window height by
/// 1px (throttled) so each call performs a real size change — GTK coalesces
/// queued resizes, so a shrink+restore in one call would net to nothing.
#[cfg(target_os = "linux")]
fn jiggle_overlay_size(window: &WebviewWindow) -> Result<(), String> {
    if bb_env_flag_enabled("BB_DISABLE_RESIZE_REPAINT") {
        return Ok(());
    }

    let label = window.label().to_string();
    let Ok(mut states) = OVERLAY_JIGGLE.lock() else {
        return Ok(());
    };

    if let Some((_, state)) = states.iter_mut().find(|(name, _)| *name == label) {
        if state.shrunk {
            window
                .set_size(Size::Physical(state.base))
                .map_err(|error| error.to_string())?;
            state.shrunk = false;
        } else if state.last_shrink.elapsed().as_millis() >= JIGGLE_MIN_INTERVAL_MS {
            let base = window.inner_size().map_err(|error| error.to_string())?;
            if base.height > 2 {
                state.base = base;
                window
                    .set_size(Size::Physical(PhysicalSize::new(
                        base.width,
                        base.height - 1,
                    )))
                    .map_err(|error| error.to_string())?;
                state.shrunk = true;
                state.last_shrink = std::time::Instant::now();
            }
        }
        return Ok(());
    }

    let base = window.inner_size().map_err(|error| error.to_string())?;
    if base.height > 2 {
        window
            .set_size(Size::Physical(PhysicalSize::new(
                base.width,
                base.height - 1,
            )))
            .map_err(|error| error.to_string())?;
        states.push((
            label,
            JiggleState {
                base,
                shrunk: true,
                last_shrink: std::time::Instant::now(),
            },
        ));
    }

    Ok(())
}

#[cfg(target_os = "linux")]
fn repaint_linux_overlay_window(window: &WebviewWindow) -> Result<(), String> {
    let gtk_win = window.gtk_window().map_err(|error| error.to_string())?;
    request_linux_repaint(&gtk_win);
    jiggle_overlay_size(window)?;
    Ok(())
}

#[tauri::command]
fn repaint_overlay_window(window: WebviewWindow) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    repaint_linux_overlay_window(&window)?;

    #[cfg(not(target_os = "linux"))]
    let _ = window;

    Ok(())
}

#[tauri::command]
fn reset_dock_input(window: WebviewWindow) -> Result<(), String> {
    // Only restore cursor capture. Clearing the GTK input shape here races with
    // native window drags and can crash WebKitGTK on the next move attempt.
    window
        .set_ignore_cursor_events(false)
        .map_err(|error| error.to_string())?;

    Ok(())
}

#[tauri::command]
fn set_buddy_window_interactive(_window: WebviewWindow, _interactive: bool) -> Result<(), String> {
    // Left as a no-op if we rely on input shapes, or optionally toggle visibility
    // window
    //     .set_ignore_cursor_events(!interactive)
    //     .map_err(|error| error.to_string())?;

    Ok(())
}

#[tauri::command]
fn snap_buddy_window(
    app: AppHandle,
    window: WebviewWindow,
    request: BuddySnapRequest,
) -> Result<BuddySnapResult, String> {
    let monitors = collect_monitors(&app)?;
    let monitor = select_monitors(&monitors, false)
        .into_iter()
        .next()
        .ok_or_else(|| "No active monitor selected for the buddy window".to_string())?;
    let position = window.outer_position().map_err(|error| error.to_string())?;
    let size = window.inner_size().map_err(|error| error.to_string())?;
    let dock_zone = nearest_dock_zone(&monitor, &position, &size);

    if let Some(dock_zone) = dock_zone {
        let envelope = calculate_buddy_envelope(dock_zone.edge);
        let bounds =
            calculate_buddy_window_bounds(&monitor, dock_zone.edge, dock_zone.slot, envelope);

        window
            .set_position(Position::Physical(PhysicalPosition::new(
                bounds.x, bounds.y,
            )))
            .map_err(|error| error.to_string())?;

        #[cfg(target_os = "linux")]
        repaint_linux_overlay_window(&window)?;

        return Ok(BuddySnapResult {
            buddy_id: request.buddy_id,
            snapped: true,
            edge: Some(dock_zone.edge.to_string()),
            slot: Some(dock_zone.slot),
            bounds,
        });
    }

    Ok(BuddySnapResult {
        buddy_id: request.buddy_id,
        snapped: false,
        edge: None,
        slot: None,
        bounds: DockBounds {
            x: position.x,
            y: position.y,
            width: size.width,
            height: size.height,
        },
    })
}

fn create_buddy_windows(app: &AppHandle) -> Result<(), String> {
    let monitors = collect_monitors(app)?;
    let monitor = select_monitors(&monitors, false)
        .into_iter()
        .next()
        .ok_or_else(|| "No active monitor selected for Border Buddies".to_string())?;

    let enabled_buddy_ids = enabled_buddy_ids();

    for spec in BUDDY_WINDOWS {
        if !enabled_buddy_ids.iter().any(|buddy_id| *buddy_id == spec.id) {
            continue;
        }

        if app.get_webview_window(spec.label).is_some() {
            continue;
        }

        let envelope = calculate_buddy_envelope(spec.edge);
        let bounds = calculate_buddy_window_bounds(&monitor, spec.edge, spec.slot, envelope);
        let window = WebviewWindowBuilder::new(
            app,
            spec.label,
            WebviewUrl::App(format!("index.html?buddy={}", spec.id).into()),
        )
        .title(spec.title)
        .inner_size(envelope.0 as f64, envelope.1 as f64)
        .position(bounds.x as f64, bounds.y as f64)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .shadow(false)
        .focused(false)
        .focusable(true)
        .disable_drag_drop_handler()
        .visible(true)
        .build()
        .map_err(|error| error.to_string())?;

        configure_overlay_window(&window)?;
    }

    // Create a dedicated dock-chrome window for the controls bar (mode switcher,
    // interact/dock pills, ticker). The unified border-dock window is hidden in
    // NVIDIA per-buddy mode to prevent transparent fullscreen ghosting, but this
    // small fixed-position window at the top-centre is safe from that artifact.
    if app.get_webview_window("dock-chrome").is_none() {
        create_dock_chrome_window(app, &monitor)?;
    }

    Ok(())
}

/// Width/height of the dock-chrome controls bar window.
const DOCK_CHROME_WIDTH: u32 = 900;
const DOCK_CHROME_HEIGHT: u32 = 96;

fn create_dock_chrome_window(app: &AppHandle, monitor: &MonitorFrame) -> Result<(), String> {
    // Centre horizontally at the top of the primary monitor.
    let x = monitor.x + (monitor.width as i32 - DOCK_CHROME_WIDTH as i32) / 2;
    let y = monitor.y + 4;

    let window = WebviewWindowBuilder::new(
        app,
        "dock-chrome",
        WebviewUrl::App("index.html?mode=dock-chrome".into()),
    )
    .title("Border Buddies · Dock")
    .inner_size(DOCK_CHROME_WIDTH as f64, DOCK_CHROME_HEIGHT as f64)
    .position(x as f64, y as f64)
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .resizable(false)
    .maximizable(false)
    .minimizable(false)
    .shadow(false)
    .focused(false)
    .focusable(true)
    .disable_drag_drop_handler()
    .visible(true)
    .build()
    .map_err(|error| error.to_string())?;

    configure_overlay_window(&window)?;

    bb_events_log(&format!(
        "[rust {}] dock-chrome window created at ({x}, {y})",
        bb_events_timestamp()
    ));

    Ok(())
}


fn enabled_buddy_ids() -> Vec<&'static str> {
    let Ok(value) = std::env::var("BORDER_BUDDIES") else {
        return DEFAULT_BUDDY_IDS.to_vec();
    };

    let selected = value
        .split(',')
        .filter_map(|candidate| {
            let candidate = candidate.trim();
            BUDDY_WINDOWS
                .iter()
                .find(|spec| spec.id == candidate)
                .map(|spec| spec.id)
        })
        .collect::<Vec<_>>();

    if selected.is_empty() {
        DEFAULT_BUDDY_IDS.to_vec()
    } else {
        selected
    }
}

fn configure_overlay_window(window: &WebviewWindow) -> Result<(), String> {
    window
        .set_always_on_top(true)
        .map_err(|error| error.to_string())?;
    window
        .set_decorations(false)
        .map_err(|error| error.to_string())?;
    window
        .set_skip_taskbar(true)
        .map_err(|error| error.to_string())?;

    #[cfg(target_os = "linux")]
    configure_linux_transparent_window(window)?;

    Ok(())
}

fn collect_monitors(app: &AppHandle) -> Result<Vec<MonitorFrame>, String> {
    let primary_name = app
        .primary_monitor()
        .map_err(|error| error.to_string())?
        .and_then(|monitor| monitor.name().map(ToOwned::to_owned));

    let monitors = app
        .available_monitors()
        .map_err(|error| error.to_string())?
        .into_iter()
        .enumerate()
        .map(|(index, monitor)| {
            let position = monitor.position();
            let size = monitor.size();
            let name = monitor.name().map(ToOwned::to_owned);
            let primary = name.is_some() && name == primary_name;

            MonitorFrame {
                id: name.clone().unwrap_or_else(|| format!("monitor-{index}")),
                name,
                x: position.x,
                y: position.y,
                width: size.width,
                height: size.height,
                scale_factor: monitor.scale_factor(),
                primary,
            }
        })
        .collect::<Vec<_>>();

    if monitors.is_empty() {
        return Err("No monitors available for the BorderDock window".to_string());
    }

    Ok(monitors)
}

fn select_monitors(monitors: &[MonitorFrame], multi_monitor: bool) -> Vec<MonitorFrame> {
    if multi_monitor {
        return monitors.to_vec();
    }

    monitors
        .iter()
        .find(|monitor| monitor.primary)
        .or_else(|| monitors.first())
        .into_iter()
        .cloned()
        .collect()
}

fn calculate_bounds(monitors: &[MonitorFrame]) -> Result<DockBounds, String> {
    let first = monitors
        .first()
        .ok_or_else(|| "No active monitor selected for the BorderDock window".to_string())?;

    let mut min_x = first.x;
    let mut min_y = first.y;
    let mut max_x = first.x + first.width as i32;
    let mut max_y = first.y + first.height as i32;

    for monitor in monitors.iter().skip(1) {
        min_x = min_x.min(monitor.x);
        min_y = min_y.min(monitor.y);
        max_x = max_x.max(monitor.x + monitor.width as i32);
        max_y = max_y.max(monitor.y + monitor.height as i32);
    }

    Ok(DockBounds {
        x: min_x,
        y: min_y,
        width: (max_x - min_x) as u32,
        height: (max_y - min_y) as u32,
    })
}

/// Fixed envelope for per-buddy overlay windows (never resized at runtime).
/// Must fit head + speech bubble + full interactive chat panel inside one
/// transparent window; input shapes keep the unused envelope click-through.
const BUDDY_ENVELOPE_WIDTH: u32 = 640;
const BUDDY_ENVELOPE_HEIGHT: u32 = 720;

/// Returns the fixed (width, height) envelope for a buddy control window.
fn calculate_buddy_envelope(_edge: &str) -> (u32, u32) {
    (BUDDY_ENVELOPE_WIDTH, BUDDY_ENVELOPE_HEIGHT)
}

fn calculate_buddy_window_bounds(
    monitor: &MonitorFrame,
    edge: &str,
    slot: f64,
    envelope: (u32, u32),
) -> DockBounds {
    let (width, height) = envelope;
    let slot = slot.clamp(0.0, 1.0);
    let x_span_start = monitor.x + 96;
    let x_span_end = monitor.x + monitor.width as i32 - width as i32 - 96;
    let y_span_start = monitor.y + 72;
    let y_span_end = monitor.y + monitor.height as i32 - height as i32 - 72;
    let slotted_x = lerp_i32(x_span_start, x_span_end, slot);
    let slotted_y = lerp_i32(y_span_start, y_span_end, slot);

    let x = match edge {
        "left" => monitor.x,
        "right" => monitor.x + monitor.width as i32 - width as i32,
        _ => slotted_x,
    };
    let y = match edge {
        "top" => monitor.y + 48,
        "bottom" => monitor.y + monitor.height as i32 - height as i32 - 24,
        _ => slotted_y,
    };

    DockBounds {
        x,
        y,
        width,
        height,
    }
}

fn default_buddy_slot(buddy_id: &str, edge: &str) -> f64 {
    BUDDY_WINDOWS
        .iter()
        .find(|spec| spec.id == buddy_id)
        .map(|spec| spec.slot)
        .unwrap_or_else(|| {
            DOCK_ZONES
                .iter()
                .find(|zone| zone.edge == edge)
                .map(|zone| zone.slot)
                .unwrap_or(0.5)
        })
}

fn nearest_dock_zone(
    monitor: &MonitorFrame,
    position: &PhysicalPosition<i32>,
    size: &PhysicalSize<u32>,
) -> Option<DockZone> {
    const SNAP_DISTANCE: i32 = 32;

    let distances = [
        ("left", position.x - monitor.x),
        (
            "right",
            monitor.x + monitor.width as i32 - (position.x + size.width as i32),
        ),
        ("top", position.y - monitor.y),
        (
            "bottom",
            monitor.y + monitor.height as i32 - (position.y + size.height as i32),
        ),
    ];
    let (edge, distance) = distances
        .into_iter()
        .min_by_key(|(_, distance)| distance.abs())?;

    if distance.abs() > SNAP_DISTANCE {
        return None;
    }

    let current_center_x = position.x + size.width as i32 / 2;
    let current_center_y = position.y + size.height as i32 / 2;

    DOCK_ZONES
        .iter()
        .copied()
        .filter(|zone| zone.edge == edge)
        .min_by_key(|zone| {
            let envelope = calculate_buddy_envelope(zone.edge);
            let bounds = calculate_buddy_window_bounds(monitor, zone.edge, zone.slot, envelope);
            let dock_center_x = bounds.x + bounds.width as i32 / 2;
            let dock_center_y = bounds.y + bounds.height as i32 / 2;

            (dock_center_x - current_center_x).abs() + (dock_center_y - current_center_y).abs()
        })
}

fn lerp_i32(start: i32, end: i32, slot: f64) -> i32 {
    if end <= start {
        return start;
    }

    start + ((end - start) as f64 * slot).round() as i32
}

fn legacy_per_buddy_windows_enabled() -> bool {
    if let Ok(value) = std::env::var("BORDER_BUDDIES_LEGACY_WINDOWS") {
        return matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on");
    }

    false
}

pub fn run() {
    init_bb_diagnostics();

    #[cfg(target_os = "linux")]
    configure_linux_webkit_environment();

    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            if legacy_per_buddy_windows_enabled() {
                if let Some(manager_window) = app.get_webview_window("border-dock") {
                    manager_window.hide()?;
                }

                create_buddy_windows(app.handle()).map_err(|error| -> Box<dyn std::error::Error> {
                    Box::new(std::io::Error::new(std::io::ErrorKind::Other, error))
                })?;
            } else if let Some(dock_window) = app.get_webview_window("border-dock") {
                configure_overlay_window(&dock_window)?;
                dock_window.show()?;
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            bb_append_log,
            configure_border_dock,
            configure_buddy_window,
            current_buddy_id,
            snap_buddy_window,
            set_buddy_window_interactive,
            set_input_hitboxes,
            repaint_overlay_window,
            reset_dock_input
        ])
        .run(tauri::generate_context!())
        .expect("error while running Border Agents");
}
