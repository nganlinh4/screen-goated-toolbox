// --- SPLASH SCREEN MODULE ---
// Animated splash screen with 3D voxel effects, procedural audio, and theme switching.

mod audio;
mod escape_overlay;
mod math;
mod render;
mod scene;

use audio::SplashAudio;
use eframe::egui::{self, Color32, Pos2, Vec2};
use escape_overlay::{EscapeCircle, EscapeOverlay};
use math::Vec3;
use scene::{Cloud, MoonFeature, Star, Voxel};
use std::cell::RefCell;
use std::sync::{Arc, Mutex};

// --- CONFIGURATION ---
const ANIMATION_DURATION: f32 = 3.6;
const START_TRANSITION: f32 = 0.8;
const EXIT_DURATION: f32 = 1.6;

// --- PALETTE ---
const C_VOID: Color32 = Color32::from_rgb(5, 5, 10);
const C_CYAN: Color32 = Color32::from_rgb(0, 255, 240);
const C_MAGENTA: Color32 = Color32::from_rgb(255, 0, 110);
const C_WHITE: Color32 = Color32::from_rgb(240, 245, 255);
const C_SHADOW: Color32 = Color32::from_rgb(20, 20, 30);

// Moon Palette
const C_MOON_BASE: Color32 = Color32::from_rgb(230, 60, 120);
const C_MOON_SHADOW: Color32 = Color32::from_rgb(130, 20, 60);
const C_MOON_HIGHLIGHT: Color32 = Color32::from_rgb(255, 180, 220);
const C_MOON_GLOW: Color32 = Color32::from_rgb(255, 0, 100);

// Dark Cloud Palette
const C_CLOUD_CORE: Color32 = Color32::from_rgb(2, 2, 5);

// Day Palette
const C_SKY_DAY_TOP: Color32 = Color32::from_rgb(100, 180, 255);
const C_DAY_REP: Color32 = Color32::from_rgb(0, 110, 255);
const C_DAY_SEC: Color32 = Color32::from_rgb(255, 255, 255);
const C_DAY_TEXT: Color32 = Color32::from_rgb(255, 120, 0);

const C_SUN_BODY: Color32 = Color32::from_rgb(255, 160, 20);
const C_SUN_FLARE: Color32 = Color32::from_rgb(255, 240, 150);
const C_SUN_GLOW: Color32 = Color32::from_rgb(255, 200, 50);
const C_SUN_HIGHLIGHT: Color32 = Color32::from_rgb(255, 255, 220);

const C_CLOUD_WHITE: Color32 = Color32::from_rgb(255, 255, 255);

pub struct SplashScreen {
    start_time: f64,
    voxels: Vec<Voxel>,
    clouds: Vec<Cloud>,
    stars: Vec<Star>,
    moon_features: Vec<MoonFeature>,
    init_done: bool,
    mouse_influence: Vec2,
    mouse_world_pos: Vec3,
    loading_text: String,
    exit_start_time: Option<f64>,
    is_dark: bool,
    audio: Arc<Mutex<Option<SplashAudio>>>,
    has_played_impact: bool,
    draw_list: RefCell<Vec<(f32, Pos2, f32, Color32, bool, bool)>>,
    // Escape overlay — separate transparent window for voxels that fly beyond the main window
    escape_overlay: RefCell<Option<EscapeOverlay>>,
}

pub enum SplashStatus {
    Ongoing,
    Finished,
}

impl SplashScreen {
    pub fn new(ctx: &egui::Context) -> Self {
        let is_dark = ctx.style().visuals.dark_mode;
        let audio_container = Arc::new(Mutex::new(None));
        let audio_container_clone = audio_container.clone();

        std::thread::spawn(move || {
            if let Some(audio) = SplashAudio::new() {
                if let Ok(mut lock) = audio_container_clone.lock() {
                    *lock = Some(audio);
                }
            }
        });

        let mut slf = Self {
            start_time: ctx.input(|i| i.time),
            voxels: Vec::with_capacity(600),
            clouds: Vec::with_capacity(20),
            stars: Vec::with_capacity(200),
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
            &mut self.stars,
            &mut self.clouds,
            &mut self.moon_features,
            self.is_dark,
        );
        self.init_done = true;
    }

    pub fn update(&mut self, ctx: &egui::Context) -> SplashStatus {
        let was_exiting = self.exit_start_time.is_some();

        let status = render::update(
            ctx,
            self.start_time,
            &mut self.exit_start_time,
            &mut self.voxels,
            &mut self.clouds,
            &mut self.mouse_influence,
            &mut self.mouse_world_pos,
            &mut self.loading_text,
            &mut self.is_dark,
            &self.audio,
            &mut self.has_played_impact,
        );

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
        let result = render::paint(
            ctx,
            self.start_time,
            self.exit_start_time,
            &self.voxels,
            &self.clouds,
            &self.stars,
            &self.moon_features,
            self.mouse_influence,
            self.is_dark,
            &self.loading_text,
            &self.draw_list,
        );

        // After paint fills draw_list, send escaped voxels to the overlay
        if self.escape_overlay.borrow().is_some() {
            self.update_escape_overlay(ctx);
        }

        result
    }

    fn update_escape_overlay(&self, ctx: &egui::Context) {
        let viewport = ctx.input(|i| i.viewport().clone());
        let inner = viewport.inner_rect.unwrap_or(eframe::egui::Rect::from_min_size(
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

        for &(_, pos, r, col, _, _) in draw_list.iter() {
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
