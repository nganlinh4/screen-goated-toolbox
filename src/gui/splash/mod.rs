// --- SPLASH SCREEN MODULE ---
// Animated splash screen with 3D voxel effects, procedural audio, and theme switching.

mod audio;
mod escape_overlay;
mod math;
mod palette;
mod pixel;
mod render;
mod scene;

use palette::Palette;

use audio::SplashAudio;
use eframe::egui::{self, Color32, Pos2, Vec2};
use escape_overlay::{EscapeCircle, EscapeOverlay};
use math::Vec3;
use scene::{Cloud, MoonFeature, Voxel};
use std::cell::RefCell;
use std::sync::{Arc, Mutex};

// --- CONFIGURATION ---
const ANIMATION_DURATION: f32 = 3.6;
const START_TRANSITION: f32 = 0.8;
const EXIT_DURATION: f32 = 1.6;

// Theme-independent voxel colours (per-palette colours live in `palette.rs`):
// `C_WHITE` = the random white-sprinkle voxels, `C_SHADOW` = debris.
const C_WHITE: Color32 = Color32::from_rgb(240, 245, 255);
const C_SHADOW: Color32 = Color32::from_rgb(20, 20, 30);

type DrawListEntry = (f32, Pos2, f32, Color32, bool, bool);

pub struct SplashScreen {
    start_time: f64,
    voxels: Vec<Voxel>,
    clouds: Vec<Cloud>,
    moon_features: Vec<MoonFeature>,
    init_done: bool,
    mouse_influence: Vec2,
    mouse_world_pos: Vec3,
    loading_text: String,
    exit_start_time: Option<f64>,
    is_dark: bool,
    audio: Arc<Mutex<Option<SplashAudio>>>,
    has_played_impact: bool,
    draw_list: RefCell<Vec<DrawListEntry>>,
    // Escape overlay — separate transparent window for voxels that fly beyond the main window
    escape_overlay: RefCell<Option<EscapeOverlay>>,
    // Pixel-art framebuffer texture.
    pixel_tex: RefCell<Option<egui::TextureHandle>>,
    // Randomly-rolled atmosphere palette (theme-coherent) + its seed.
    palette: Palette,
    palette_seed: u64,
}

pub enum SplashStatus {
    Ongoing,
    Finished,
}

impl SplashScreen {
    pub fn new(ctx: &egui::Context) -> Self {
        let is_dark = ctx.global_style().visuals.dark_mode;
        // Roll a random, theme-coherent atmosphere for this launch.
        let palette_seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E37_79B9_7F4A_7C15);
        let palette = palette::pick(is_dark, palette_seed);
        crate::log_info!("[Splash] atmosphere: {}", palette.name);
        let audio_container = Arc::new(Mutex::new(None));
        let audio_container_clone = audio_container.clone();

        std::thread::spawn(move || {
            if let Some(audio) = SplashAudio::new()
                && let Ok(mut lock) = audio_container_clone.lock()
            {
                *lock = Some(audio);
            }
        });

        let mut slf = Self {
            start_time: ctx.input(|i| i.time),
            voxels: Vec::with_capacity(600),
            clouds: Vec::with_capacity(20),
            moon_features: Vec::with_capacity(100),
            init_done: false,
            mouse_influence: Vec2::ZERO,
            mouse_world_pos: Vec3::ZERO,
            loading_text: "TRANSLATING...".to_string(),
            exit_start_time: None,
            is_dark,
            audio: audio_container,
            has_played_impact: false,
            draw_list: RefCell::new(Vec::with_capacity(600)),
            escape_overlay: RefCell::new(None),
            pixel_tex: RefCell::new(None),
            palette,
            palette_seed,
        };

