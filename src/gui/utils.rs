use windows::Win32::Foundation::{BOOL, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, HDC, HMONITOR, GetMonitorInfoW, MONITORINFOEXW};
use eframe::egui;

// --- Monitor Enumeration ---

struct MonitorEnumContext {
    monitors: Vec<String>,
}

unsafe extern "system" fn monitor_enum_proc(hmonitor: HMONITOR, _hdc: HDC, _lprc: *mut RECT, dwdata: LPARAM) -> BOOL {
    let context = &mut *(dwdata.0 as *mut MonitorEnumContext);
    let mut mi = MONITORINFOEXW::default();
    mi.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
    
    if GetMonitorInfoW(hmonitor, &mut mi as *mut _ as *mut _).as_bool() {
        let device_name = String::from_utf16_lossy(&mi.szDevice);
        let trimmed_name = device_name.trim_matches(char::from(0)).to_string();
        context.monitors.push(trimmed_name);
    }
    BOOL(1)
}

pub fn get_monitor_names() -> Vec<String> {
    let mut ctx = MonitorEnumContext { monitors: Vec::new() };
    unsafe {
        EnumDisplayMonitors(HDC(0), None, Some(monitor_enum_proc), LPARAM(&mut ctx as *mut _ as isize));
    }
    ctx.monitors
}

// --- Font Configuration ---

pub fn configure_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let viet_font_name = "segoe_ui";
    
    // Dynamic Windows font path
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".to_string());
    let font_dir = std::path::Path::new(&windir).join("Fonts");
    
    let viet_font_path = font_dir.join("segoeui.ttf");
    let viet_fallback_path = font_dir.join("arial.ttf");
    let viet_data = std::fs::read(&viet_font_path).or_else(|_| std::fs::read(&viet_fallback_path));

    let korean_font_name = "malgun_gothic";
    let korean_font_path = font_dir.join("malgun.ttf");
    let korean_data = std::fs::read(&korean_font_path);

    if let Ok(data) = viet_data {
        fonts.font_data.insert(viet_font_name.to_owned(), egui::FontData::from_owned(data));
        if let Some(vec) = fonts.families.get_mut(&egui::FontFamily::Proportional) { vec.insert(0, viet_font_name.to_owned()); }
        if let Some(vec) = fonts.families.get_mut(&egui::FontFamily::Monospace) { vec.insert(0, viet_font_name.to_owned()); }
    }
    if let Ok(data) = korean_data {
        fonts.font_data.insert(korean_font_name.to_owned(), egui::FontData::from_owned(data));
        if let Some(vec) = fonts.families.get_mut(&egui::FontFamily::Proportional) { 
            let idx = if vec.contains(&viet_font_name.to_string()) { 1 } else { 0 };
            vec.insert(idx, korean_font_name.to_owned()); 
        }
        if let Some(vec) = fonts.families.get_mut(&egui::FontFamily::Monospace) { 
             let idx = if vec.contains(&viet_font_name.to_string()) { 1 } else { 0 };
             vec.insert(idx, korean_font_name.to_owned()); 
        }
    }
    ctx.set_fonts(fonts);
}
