// --- SPLASH PALETTES ---
// Full-atmosphere colour sets for the splash. The mood is theme-coherent: dark
// theme rolls a random NIGHT palette, light theme a random DAY palette. Each set
// defines the whole scene's colours so every startup is a distinct vibe.

use eframe::egui::Color32;

const fn c(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

#[derive(Clone, Copy)]
pub struct Palette {
    pub is_night: bool,
    pub name: &'static str,
    pub sky_top: Color32,
    pub sky_horizon: Color32,
    pub haze_a: f32,
    pub body: Color32,             // moon / sun body
    pub body_hi: Color32,          // highlight
    pub body_lo: Color32,          // moon shadow / crater
    pub flare: Color32,            // sun flare (day)
    pub glow: Color32,             // halo glow
    pub accent_primary: Color32,   // G voxels + grid + progress
    pub accent_secondary: Color32, // S/T voxels
    pub cloud: Color32,
    pub text_primary: Color32, // wordmark
    pub text_accent: Color32,  // loading + click
}

// --- NIGHT (dark sky + moon) ------------------------------------------------

const NIGHT: [Palette; 5] = [
    // Synthwave — pink moon, magenta + cyan neon (the original).
    Palette {
        is_night: true,
        name: "Synthwave",
        sky_top: c(5, 5, 12),
        sky_horizon: c(34, 22, 58),
        haze_a: 0.16,
        body: c(230, 60, 120),
        body_hi: c(255, 180, 220),
        body_lo: c(130, 20, 60),
        flare: c(255, 180, 220),
        glow: c(255, 0, 100),
        accent_primary: c(255, 0, 110),
        accent_secondary: c(0, 255, 240),
        cloud: c(3, 3, 8),
        text_primary: c(240, 245, 255),
        text_accent: c(0, 255, 240),
    },
    // Aurora — navy sky, pale moon, emerald + violet, green glow.
    Palette {
        is_night: true,
        name: "Aurora",
        sky_top: c(4, 8, 24),
        sky_horizon: c(8, 38, 46),
        haze_a: 0.18,
        body: c(205, 235, 215),
        body_hi: c(240, 255, 245),
        body_lo: c(70, 120, 95),
        flare: c(240, 255, 245),
        glow: c(60, 255, 160),
        accent_primary: c(165, 120, 255), // violet G pops on the pale moon
        accent_secondary: c(90, 255, 150), // emerald S/T on the dark sky
        cloud: c(3, 9, 11),
        text_primary: c(235, 255, 245),
        text_accent: c(155, 125, 255),
    },
    // Blood Moon — dark maroon sky, red-orange moon, crimson glow, amber/gold.
    Palette {
        is_night: true,
        name: "Blood Moon",
        sky_top: c(14, 5, 7),
        sky_horizon: c(44, 14, 10),
        haze_a: 0.16,
        body: c(205, 55, 35),
        body_hi: c(255, 155, 110),
        body_lo: c(105, 22, 12),
        flare: c(255, 155, 110),
        glow: c(255, 55, 20),
        accent_primary: c(255, 205, 70), // gold G pops on the red moon
        accent_secondary: c(255, 80, 55), // scarlet S/T (the moon's family) — red + gold
        cloud: c(10, 4, 4),
        text_primary: c(255, 230, 215),
        text_accent: c(255, 195, 80),
    },
    // Nebula — purple sky, violet moon, magenta + electric-blue.
    Palette {
        is_night: true,
        name: "Nebula",
        sky_top: c(10, 5, 26),
        sky_horizon: c(32, 12, 58),
        haze_a: 0.18,
        body: c(190, 130, 235),
        body_hi: c(235, 205, 255),
        body_lo: c(85, 40, 135),
        flare: c(235, 205, 255),
        glow: c(185, 65, 255),
        accent_primary: c(80, 170, 255), // electric-blue G pops on the violet moon
        accent_secondary: c(200, 120, 255), // violet S/T (the moon's family) — violet + blue
        cloud: c(7, 4, 16),
        text_primary: c(240, 230, 255),
        text_accent: c(80, 165, 255),
    },
    // Frostbite — dark teal-navy, icy moon, cyan + silver.
    Palette {
        is_night: true,
        name: "Frostbite",
        sky_top: c(5, 13, 22),
        sky_horizon: c(12, 42, 56),
        haze_a: 0.18,
        body: c(212, 236, 255),
        body_hi: c(245, 252, 255),
        body_lo: c(88, 128, 160),
        flare: c(245, 252, 255),
        glow: c(120, 220, 255),
        accent_primary: c(70, 140, 255), // royal-blue G pops on the icy moon
        accent_secondary: c(140, 225, 255), // bright cyan S/T on the dark sky
        cloud: c(4, 10, 14),
        text_primary: c(235, 246, 255),
        text_accent: c(120, 220, 255),
    },
];

// --- DAY (bright sky + sun) -------------------------------------------------

const DAY: [Palette; 5] = [
    // Summer Beach — blue sky, orange sun (the original).
    Palette {
        is_night: false,
        name: "Summer Beach",
        sky_top: c(100, 180, 255),
        sky_horizon: c(226, 239, 255),
        haze_a: 0.6,
        body: c(255, 160, 20),
        body_hi: c(255, 255, 220),
        body_lo: c(235, 110, 20),
        flare: c(255, 240, 150),
        glow: c(255, 200, 50),
        accent_primary: c(0, 110, 255), // blue G (the sky) pops on the orange sun
        accent_secondary: c(255, 150, 35), // orange S/T (the sun) pops on the blue sky
        cloud: c(255, 255, 255),
        text_primary: c(255, 120, 0),
        text_accent: c(20, 60, 150),
    },
    // Sunset — warm pink-orange sky, red sun, magenta + gold.
    Palette {
        is_night: false,
        name: "Sunset",
        sky_top: c(255, 140, 95),
        sky_horizon: c(255, 205, 160),
        haze_a: 0.5,
        body: c(255, 75, 60),
        body_hi: c(255, 220, 180),
        body_lo: c(200, 50, 40),
        flare: c(255, 180, 120),
        glow: c(255, 120, 60),
        accent_primary: c(255, 225, 90), // gold G pops on the red sun
        accent_secondary: c(255, 95, 75), // coral S/T (the sun's family) — warm red + gold
        cloud: c(255, 235, 222),
        text_primary: c(120, 25, 60),
        text_accent: c(90, 30, 30),
    },
    // Tropical — teal sky, coral sun, pink + lime.
    Palette {
        is_night: false,
        name: "Tropical",
        sky_top: c(40, 200, 200),
        sky_horizon: c(185, 255, 248),
        haze_a: 0.55,
        body: c(255, 115, 90),
        body_hi: c(255, 230, 205),
        body_lo: c(220, 80, 55),
        flare: c(255, 180, 140),
        glow: c(255, 150, 100),
        accent_primary: c(0, 160, 170), // teal G (the sky) pops on the coral sun
        accent_secondary: c(255, 110, 85), // coral S/T (the sun) pops on the teal sky
        cloud: c(255, 255, 255),
        text_primary: c(0, 110, 105),
        text_accent: c(200, 40, 90),
    },
    // Cotton Candy — lavender sky, peach sun, pastel pink + mint.
    Palette {
        is_night: false,
        name: "Cotton Candy",
        sky_top: c(222, 185, 255),
        sky_horizon: c(255, 228, 246),
        haze_a: 0.5,
        body: c(255, 200, 160),
        body_hi: c(255, 245, 232),
        body_lo: c(235, 165, 135),
        flare: c(255, 225, 200),
        glow: c(255, 210, 180),
        accent_primary: c(165, 110, 235), // orchid G (the sky) pops on the peach sun
        accent_secondary: c(255, 150, 110), // coral-peach S/T (the sun) on the lavender sky
        cloud: c(255, 255, 255),
        text_primary: c(140, 85, 175),
        text_accent: c(210, 80, 150),
    },
    // Golden Hour — warm amber sky, gold sun, rose + cream.
    Palette {
        is_night: false,
        name: "Golden Hour",
        sky_top: c(255, 200, 120),
        sky_horizon: c(255, 238, 200),
        haze_a: 0.5,
        body: c(255, 210, 90),
        body_hi: c(255, 250, 225),
        body_lo: c(230, 160, 60),
        flare: c(255, 235, 180),
        glow: c(255, 220, 130),
        accent_primary: c(230, 80, 100), // rose G pops on the gold sun
        accent_secondary: c(245, 105, 125), // lighter rose S/T — gold + rose, both warm
        cloud: c(255, 255, 255),
        text_primary: c(140, 65, 35),
        text_accent: c(170, 60, 70),
    },
];

/// Pick a theme-coherent palette: a random NIGHT set in dark theme, a random DAY
/// set in light theme. `seed` selects which set (advance it for a fresh roll).
pub fn pick(is_dark: bool, seed: u64) -> Palette {
    let pool: &[Palette] = if is_dark { &NIGHT } else { &DAY };
    pool[(mix(seed) % pool.len() as u64) as usize]
}

/// splitmix64 finalizer. The raw seed is `SystemTime` nanos, which on Windows is
/// 100ns-granular (always a multiple of 100); `nanos % 5 == 0` always, so without
/// mixing every launch would land on the same palette. Mixing scrambles the low
/// bits so `% pool.len()` is uniform regardless of the seed's granularity.
fn mix(seed: u64) -> u64 {
    let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}
