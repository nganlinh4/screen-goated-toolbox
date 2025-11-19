use image::{ImageBuffer, Rgba};

pub fn generate_icon() -> tray_icon::Icon {
    let width = 32;
    let height = 32;
    let mut img = ImageBuffer::new(width, height);

    // Prefix x and y with _ to ignore "unused variable" warning
    for (_x, _y, pixel) in img.enumerate_pixels_mut() {
        // Blue background
        *pixel = Rgba([50, 100, 255, 255]); 
    }
    
    // White center dot
    for i in 12..20 {
         for j in 12..20 {
             img.put_pixel(i, j, Rgba([255, 255, 255, 255]));
         }
    }

    let rgba = img.into_raw();
    tray_icon::Icon::from_rgba(rgba, width, height).unwrap()
}