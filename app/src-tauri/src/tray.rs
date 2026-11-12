//! The tray icon and its menu, as 15.0.0's `TrayIcon` lays it out on
//! Windows. A left click captures; the menu is on the right button.

use std::sync::atomic::{AtomicBool, Ordering};

use phramer_core::config::Settings;
use tauri::menu::{IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Runtime};

use crate::actions;
use crate::cli::GuiRequest;
use crate::notify;
use crate::restart;
use crate::updater;

const TRAY_ID: &str = "phramer";
const TOOLTIP: &str = "Phramer";

/// `QTimer::singleShot(400, ...)` in 15.0.0: lets the menu close before the
/// screen is captured.
const MENU_CAPTURE_DELAY_MS: u32 = 400;

const HELLO: &str = "Hello, I'm here! Click icon in the tray to take a screenshot or click with \
                     a right button to see more options.";

/// Whether the icon is shown (`disabledTrayIcon` is off).
#[derive(Default)]
pub(crate) struct TrayState {
    visible: AtomicBool,
}

/// One row of the menu.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Entry {
    Item {
        id: String,
        text: String,
        enabled: bool,
    },
    Separator,
    Submenu {
        text: String,
        items: Vec<Entry>,
    },
}

fn item(id: &str, text: &str) -> Entry {
    Entry::Item {
        id: id.to_owned(),
        text: text.to_owned(),
        enabled: true,
    }
}

/// What decides the shape of the menu.
#[derive(Debug, Clone, Default)]
struct MenuFlags {
    video_capture_enabled: bool,
    check_for_updates: bool,
    /// One label per monitor, in capture-number order.
    monitors: Vec<String>,
    last_capture_available: bool,
    /// A newer version the update check found.
    update: Option<String>,
}

/// `TrayIcon::initMenu` and `initScreenMenu` on Windows.
fn menu_entries(flags: &MenuFlags) -> Vec<Entry> {
    let mut entries = vec![item("capture", "&Take Screenshot")];
    // Only offered once the user has opted in to video
    if flags.video_capture_enabled {
        entries.push(item("record_video", "&Record Video"));
    }
    entries.push(item("open_editor", "Open &Editor"));
    entries.push(item("open_launcher", "&Open Launcher"));
    if flags.monitors.len() > 1 {
        entries.push(Entry::Submenu {
            text: "Select Screen".to_owned(),
            items: flags
                .monitors
                .iter()
                .enumerate()
                .map(|(index, label)| item(&format!("screen:{index}"), label))
                .collect(),
        });
    }
    entries.push(Entry::Separator);
    entries.push(item("open_save_path", "&Open Save Path"));
    entries.push(Entry::Item {
        id: "show_last_capture".to_owned(),
        text: "&Show Last Capture in Folder".to_owned(),
        // Remembered in memory only: nothing to show until this session
        // has saved something
        enabled: flags.last_capture_available,
    });
    entries.push(Entry::Separator);
    entries.push(item("configuration", "&Configuration"));
    entries.push(Entry::Separator);
    // With automatic checks on, the entry appears only once an update has
    // been found, and then right above About. The Store updates the Store
    // build.
    if cfg!(not(feature = "store")) && !flags.check_for_updates && flags.update.is_none() {
        entries.push(item("check_for_updates", "Check for updates"));
    }
    entries.push(item("whats_new", "&What's New"));
    if let Some(version) = flags
        .update
        .as_ref()
        .filter(|_| cfg!(not(feature = "store")))
    {
        // Check only: the release page, where 15.0.0 installed in place
        entries.push(item(
            "check_for_updates",
            &format!("Download version {version}"),
        ));
    }
    entries.push(item("about", "&About"));
    entries.push(Entry::Separator);
    entries.push(item("restart", "&Restart"));
    entries.push(item("quit", "&Quit"));
    entries
}

/// "Monitor 1: \\.\DISPLAY1 (1920x1080)", primary first as Qt lists screens.
fn monitor_labels() -> Vec<String> {
    let mut monitors = match phramer_capture::enumerate_monitors() {
        Ok(monitors) => monitors,
        Err(error) => {
            log::warn!("cannot list monitors for the tray menu: {error}");
            return Vec::new();
        }
    };
    monitors.sort_by_key(|monitor| !monitor.is_primary());
    monitors
        .iter()
        .enumerate()
        .map(|(index, monitor)| {
            let (width, height) = monitor.logical_size();
            format!(
                "Monitor {}: {} ({width}x{height})",
                index + 1,
                monitor.id().as_str()
            )
        })
        .collect()
}

