#[cfg(feature = "desktop")]
pub(crate) fn build_menu_bar() -> dioxus::desktop::muda::Menu {
    use dioxus::desktop::muda::{
        Menu, MenuItem, PredefinedMenuItem, Submenu,
        accelerator::{Accelerator, Code, Modifiers},
    };

    let menu = Menu::new();

    let file_menu = Submenu::new("File", true);
    file_menu
        .append_items(&[
            &MenuItem::with_id(
                "open-pdf",
                "Open PDF\u{2026}",
                true,
                Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyO)),
            ),
            &PredefinedMenuItem::separator(),
            &MenuItem::with_id(
                "import-bibtex",
                "Import BibTeX\u{2026}",
                true,
                Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyI)),
            ),
            &MenuItem::with_id(
                "export-bibtex",
                "Export BibTeX\u{2026}",
                true,
                Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyE)),
            ),
            &PredefinedMenuItem::separator(),
            &MenuItem::with_id(
                "close-tab",
                "Close Tab",
                true,
                Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyW)),
            ),
        ])
        .unwrap();

    let edit_menu = Submenu::new("Edit", true);
    edit_menu
        .append_items(&[
            &PredefinedMenuItem::undo(None),
            &PredefinedMenuItem::redo(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::cut(None),
            &PredefinedMenuItem::copy(None),
            &PredefinedMenuItem::paste(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::select_all(None),
            &PredefinedMenuItem::separator(),
            &MenuItem::with_id(
                "find",
                "Find\u{2026}",
                true,
                Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyF)),
            ),
        ])
        .unwrap();

    let view_menu = Submenu::new("View", true);
    view_menu
        .append_items(&[
            &MenuItem::with_id(
                "show-library",
                "Library",
                true,
                Some(Accelerator::new(Some(Modifiers::SUPER), Code::Digit1)),
            ),
            &PredefinedMenuItem::separator(),
            &MenuItem::with_id(
                "new-collection",
                "New Collection",
                true,
                Some(Accelerator::new(Some(Modifiers::SUPER), Code::KeyN)),
            ),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::fullscreen(None),
        ])
        .unwrap();

    let window_menu = Submenu::new("Window", true);
    window_menu
        .append_items(&[
            &PredefinedMenuItem::minimize(None),
            &PredefinedMenuItem::maximize(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::hide(None),
            &PredefinedMenuItem::hide_others(None),
            &PredefinedMenuItem::show_all(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::close_window(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::quit(None),
        ])
        .unwrap();

    menu.append_items(&[&file_menu, &edit_menu, &view_menu, &window_menu])
        .unwrap();

    if cfg!(debug_assertions) {
        let help_menu = Submenu::new("Help", true);
        help_menu
            .append_items(&[
                &MenuItem::with_id(
                    "dioxus-toggle-dev-tools",
                    "Toggle Developer Tools",
                    true,
                    None,
                ),
                &MenuItem::with_id("dioxus-float-top", "Float on Top", true, None),
            ])
            .unwrap();
        menu.append(&help_menu).unwrap();

        #[cfg(target_os = "macos")]
        help_menu.set_as_help_menu_for_nsapp();
    }

    #[cfg(target_os = "macos")]
    window_menu.set_as_windows_menu_for_nsapp();

    menu
}

#[cfg(feature = "desktop")]
pub(crate) fn launch_desktop(config: &crate::sync::engine::SyncConfig) {
    use dioxus::desktop::tao::dpi::LogicalSize;
    use dioxus::desktop::tao::window::WindowBuilder;

    let menu = build_menu_bar();

    let window = WindowBuilder::new()
        .with_title("Rotero")
        .with_inner_size(LogicalSize::new(1200.0, 800.0))
        .with_min_inner_size(LogicalSize::new(600.0, 400.0))
        .with_theme(None);

    let data_dir = config.effective_library_path();
    dioxus::LaunchBuilder::new()
        .with_cfg(
            dioxus::desktop::Config::default()
                .with_disable_context_menu(true)
                .with_window(window)
                .with_menu(menu)
                .with_background_color(if config.ui.dark_mode {
                    (15, 23, 42, 255) // slate-900
                } else {
                    (255, 255, 255, 255)
                })
                .with_custom_protocol("rotero-cache".to_string(), move |_webview_id, req| {
                    let uri = req.uri().to_string();
                    let path = uri.strip_prefix("rotero-cache://").unwrap_or(&uri);
                    // Strip leading slashes to prevent absolute path interpretation
                    let path = path.trim_start_matches('/');
                    let cache_dir = data_dir.join("cache");
                    let file_path = cache_dir.join(path);

                    // Prevent path traversal: canonicalize and verify the path is within cache_dir
                    let body = match (file_path.canonicalize(), cache_dir.canonicalize()) {
                        (Ok(canonical), Ok(cache_canonical)) if canonical.starts_with(&cache_canonical) => {
                            std::fs::read(&canonical).unwrap_or_default()
                        }
                        _ => Vec::new(),
                    };

                    let mime = if file_path.extension().and_then(|e| e.to_str()) == Some("png") {
                        "image/png"
                    } else {
                        "image/jpeg"
                    };
                    dioxus::desktop::wry::http::Response::builder()
                        .header("Content-Type", mime)
                        .body(std::borrow::Cow::Owned(body))
                        .unwrap()
                }),
        )
        .launch(crate::app::App);
}
