use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(
        std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set by cargo"),
    );

    // Иконка вшивается в exe во время сборки. Без этих строк Cargo не заметит,
    // что картинку заменили, и exe останется со старой иконкой.
    println!("cargo:rerun-if-changed=icons/icon.ico");
    println!("cargo:rerun-if-changed=icons/icon.png");
    println!("cargo:rerun-if-changed=assets/tray.png");

    let mut attributes = tauri_build::Attributes::new().app_manifest(
        // Генерирует разрешение allow-update_presence для своей команды.
        tauri_build::AppManifest::new().commands(&["update_presence"]),
    );

    // windres (GNU-сборка) не умеет открывать файлы по путям с не-ASCII символами.
    // Если проект лежит в такой папке, кладём иконку в ASCII-кэш и указываем её.
    if !manifest_dir.to_string_lossy().is_ascii() {
        let cache = std::env::var("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| std::env::temp_dir())
            .join("emocloud-build");

        let _ = std::fs::create_dir_all(&cache);
        let icon = cache.join("icon.ico");

        if std::fs::copy(manifest_dir.join("icons").join("icon.ico"), &icon).is_ok() {
            attributes = attributes
                .windows_attributes(tauri_build::WindowsAttributes::new().window_icon_path(icon));
        }
    }

    tauri_build::try_build(attributes).expect("failed to run tauri-build");
}
