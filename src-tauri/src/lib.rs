use serde::Serialize;
use tauri::{AppHandle, PhysicalPosition, PhysicalSize, Position, Size, WebviewWindow};

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

#[derive(Debug, Serialize)]
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

#[tauri::command]
fn configure_border_dock(
    app: AppHandle,
    window: WebviewWindow,
    multi_monitor: Option<bool>,
) -> Result<DockLayout, String> {
    let use_multi_monitor = multi_monitor.unwrap_or(false);
    let monitors = collect_monitors(&app)?;
    let selected_monitors = select_monitors(&monitors, use_multi_monitor);
    let bounds = calculate_bounds(&selected_monitors)?;

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
        .set_position(Position::Physical(PhysicalPosition::new(bounds.x, bounds.y)))
        .map_err(|error| error.to_string())?;
    window
        .set_size(Size::Physical(PhysicalSize::new(bounds.width, bounds.height)))
        .map_err(|error| error.to_string())?;

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

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![configure_border_dock])
        .run(tauri::generate_context!())
        .expect("error while running Border Agents");
}
