use std::path::PathBuf;

use crate::config::ThemeMode;
use crate::gui::locale::LocaleText;
use crate::{APP, MIN_WINDOW_HEIGHT, MIN_WINDOW_WIDTH, WINDOW_HEIGHT, WINDOW_WIDTH, gui};
use tray_icon::menu::{CheckMenuItem, Menu, MenuItem};

fn native_options_for_wgpu(viewport: eframe::egui::ViewportBuilder) -> eframe::NativeOptions {
    let mut options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };

    if let eframe::egui_wgpu::WgpuSetup::CreateNew(create_new) =
        &mut options.wgpu_options.wgpu_setup
    {
        create_new.instance_descriptor.backends = eframe::wgpu::Backends::PRIMARY;
    }

    options
}

pub(super) fn run(
    screen_record_wry_smoke: bool,
    pending_file_path: Option<PathBuf>,
) -> eframe::Result<()> {
    let initial_config = APP.lock().unwrap().config.clone();

    let tray_locale = LocaleText::get(&initial_config.ui_language);
    let tray_menu = Menu::new();

    let has_favorites = initial_config.presets.iter().any(|p| p.is_favorite);
    let favorite_bubble_text = if has_favorites {
        tray_locale.shell.tray_favorite_bubble
    } else {
        tray_locale.shell.tray_favorite_bubble_disabled
    };
    let tray_favorite_bubble_item = CheckMenuItem::with_id(
        "1003",
        favorite_bubble_text,
        true,
        initial_config.show_favorite_bubble,
        None,
    );

    let tray_settings_item = MenuItem::with_id("1002", tray_locale.shell.tray_settings, true, None);
    let tray_quit_item = MenuItem::with_id("1001", tray_locale.shell.tray_quit, true, None);
    let _ = tray_menu.append(&tray_favorite_bubble_item);
    let _ = tray_menu.append(&tray_settings_item);
    let _ = tray_menu.append(&tray_quit_item);

    let mut viewport_builder = eframe::egui::ViewportBuilder::default()
        .with_inner_size([WINDOW_WIDTH, WINDOW_HEIGHT])
        .with_min_inner_size([MIN_WINDOW_WIDTH, MIN_WINDOW_HEIGHT])
        .with_resizable(true)
        .with_visible(false)
        .with_transparent(false)
        .with_decorations(true);

    let system_dark = gui::utils::is_system_in_dark_mode();
    let effective_dark = match initial_config.theme_mode {
        ThemeMode::Dark => true,
        ThemeMode::Light => false,
        ThemeMode::System => system_dark,
    };

    let icon_data = crate::icon_gen::get_window_icon(effective_dark);
    viewport_builder = viewport_builder.with_icon(std::sync::Arc::new(icon_data));

    let options = native_options_for_wgpu(viewport_builder);

    crate::log_info!("[Main] Starting eframe with wgpu renderer");
    eframe::run_native(
        "Screen Goated Toolbox (SGT by nganlinh4)",
        options,
        Box::new(move |cc| {
            gui::configure_fonts(&cc.egui_ctx);
            *gui::GUI_CONTEXT.lock().unwrap() = Some(cc.egui_ctx.clone());
            gui::theme::AppTheme::apply_global_style(&cc.egui_ctx, effective_dark);
            gui::utils::update_window_icon_native(effective_dark);

            if screen_record_wry_smoke {
                std::thread::spawn(|| {
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    crate::log_info!("[WrySmoke] Opening SGT Record window");
                    crate::overlay::screen_record::show_screen_record();
                });
            }

            Ok(Box::new(gui::SettingsApp::new(gui::SettingsAppInit {
                config: initial_config,
                app_state: APP.clone(),
                tray_menu,
                tray_settings_item,
                tray_quit_item,
                tray_favorite_bubble_item,
                ctx: cc.egui_ctx.clone(),
                pending_file_path,
            })))
        }),
    )
}
