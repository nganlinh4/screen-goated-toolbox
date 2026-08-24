//! Shared headless-egui test boundaries.

pub(crate) fn run_ui(
    context: &eframe::egui::Context,
    input: eframe::egui::RawInput,
    run: impl FnMut(&mut eframe::egui::Ui),
) -> eframe::egui::FullOutput {
    let mut output = context.run_ui(input, run);
    // Headless tests inspect layout state but have no renderer to upload or
    // release textures. Explicitly consume that renderer-owned work.
    output.textures_delta.clear();
    output
}