fn flags(settings: &Settings) -> MenuFlags {
    MenuFlags {
        video_capture_enabled: settings.video_capture_enabled(),
        check_for_updates: settings.check_for_updates(),
        monitors: monitor_labels(),
        last_capture_available: actions::last_capture().is_some(),
        update: updater::available_version(),
    }
}

fn build_entries<R: Runtime>(
    app: &AppHandle<R>,
    entries: &[Entry],
) -> tauri::Result<Vec<Box<dyn IsMenuItem<R>>>> {
    let mut built: Vec<Box<dyn IsMenuItem<R>>> = Vec::new();
    for entry in entries {
        match entry {
            Entry::Item { id, text, enabled } => {
                built.push(Box::new(MenuItem::with_id(
                    app,
                    id.as_str(),
                    text,
                    *enabled,
                    None::<&str>,
                )?));
            }
            Entry::Separator => built.push(Box::new(PredefinedMenuItem::separator(app)?)),
            Entry::Submenu { text, items } => {
                let children = build_entries(app, items)?;
                let refs: Vec<&dyn IsMenuItem<R>> = children.iter().map(AsRef::as_ref).collect();
                built.push(Box::new(Submenu::with_items(app, text, true, &refs)?));
            }
        }
    }
    Ok(built)
}

fn build_menu<R: Runtime>(app: &AppHandle<R>, settings: &Settings) -> tauri::Result<Menu<R>> {
    let items = build_entries(app, &menu_entries(&flags(settings)))?;
    let refs: Vec<&dyn IsMenuItem<R>> = items.iter().map(AsRef::as_ref).collect();
    Menu::with_items(app, &refs)
}

/// Creates the icon, hidden when `disabledTrayIcon` is set.
pub(crate) fn create<R: Runtime>(app: &AppHandle<R>, settings: &Settings) -> tauri::Result<()> {
    app.manage(TrayState::default());
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip(TOOLTIP)
        .menu(&build_menu(app, settings)?)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| on_menu(app, event.id().as_ref()))
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                actions::capture_gui(tray.app_handle(), GuiRequest::default());
            }
        });
    match app.default_window_icon() {
        Some(icon) => builder = builder.icon(icon.clone()),
        None => log::error!("no application icon for the tray"),
    }
    let tray = builder.build(app)?;
    let visible = !settings.disabled_tray_icon();
    tray.set_visible(visible)?;
    app.state::<TrayState>()
        .visible
        .store(visible, Ordering::SeqCst);

    if visible && settings.show_startup_launch_message() {
        notify::tray_message(app, "Phramer", HELLO);
    }
    Ok(())
}

/// After a settings change: shows or hides the icon and rebuilds the menu.
pub(crate) fn refresh<R: Runtime>(app: &AppHandle<R>, settings: &Settings) -> tauri::Result<()> {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return Ok(());
    };
    let visible = !settings.disabled_tray_icon();
    tray.set_visible(visible)?;
    app.state::<TrayState>()
        .visible
        .store(visible, Ordering::SeqCst);
    show_update(app, &tray)?;
    tray.set_menu(Some(build_menu(app, settings)?))
}

/// `TrayIcon::showUpdateBadge`: the badged icon and its tooltip while a
/// newer version is known.
fn show_update<R: Runtime>(
    app: &AppHandle<R>,
    tray: &tauri::tray::TrayIcon<R>,
) -> tauri::Result<()> {
    let (Some(version), Some(icon)) = (updater::available_version(), app.default_window_icon())
    else {
        return Ok(());
    };
    let badged = updater::badge(icon.rgba(), icon.width(), icon.height());
    tray.set_icon(Some(tauri::image::Image::new_owned(
        badged,
        icon.width(),
        icon.height(),
    )))?;
    tray.set_tooltip(Some(format!("{TOOLTIP} \u{2014} update available ({version})")))
}

/// The window behind the icon while the icon is shown, for balloons.
pub(crate) fn visible_window<R: Runtime>(app: &AppHandle<R>) -> Option<isize> {
    let shown = app
        .try_state::<TrayState>()
        .is_some_and(|state| state.visible.load(Ordering::SeqCst));
    if !shown {
        return None;
    }
    app.tray_by_id(TRAY_ID)?
        .with_inner_tray_icon(|inner| inner.window_handle() as isize)
        .ok()
}

