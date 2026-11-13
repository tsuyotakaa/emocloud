#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_opener::OpenerExt;

mod presence;

use presence::TrackInfo;

const HOME: &str = "https://soundcloud.com/";

/// Читает панель плеера SoundCloud и отправляет трек в Discord.
const PRESENCE_SCRIPT: &str = include_str!("presence.js");

// Обычный Chrome-UA: сайт и вход не должны видеть, что это WebView2.
const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) \
AppleWebKit/537.36 (KHTML, like Gecko) \
Chrome/141.0.0.0 Safari/537.36";

fn is_soundcloud(url: &tauri::Url) -> bool {
    match url.host_str() {
        Some(host) => {
            let host = host.to_ascii_lowercase();
            host == "soundcloud.com"
                || host.ends_with(".soundcloud.com")
                || host == "sndcdn.com"
                || host.ends_with(".sndcdn.com")
        }
        None => false,
    }
}

fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn toggle_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        match window.is_visible() {
            Ok(true) => {
                let _ = window.hide();
            }
            _ => show_main(app),
        }
    }
}

fn quit_app(app: &AppHandle, exiting: &AtomicBool) {
    exiting.store(true, Ordering::SeqCst);
    // Сайт вешает beforeunload, поэтому окно сносим принудительно.
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.destroy();
    }
    app.exit(0);
}

/// Команда доступна только странице SoundCloud (см. capabilities/soundcloud.json).
#[tauri::command]
fn update_presence(track: Option<TrackInfo>) {
    presence::push(track);
}

fn build_tray(app: &AppHandle, exiting: Arc<AtomicBool>) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Открыть emocloud", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Выход", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &quit])?;

    let exiting_for_menu = exiting.clone();

    TrayIconBuilder::with_id("main-tray")
        .icon(tauri::include_image!("assets/tray.png"))
        .tooltip("emocloud")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "open" => show_main(app),
            "quit" => quit_app(app, &exiting_for_menu),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_main(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            show_main(app);
        }))
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![update_presence])
        .setup(|app| {
            let exiting = Arc::new(AtomicBool::new(false));

            presence::start();

            let profile_dir = app
                .path()
                .app_local_data_dir()
                .map(|dir| dir.join("profile"))
                .unwrap_or_else(|_| std::path::PathBuf::from("profile"));

            let navigation_app = app.handle().clone();
            let start_url: tauri::Url = HOME.parse().expect("valid SoundCloud URL");

            let window = WebviewWindowBuilder::new(app, "main", WebviewUrl::External(start_url))
                .title("emocloud")
                .inner_size(1300.0, 860.0)
                .min_inner_size(960.0, 620.0)
                .user_agent(UA)
                .additional_browser_args(
                    "--autoplay-policy=no-user-gesture-required \
                     --disable-background-timer-throttling \
                     --disable-renderer-backgrounding \
                     --disable-backgrounding-occluded-windows",
                )
                .data_directory(profile_dir)
                .initialization_script(PRESENCE_SCRIPT)
                .on_navigation(move |url| {
                    if is_soundcloud(url) || matches!(url.scheme(), "about" | "data" | "blob") {
                        true
                    } else {
                        let _ = navigation_app.opener().open_url(url.as_str(), None::<&str>);
                        false
                    }
                })
                .build()?;

            let exiting_for_close = exiting.clone();
            let app_for_close = app.handle().clone();

            window.on_window_event(move |event| {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    if !exiting_for_close.load(Ordering::SeqCst) {
                        // Крестик прячет окно в трей, музыка продолжает играть.
                        api.prevent_close();
                        if let Some(window) = app_for_close.get_webview_window("main") {
                            let _ = window.hide();
                        }
                    }
                }
            });

            build_tray(app.handle(), exiting)?;

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running emocloud");
}