        slf.init_scene();
        slf
    }

    pub fn reset_timer(&mut self, ctx: &egui::Context) {
        self.start_time = ctx.input(|i| i.time);
    }

    fn init_scene(&mut self) {
        scene::init_scene(
            &mut self.voxels,
            &mut self.clouds,
            &mut self.moon_features,
            self.palette.accent_primary,
            self.palette.accent_secondary,
        );
        self.init_done = true;
    }

    pub fn update(&mut self, ctx: &egui::Context) -> SplashStatus {
        let was_exiting = self.exit_start_time.is_some();

        let status = render::update(render::SplashUpdateContext {
            ctx,
            start_time: self.start_time,
            exit_start_time: &mut self.exit_start_time,
            voxels: &mut self.voxels,
            clouds: &mut self.clouds,
            mouse_influence: &mut self.mouse_influence,
            mouse_world_pos: &mut self.mouse_world_pos,
            loading_text: &mut self.loading_text,
            is_dark: &mut self.is_dark,
            audio: &self.audio,
            has_played_impact: &mut self.has_played_impact,
        });

        // Theme toggled mid-splash → roll a fresh palette of the new mood and
        // recolour the existing voxels in place (keeping their positions).
        if self.is_dark != self.palette.is_night {
            let old = self.palette;
            self.palette_seed = self
                .palette_seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1);
            self.palette = palette::pick(self.is_dark, self.palette_seed);
            crate::log_info!("[Splash] atmosphere: {}", self.palette.name);
            for v in self.voxels.iter_mut() {
                if v.is_debris {
                    continue;
                }
                if v.color == old.accent_primary {
                    v.color = self.palette.accent_primary;
                } else if v.color == old.accent_secondary {
                    v.color = self.palette.accent_secondary;
                }
            }
        }

        // Create escape overlay on first frame of exit
        if self.exit_start_time.is_some() && !was_exiting {
            *self.escape_overlay.borrow_mut() = EscapeOverlay::new();
        }

        // Destroy overlay when splash finishes
        if matches!(status, SplashStatus::Finished) {
            *self.escape_overlay.borrow_mut() = None;
        }

        status
    }

    pub fn paint(&self, ctx: &egui::Context, _theme_mode: &crate::config::ThemeMode) -> bool {
        // The pixel path fills draw_list with the voxels' logical positions, so
        // the escape-overlay trick below works.
        let result = pixel::paint_pixel(pixel::PixelPaintContext {
            ctx,
            start_time: self.start_time,
            exit_start_time: self.exit_start_time,
            palette: self.palette,
            voxels: &self.voxels,
            moon_features: &self.moon_features,
            clouds: &self.clouds,
            mouse_influence: self.mouse_influence,
            loading_text: &self.loading_text,
            draw_list: &self.draw_list,
            tex: &self.pixel_tex,
        });

        // After paint fills draw_list, send escaped voxels to the overlay.
        if self.escape_overlay.borrow().is_some() {
            self.update_escape_overlay(ctx);
        }

        result
    }

    fn update_escape_overlay(&self, ctx: &egui::Context) {
        let viewport = ctx.input(|i| i.viewport().clone());
        let inner = viewport
            .inner_rect
            .unwrap_or(eframe::egui::Rect::from_min_size(
                Pos2::ZERO,
                Vec2::new(crate::WINDOW_WIDTH, crate::WINDOW_HEIGHT),
            ));
        let ppp = ctx.pixels_per_point();

        // Window position in physical pixels
        let win_phys_x = inner.min.x * ppp;
        let win_phys_y = inner.min.y * ppp;
        let win_w = inner.width();
        let win_h = inner.height();

        let overlay_ref = self.escape_overlay.borrow();
        let overlay = match overlay_ref.as_ref() {
            Some(o) => o,
            None => return,
        };

        let overlay_ox = overlay.origin_x as f32;
        let overlay_oy = overlay.origin_y as f32;

        let draw_list = self.draw_list.borrow();
        let mut circles = Vec::new();

        for &(_, pos, r, col, _, is_debris) in draw_list.iter() {
            // Only SGT text voxels should fly outside the window.
            // Debris remains window-contained by not drawing it on the escape overlay.
            if is_debris {
                continue;
            }

            // Skip voxels whose center is inside the window (egui already renders those)
            if pos.x >= 0.0 && pos.x <= win_w && pos.y >= 0.0 && pos.y <= win_h {
                continue;
            }

            // Fade in based on distance outside the window edge — prevents abrupt pop-in
            let dist_outside = (-pos.x)
                .max(pos.x - win_w)
                .max(-pos.y)
                .max(pos.y - win_h)
                .max(0.0);
            let fade = (dist_outside / 40.0).clamp(0.0, 1.0);
            let faded_a = (col.a() as f32 * fade) as u8;
            if faded_a == 0 {
                continue;
            }

            // Convert window-local logical position → physical screen position → overlay-local
            let screen_phys_x = win_phys_x + pos.x * ppp;
            let screen_phys_y = win_phys_y + pos.y * ppp;
            let phys_r = r * ppp;

            circles.push(EscapeCircle {
                x: screen_phys_x - overlay_ox,
                y: screen_phys_y - overlay_oy,
                radius: phys_r,
                r: col.r(),
                g: col.g(),
                b: col.b(),
                a: faded_a,
            });
        }

        overlay.update(&circles);
    }
}