fn on_menu<R: Runtime>(app: &AppHandle<R>, id: &str) {
    let menu_capture = || GuiRequest {
        delay_ms: MENU_CAPTURE_DELAY_MS,
        ..GuiRequest::default()
    };
    match id {
        "capture" => actions::capture_gui(app, menu_capture()),
        "record_video" => actions::record_video(app),
        "open_editor" => actions::open_editor(app),
        "open_launcher" => actions::open_launcher(app),
        "open_save_path" => actions::open_save_path(app),
        "show_last_capture" => actions::show_last_capture_in_folder(app),
        "configuration" => actions::open_configuration(app),
        "check_for_updates" => actions::check_for_updates(app),
        "whats_new" => actions::show_whats_new(app),
        "about" => actions::show_about(app),
        "restart" => restart::request(app),
        "quit" => app.exit(0),
        other => match other
            .strip_prefix("screen:")
            .and_then(|index| index.parse().ok())
        {
            Some(monitor) => actions::capture_gui(
                app,
                GuiRequest {
                    monitor: Some(monitor),
                    ..menu_capture()
                },
            ),
            None => log::warn!("unknown tray menu entry {other}"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout(entries: &[Entry]) -> Vec<String> {
        entries
            .iter()
            .map(|entry| match entry {
                Entry::Item { text, enabled, .. } if !enabled => format!("({text})"),
                Entry::Item { text, .. } => text.clone(),
                Entry::Separator => "-".to_owned(),
                Entry::Submenu { text, items } => format!("{text} {:?}", layout(items)),
            })
            .collect()
    }

    #[test]
    fn default_menu_matches_15_0_0() {
        let flags = MenuFlags {
            check_for_updates: true,
            monitors: vec!["Monitor 1: A (1920x1080)".into()],
            ..MenuFlags::default()
        };
        assert_eq!(
            layout(&menu_entries(&flags)),
            [
                "&Take Screenshot",
                "Open &Editor",
                "&Open Launcher",
                "-",
                "&Open Save Path",
                "(&Show Last Capture in Folder)",
                "-",
                "&Configuration",
                "-",
                "&What's New",
                "&About",
                "-",
                "&Restart",
                "&Quit",
            ]
        );
    }

    #[cfg(not(feature = "store"))]
    #[test]
    fn every_optional_entry_in_place() {
        let flags = MenuFlags {
            video_capture_enabled: true,
            check_for_updates: false,
            monitors: vec!["Monitor 1: A (1920x1080)".into(), "Monitor 2: B (1280x1024)".into()],
            last_capture_available: true,
            update: None,
        };
        assert_eq!(
            layout(&menu_entries(&flags)),
            [
                "&Take Screenshot",
                "&Record Video",
                "Open &Editor",
                "&Open Launcher",
                "Select Screen [\"Monitor 1: A (1920x1080)\", \"Monitor 2: B (1280x1024)\"]",
                "-",
                "&Open Save Path",
                "&Show Last Capture in Folder",
                "-",
                "&Configuration",
                "-",
                "Check for updates",
                "&What's New",
                "&About",
                "-",
                "&Restart",
                "&Quit",
            ]
        );
    }

    #[cfg(not(feature = "store"))]
    #[test]
    fn a_found_update_sits_above_about() {
        for check_for_updates in [true, false] {
            let flags = MenuFlags {
                check_for_updates,
                update: Some("16.0.1".into()),
                ..MenuFlags::default()
            };
            let entries = layout(&menu_entries(&flags));
            let at = |text: &str| entries.iter().position(|entry| entry == text);
            assert_eq!(at("Check for updates"), None);
            let update = at("Download version 16.0.1").expect("update entry");
            assert_eq!(entries[update - 1], "&What's New");
            assert_eq!(entries[update + 1], "&About");
        }
    }

    #[test]
    fn screen_entries_carry_the_monitor_number() {
        let flags = MenuFlags {
            monitors: vec!["a".into(), "b".into()],
            ..MenuFlags::default()
        };
        let entries = menu_entries(&flags);
        let Some(Entry::Submenu { items, .. }) =
            entries.iter().find(|e| matches!(e, Entry::Submenu { .. }))
        else {
            panic!("no screen menu");
        };
        assert_eq!(
            items[1],
            Entry::Item {
                id: "screen:1".into(),
                text: "b".into(),
                enabled: true
            }
        );
    }
}
