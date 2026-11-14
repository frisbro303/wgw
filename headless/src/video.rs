//! Videos' frames as files (the "video" feature): animated PNG and GIF, written here, and other formats by ffmpeg.

use std::io::Write;
use std::process::{Command, Stdio};

use crate::Image;

/// Frames as an animated PNG, looping.
pub fn apng(frames: &[Image], fps: f64) -> Result<Vec<u8>, String> {
    let first = frames.first().ok_or("no frames")?;
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, first.width, first.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_animated(frames.len() as u32, 0).map_err(|e| e.to_string())?;
    // Each frame shows for 100 / (100 fps) seconds.
    encoder.set_frame_delay(100, (100.0 * fps).round().clamp(1.0, 65535.0) as u16).map_err(|e| e.to_string())?;
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    for f in frames {
        writer.write_image_data(&f.rgba).map_err(|e| e.to_string())?;
    }
    writer.finish().map_err(|e| e.to_string())?;
    Ok(out)
}

/// Frames as a GIF, looping (its colors quantized per frame; transparent pixels, if any, as GIF's one transparent
/// color).
pub fn gif(frames: &[Image], fps: f64) -> Result<Vec<u8>, String> {
    let first = frames.first().ok_or("no frames")?;
    let mut out = Vec::new();
    {
        let mut encoder = gif::Encoder::new(&mut out, first.width as u16, first.height as u16, &[]).map_err(|e| e.to_string())?;
        encoder.set_repeat(gif::Repeat::Infinite).map_err(|e| e.to_string())?;
        let delay = (100.0 / fps).round().max(1.0) as u16; // hundredths of a second
        for f in frames {
            let mut rgba = f.rgba.clone();
            let mut frame = gif::Frame::from_rgba_speed(f.width as u16, f.height as u16, &mut rgba, 10);
            frame.delay = delay;
            encoder.write_frame(&frame).map_err(|e| e.to_string())?;
        }
    }
    Ok(out)
}

/// Frames to a video file, by ffmpeg.
pub fn ffmpeg(path: &std::path::Path, frames: &[Image], fps: f64) -> Result<(), String> {
    let first = frames.first().ok_or("no frames")?;
    let mut child = Command::new("ffmpeg")
        .args(["-y", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "rgba"])
        .args(["-s", &format!("{}x{}", first.width, first.height), "-r", &fps.to_string(), "-i", "-"])
        .args(["-vf", "pad=ceil(iw/2)*2:ceil(ih/2)*2", "-pix_fmt", "yuv420p"])
        .arg(path)
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| "saving this kind of video needs ffmpeg on the PATH (or save a .gif or .apng)".to_string())?;
    let mut stdin = child.stdin.take().unwrap();
    for f in frames {
        if stdin.write_all(&f.rgba).is_err() {
            break;
        }
    }
    drop(stdin);
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!("ffmpeg: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(())
}
