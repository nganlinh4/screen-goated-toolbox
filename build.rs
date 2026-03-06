use std::fs;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());

    // Declare the custom configuration 'nopack' to avoid warnings
    println!("cargo::rustc-check-cfg=cfg(nopack)");

    // Ensure assets directory exists
    let assets_dir = manifest_dir.join("assets");
    let _ = fs::create_dir_all(&assets_dir);

    // Optimize Tray Icon (32x32 is standard for tray)
    let tray_source = assets_dir.join("tray-icon.png");
    if tray_source.exists() {
        let tray_icon_path = assets_dir.join("tray_icon.png");
        if let Ok(img) = image::open(&tray_source) {
            let resized = img.resize(32, 32, image::imageops::FilterType::Lanczos3);
            let _ = resized.save_with_format(&tray_icon_path, image::ImageFormat::Png);
        }
    }

    // Generate the Windows app icon into OUT_DIR so builds do not dirty the repo.
    let app_icon_small_path = assets_dir.join("app-icon-small.png");
    let generated_ico_path = out_dir.join("app.ico");
    if app_icon_small_path.exists() {
        create_multi_size_ico(&app_icon_small_path, &generated_ico_path);
    }

    // Embed icon in Windows executable using manual windres compilation
    #[cfg(target_os = "windows")]
    {
        let rc_path = manifest_dir.join("app.rc");

        if generated_ico_path.exists() && rc_path.exists() {
            // Define output path for the object file in the OUT_DIR
            let generated_rc_path = out_dir.join("app.rc");
            let res_path = out_dir.join("resources.o");
            write_generated_rc(&rc_path, &generated_rc_path, &generated_ico_path);

            // Run windres manually
            // windres app.rc -o resources.o
            let status = std::process::Command::new("windres")
                .arg(&generated_rc_path)
                .arg("-o")
                .arg(&res_path)
                .status();

            match status {
                Ok(s) if s.success() => {
                    // Tell Cargo to pass the object file to the linker
                    println!("cargo:rustc-link-arg={}", res_path.display());
                }
                Ok(s) => {
                    panic!("windres failed with exit code: {}", s);
                }
                Err(e) => {
                    panic!("Failed to execute windres: {}", e);
                }
            }
        }
    }

    println!("cargo:rerun-if-changed=assets/app-icon-small.png");
    println!("cargo:rerun-if-changed=assets/tray-icon.png");
    println!("cargo:rerun-if-changed=app.rc");
    println!("cargo:rerun-if-changed=build.rs");
}

#[cfg(target_os = "windows")]
fn write_generated_rc(template_path: &Path, rc_path: &Path, ico_path: &Path) {
    let template = fs::read_to_string(template_path)
        .unwrap_or_else(|err| panic!("Failed to read {}: {}", template_path.display(), err));
    let ico_path = ico_path.to_string_lossy().replace('\\', "/");
    let generated = template.replace("\"assets/app.ico\"", &format!("\"{ico_path}\""));
    fs::write(rc_path, generated)
        .unwrap_or_else(|err| panic!("Failed to write {}: {}", rc_path.display(), err));
}

fn create_multi_size_ico(png_path: &Path, ico_path: &Path) {
    let img = match image::open(png_path) {
        Ok(i) => i,
        Err(e) => {
            println!("cargo:warning=Failed to open PNG for ICO creation: {}", e);
            return;
        }
    };
    let mut file = match fs::File::create(ico_path) {
        Ok(f) => f,
        Err(e) => {
            println!("cargo:warning=Failed to create ICO file: {}", e);
            return;
        }
    };

    // Reduced sizes to save space: 16, 32, 48, 256 (Removed 64)
    let sizes = [16, 32, 48, 256];
    let num_images = sizes.len() as u16;

    // ICO Header
    file.write_all(&[0, 0]).unwrap(); // Reserved
    file.write_all(&[1, 0]).unwrap(); // Type 1 (Icon)
    file.write_all(&num_images.to_le_bytes()).unwrap();

    let mut offset = 6 + (16 * num_images as u32);

    // Prepare image data
    let mut images_data: Vec<Vec<u8>> = Vec::new();

    for &size in &sizes {
        let mut data = Vec::new();

        if size == 256 {
            // Use PNG format for 256x256 (Vista+)
            let resized = img.resize(size, size, image::imageops::FilterType::Lanczos3);
            let mut buffer = Cursor::new(Vec::new());
            resized
                .write_to(&mut buffer, image::ImageFormat::Png)
                .unwrap();
            data = buffer.into_inner();
        } else {
            // BMP format for smaller sizes
            let resized = img.resize(size, size, image::imageops::FilterType::Lanczos3);
            let rgba = resized.to_rgba8();

            // BMP Header (40 bytes)
            data.extend_from_slice(&40u32.to_le_bytes());
            data.extend_from_slice(&(size as i32).to_le_bytes());
            data.extend_from_slice(&(size as i32 * 2).to_le_bytes()); // Height * 2
            data.extend_from_slice(&[1, 0]); // Planes
            data.extend_from_slice(&[32, 0]); // BPP
            data.extend_from_slice(&[0, 0, 0, 0]); // Compression
            data.extend_from_slice(&[0, 0, 0, 0]); // ImageSize
            data.extend_from_slice(&[0, 0, 0, 0]); // Xppm
            data.extend_from_slice(&[0, 0, 0, 0]); // Yppm
            data.extend_from_slice(&[0, 0, 0, 0]); // ColorsUsed
            data.extend_from_slice(&[0, 0, 0, 0]); // ColorsImportant

            // Pixel Data (BGRA, bottom-up)
            for row in (0..rgba.height()).rev() {
                for col in 0..rgba.width() {
                    let pixel = rgba.get_pixel(col, row);
                    data.push(pixel[2]); // B
                    data.push(pixel[1]); // G
                    data.push(pixel[0]); // R
                    data.push(pixel[3]); // A
                }
            }

            // AND Mask (1 bit per pixel, padded to 32 bits)
            // All zeros (transparent) since we use alpha channel
            let row_bytes = size.div_ceil(32) * 4;
            data.extend(std::iter::repeat_n(0, (size * row_bytes) as usize));
        }
        images_data.push(data);
    }

    // Write Directory Entries
    for (i, size) in sizes.iter().enumerate() {
        let width = if *size == 256 { 0 } else { *size as u8 };
        let height = if *size == 256 { 0 } else { *size as u8 };
        let data_size = images_data[i].len() as u32;

        file.write_all(&[width]).unwrap();
        file.write_all(&[height]).unwrap();
        file.write_all(&[0]).unwrap(); // Colors
        file.write_all(&[0]).unwrap(); // Reserved
        file.write_all(&[1, 0]).unwrap(); // Planes
        file.write_all(&[32, 0]).unwrap(); // BPP
        file.write_all(&data_size.to_le_bytes()).unwrap();
        file.write_all(&offset.to_le_bytes()).unwrap();

        offset += data_size;
    }

    // Write Image Data
    for data in images_data {
        file.write_all(&data).unwrap();
    }
}
