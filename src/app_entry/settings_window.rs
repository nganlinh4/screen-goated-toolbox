use std::path::PathBuf;

use super::arguments::CreationUiTestApp;
use crate::config::ThemeMode;
use crate::{APP, MIN_WINDOW_HEIGHT, MIN_WINDOW_WIDTH, WINDOW_HEIGHT, WINDOW_WIDTH, gui};

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
        create_new
            .instance_descriptor
            .backend_options
            .dx12
            .presentation_system = eframe::wgpu::Dx12SwapchainKind::DxgiFromVisual;
    }
    options.wgpu_options.surface.present_mode = eframe::wgpu::PresentMode::AutoNoVsync;
    options.wgpu_options.surface.desired_maximum_frame_latency = Some(1);

    options.wgpu_options.on_device_recovery = std::sync::Arc::new(|message| {
        crate::debug_log::log_debug(&format!("[WGPU recovery] {message}"));
    });

    options
}

pub(super) fn run(
    screen_record_wry_smoke: bool,
    creation_ui_test: Option<CreationUiTestApp>,
    screen_translate_ui_test: bool,
    screen_translate_ui_test_image: Option<PathBuf>,
    screen_translate_lab_queue: Option<PathBuf>,
    pending_file_path: Option<PathBuf>,
) -> eframe::Result<()> {
    let initial_config = APP.lock().unwrap().config.clone();

    let (available_width, available_height) = crate::gui::window_state::available_size_points();
    let mut viewport_builder = eframe::egui::ViewportBuilder::default()
        .with_inner_size([
            WINDOW_WIDTH.min(available_width),
            WINDOW_HEIGHT.min(available_height),
        ])
        .with_min_inner_size([
            MIN_WINDOW_WIDTH.min(available_width),
            MIN_WINDOW_HEIGHT.min(available_height),
        ])
        .with_resizable(true)
        .with_visible(false)
        .with_transparent(false)
        .with_decorations(true);
    viewport_builder = crate::gui::window_state::restore(viewport_builder);

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
            // Everything below may address the main window; record it first.
            gui::app::remember_main_window(cc);
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
            if let Some(app) = creation_ui_test {
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    crate::log_info!("[CreationUiTest] Opening creation window");
                    match app {
                        CreationUiTestApp::ThreeD => {
                            crate::overlay::three_d_generator::show_three_d_generator()
                        }
                        CreationUiTestApp::Svg => crate::overlay::image_to_svg::show_image_to_svg(),
                    }
                });
            }
            if screen_translate_ui_test {
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    crate::log_info!("[ScreenTranslateUiTest] Running Screen Translate");
                    crate::overlay::screen_translate::run_ui_test(screen_translate_ui_test_image);
                });
            }
            if let Some(queue) = screen_translate_lab_queue {
                crate::overlay::screen_translate::run_lab_queue(queue);
            }

            Ok(Box::new(gui::SettingsApp::new(gui::SettingsAppInit {
                config: initial_config,
                app_state: APP.clone(),
                ctx: cc.egui_ctx.clone(),
                pending_file_path,
            })))
        }),
    )
}
