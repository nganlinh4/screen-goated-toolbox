use eframe::egui;

use crate::runtime_support::{self, RuntimeArch, WebView2InstallStatus};

fn status_color(status: &str) -> egui::Color32 {
    match status {
        "Supported" => egui::Color32::from_rgb(34, 139, 34),
        "Missing dependency" => egui::Color32::from_rgb(210, 140, 40),
        "Unsupported" => egui::Color32::from_rgb(200, 70, 70),
        _ => egui::Color32::from_rgb(180, 130, 50),
    }
}

fn support_rows() -> Vec<(&'static str, &'static str, String)> {
    let env = runtime_support::environment_info();
    let webview = match runtime_support::current_webview2_status() {
        WebView2InstallStatus::Installed => (
            "Supported",
            "WebView2 Runtime is installed, so web-based overlays can initialize.",
        ),
        WebView2InstallStatus::Installing { .. } => (
            "Missing dependency",
            "WebView2 Runtime install is in progress.",
        ),
        WebView2InstallStatus::Missing => (
            "Missing dependency",
            "Install Microsoft Edge WebView2 Runtime to use web-based overlays and tools.",
        ),
        WebView2InstallStatus::Error(_) => (
            "Missing dependency",
            "WebView2 Runtime is not ready. Re-run the install from Downloaded Tools.",
        ),
    };

    let qwen = runtime_support::supports_qwen3_local_runtime();
    let qwen_status = if qwen.is_supported() {
        "Supported"
    } else {
        "Unsupported"
    };

    let parakeet = if env.native_arch == RuntimeArch::Arm64 {
        (
            "VM-dependent",
            "Local DirectML inference compiles for ARM64, but real behavior still depends on the VM GPU stack.",
        )
    } else {
        (
            "Supported",
            "Architecture-aware runtime downloads are wired up for the local Parakeet path.",
        )
    };

    let recorder = if env.native_arch == RuntimeArch::Arm64 {
        (
            "VM-dependent",
            "Recorder UI is supportable, but capture, GPU export, and encoder behavior still depend on the VM graphics stack.",
        )
    } else {
        (
            "Supported",
            "Recorder entrypoints are available; GPU/export support still depends on the machine graphics stack.",
        )
    };

    vec![
        (
            "General app launch",
            "Supported",
            "Portable x64/arm64 packaging and runtime selection are now architecture-aware."
                .to_string(),
        ),
        ("WebView2 overlays", webview.0, webview.1.to_string()),
        ("Parakeet local AI", parakeet.0, parakeet.1.to_string()),
        ("Screen recorder", recorder.0, recorder.1.to_string()),
        ("Qwen3 local AI", qwen_status, qwen.details),
    ]
}

pub(crate) fn render_support_report_card(ui: &mut egui::Ui) {
    let env = runtime_support::environment_info();
    let emulation_text = if env.is_emulated { "Yes" } else { "No" };

    ui.group(|ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Environment Support").strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(runtime_support::architecture_summary())
                        .color(egui::Color32::GRAY),
                );
            });
        });
        ui.add_space(4.0);
        ui.label(format!(
            "Process arch: {} | Native arch: {} | Emulated: {}",
            env.process_arch, env.native_arch, emulation_text
        ));
        ui.add_space(6.0);

        egui::Grid::new("environment_support_grid")
            .num_columns(3)
            .spacing([12.0, 6.0])
            .show(ui, |ui| {
                for (feature, status, details) in support_rows() {
                    ui.label(egui::RichText::new(feature).strong());
                    ui.label(egui::RichText::new(status).color(status_color(status)));
                    ui.label(details);
                    ui.end_row();
                }
            });
    });
}

pub(crate) fn render_support_report_compact(ui: &mut egui::Ui) {
    ui.group(|ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Support Matrix").strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(runtime_support::architecture_summary())
                        .color(egui::Color32::GRAY),
                );
            });
        });
        ui.add_space(4.0);
        for (feature, status, details) in support_rows() {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(feature).strong());
                ui.label(egui::RichText::new(status).color(status_color(status)));
            });
            ui.label(
                egui::RichText::new(details)
                    .small()
                    .color(egui::Color32::GRAY),
            );
            ui.add_space(4.0);
        }
    });
}
