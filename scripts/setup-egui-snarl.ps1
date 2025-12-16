# Setup script for patched egui-snarl
# This clones egui-snarl and patches it for scroll-to-zoom support

$snarlDir = Join-Path $PSScriptRoot "..\libs\egui-snarl"
$patchFile = Join-Path $PSScriptRoot "egui-snarl-scroll-zoom.patch"

# Check if already set up
if (Test-Path $snarlDir) {
    Write-Host "egui-snarl already exists at $snarlDir"
    Write-Host "To re-patch, delete the folder and run this script again."
    exit 0
}

# Clone egui-snarl (latest version)
Write-Host "Cloning egui-snarl..."
git clone --depth 1 https://github.com/zakarumych/egui-snarl.git $snarlDir

if (-not (Test-Path $snarlDir)) {
    Write-Error "Failed to clone egui-snarl"
    exit 1
}

# Apply the patch
Write-Host "Applying scroll-to-zoom patch..."

# Read the ui.rs file
$uiRsPath = Join-Path $snarlDir "src\ui.rs"
$content = Get-Content $uiRsPath -Raw

# The original code we're replacing (Scene::register_pan_and_zoom)
$originalCode = @"
    clamp_scale(&mut to_global, min_scale, max_scale, ui_rect);

    let mut snarl_resp = ui.response();
    Scene::new()
        .zoom_range(min_scale..=max_scale)
        .register_pan_and_zoom(&ui, &mut snarl_resp, &mut to_global);

    if snarl_resp.changed() {
        ui.ctx().request_repaint();
    }
"@

# The patched code (scroll-to-zoom without Ctrl)
$patchedCode = @"
    clamp_scale(&mut to_global, min_scale, max_scale, ui_rect);

    let mut snarl_resp = ui.response();
    
    // CUSTOM SCROLL-TO-ZOOM: Instead of using Scene::register_pan_and_zoom which uses Ctrl+scroll for zoom,
    // we manually handle scroll as zoom directly (no Ctrl required)
    {
        let scroll_delta = ui.ctx().input(|i| i.raw_scroll_delta);
        let zoom_delta = ui.ctx().input(|i| i.zoom_delta());
        let pointer_in_canvas = ui.ctx().input(|i| {
            i.pointer.hover_pos().map(|pos| ui_rect.contains(pos)).unwrap_or(false)
        });
        
        // Handle scroll wheel as zoom (not pan) - works anywhere in the canvas, including over nodes
        if scroll_delta.y.abs() > 0.1 && pointer_in_canvas {
            let zoom_factor = if scroll_delta.y > 0.0 { 1.1 } else { 0.9 };
            let pointer_pos = ui.ctx().input(|i| i.pointer.hover_pos()).unwrap_or(ui_rect.center());
            
            // Apply zoom centered on pointer position
            let new_scale = (to_global.scaling * zoom_factor).clamp(min_scale, max_scale);
            if new_scale != to_global.scaling {
                // Zoom towards the pointer: adjust translation so pointer stays at same graph position
                let scale_ratio = new_scale / to_global.scaling;
                to_global.translation = pointer_pos.to_vec2() + (to_global.translation - pointer_pos.to_vec2()) * scale_ratio;
                to_global.scaling = new_scale;
                snarl_resp.mark_changed();
            }
        }
        
        // Also handle pinch zoom gestures (zoom_delta from touch)
        if zoom_delta != 1.0 && pointer_in_canvas {
            let pointer_pos = ui.ctx().input(|i| i.pointer.hover_pos()).unwrap_or(ui_rect.center());
            let new_scale = (to_global.scaling * zoom_delta).clamp(min_scale, max_scale);
            if new_scale != to_global.scaling {
                let scale_ratio = new_scale / to_global.scaling;
                to_global.translation = pointer_pos.to_vec2() + (to_global.translation - pointer_pos.to_vec2()) * scale_ratio;
                to_global.scaling = new_scale;
                snarl_resp.mark_changed();
            }
        }
        
        // Handle drag for panning (left mouse button, middle mouse button, or right mouse button)
        if snarl_resp.dragged_by(PointerButton::Primary) || snarl_resp.dragged_by(PointerButton::Middle) || snarl_resp.dragged_by(PointerButton::Secondary) {
            to_global.translation += snarl_resp.drag_delta();
            snarl_resp.mark_changed();
        }
    }

    if snarl_resp.changed() {
        ui.ctx().request_repaint();
    }
"@

# Replace the code
$newContent = $content -replace [regex]::Escape($originalCode), $patchedCode

if ($newContent -eq $content) {
    Write-Warning "Could not find the exact code to patch. egui-snarl may have updated."
    Write-Warning "Please check libs/egui-snarl/src/ui.rs manually around line 989."
    exit 1
}

# Also remove unused Scene import to avoid warning
$newContent = $newContent -replace "Pos2, Rect, Scene, Sense,", "Pos2, Rect, Sense,"

# Write the patched file
Set-Content -Path $uiRsPath -Value $newContent -NoNewline

Write-Host "Patch applied successfully!"
Write-Host "egui-snarl is ready at: $snarlDir"
