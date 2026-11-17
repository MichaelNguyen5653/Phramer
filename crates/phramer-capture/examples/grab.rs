//! Grabs every monitor and the whole virtual desktop, saves each as a PNG
//! and prints what was found and how long each grab took.
//!
//! ```text
//! cargo run -p phramer-capture --release --example grab -- <output directory>
//! ```

#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::path::PathBuf;
    use std::time::Instant;

    use phramer_capture::{
        cursor_position, enable_per_monitor_dpi_awareness, enumerate_monitors, grab_monitor,
        grab_virtual_desktop, monitor_at, visible_windows,
    };

    let directory = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("usage: grab <output directory>")?,
    );
    std::fs::create_dir_all(&directory)?;

    // Must come first: see the crate documentation
    enable_per_monitor_dpi_awareness()?;

    let monitors = enumerate_monitors()?;
    let cursor = cursor_position()?;
    let under_cursor = monitor_at(&monitors, cursor);
    println!(
        "{} monitor(s), cursor at ({}, {})",
        monitors.len(),
        cursor.x,
        cursor.y
    );

    for (index, monitor) in monitors.iter().enumerate() {
        let bounds = monitor.bounds();
        let (logical_width, logical_height) = monitor.logical_size();
        println!(
            "monitor {index}: {}x{} at ({}, {}), {} DPI ({}%), logical {}x{}{}{}",
            bounds.width,
            bounds.height,
            bounds.x,
            bounds.y,
            monitor.dpi(),
            (monitor.scale() * 100.0).round(),
            logical_width,
            logical_height,
            if monitor.is_primary() {
                ", primary"
            } else {
                ""
            },
            if under_cursor == Some(monitor) {
                ", under cursor"
            } else {
                ""
            },
        );
        println!("  id: {}", monitor.id().as_str());

        let started = Instant::now();
        let frame = grab_monitor(monitor)?;
        let elapsed = started.elapsed();
        let path = directory.join(format!("monitor-{index}.png"));
        println!(
            "  grab: {:.1} ms, {}x{} -> {}",
            elapsed.as_secs_f64() * 1000.0,
            frame.width(),
            frame.height(),
            path.display()
        );
        save_png(&path, frame)?;
    }

    let started = Instant::now();
    let frame = grab_virtual_desktop(&monitors)?;
    let elapsed = started.elapsed();
    let path = directory.join("desktop.png");
    println!(
        "virtual desktop: {}x{} at ({}, {}), grab: {:.1} ms -> {}",
        frame.width(),
        frame.height(),
        frame.bounds().x,
        frame.bounds().y,
        elapsed.as_secs_f64() * 1000.0,
        path.display()
    );
    save_png(&path, frame)?;

    let started = Instant::now();
    let windows = visible_windows()?;
    println!(
        "{} snap window(s), listed in {:.1} ms",
        windows.len(),
        started.elapsed().as_secs_f64() * 1000.0
    );
    Ok(())
}

#[cfg(windows)]
fn save_png(
    path: &std::path::Path,
    frame: phramer_capture::Frame,
) -> Result<(), Box<dyn std::error::Error>> {
    let file = std::io::BufWriter::new(std::fs::File::create(path)?);
    let mut encoder = png::Encoder::new(file, frame.width(), frame.height());
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Fast);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&frame.into_rgba())?;
    writer.finish()?;
    Ok(())
}

#[cfg(not(windows))]
fn main() {
    eprintln!("phramer-capture only captures on Windows");
}
