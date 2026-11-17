//! Video cutscenes: a film played over the game screen, from the Timeline ("Play video") or
//! from a script (`video.play("videos/intro.cryovid")`).
//!
//! Games play one format, `.cryovid`: JPEG frames at a fixed rate plus an optional WAV sound
//! track, in one file. It needs no system codec, so an exported game plays the same on every
//! computer. The editor makes it from a normal video (`.mp4`, `.mov`, `.webm`, `.mkv`, `.avi`,
//! `.gif`) with ffmpeg, when ffmpeg is installed on the computer where the game is made.
//!
//! File layout: the 9 bytes `CRYOVID1\n`, the length of the header (u32, little endian), the
//! header (JSON), then the frames and the sound, at the offsets the header gives.

use serde::{Deserialize, Serialize};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

const MAGIC: &[u8] = b"CRYOVID1\n";
/// Video files the editor can convert (with ffmpeg).
pub const SOURCE_EXTS: &[&str] = &["mp4", "mov", "m4v", "webm", "mkv", "avi", "gif"];
pub const EXT: &str = "cryovid";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
struct Header {
    fps: f32,
    width: u32,
    height: u32,
    /// (offset from the end of the header, length) of each JPEG frame.
    frames: Vec<(u64, u64)>,
    /// The WAV sound track, if any.
    audio: Option<(u64, u64)>,
}

/// An open video. Frames are read from the file when asked for.
pub struct Video {
    path: PathBuf,
    header: Header,
    data_start: u64,
}

impl Video {
    pub fn open(path: &Path) -> Result<Video, String> {
        let mut f = std::fs::File::open(path).map_err(|e| format!("could not open the video ({e})"))?;
        let mut magic = [0u8; 9];
        f.read_exact(&mut magic).map_err(|_| "not a Cryo video".to_string())?;
        if magic != MAGIC {
            return Err("not a Cryo video (convert it with File > Import video)".into());
        }
        let mut len = [0u8; 4];
        f.read_exact(&mut len).map_err(|e| e.to_string())?;
        let len = u32::from_le_bytes(len) as usize;
        if len > 64 << 20 {
            return Err("the video's header is damaged".into());
        }
        let mut json = vec![0u8; len];
        f.read_exact(&mut json).map_err(|e| e.to_string())?;
        let header: Header = serde_json::from_slice(&json).map_err(|e| format!("the video's header is damaged ({e})"))?;
        if header.frames.is_empty() || !(header.fps > 0.0) {
            return Err("the video has no frames".into());
        }
        Ok(Video { path: path.to_path_buf(), header, data_start: (MAGIC.len() + 4 + len) as u64 })
    }

    #[allow(dead_code)]
    pub fn fps(&self) -> f32 {
        self.header.fps
    }
    pub fn size(&self) -> (u32, u32) {
        (self.header.width, self.header.height)
    }
    #[allow(dead_code)]
    pub fn frame_count(&self) -> usize {
        self.header.frames.len()
    }
    /// Length in seconds.
    pub fn duration(&self) -> f32 {
        self.header.frames.len() as f32 / self.header.fps
    }
    /// The frame on screen `t` seconds after the start.
    pub fn frame_at(&self, t: f64) -> usize {
        ((t.max(0.0) * self.header.fps as f64) as usize).min(self.header.frames.len() - 1)
    }

    fn blob(&self, (offset, len): (u64, u64)) -> Result<Vec<u8>, String> {
        let mut f = std::fs::File::open(&self.path).map_err(|e| e.to_string())?;
        f.seek(SeekFrom::Start(self.data_start + offset)).map_err(|e| e.to_string())?;
        let mut buf = vec![0u8; len as usize];
        f.read_exact(&mut buf).map_err(|e| e.to_string())?;
        Ok(buf)
    }

    /// One frame as RGBA pixels: (width, height, pixels).
    pub fn frame(&self, index: usize) -> Result<(u32, u32, Vec<u8>), String> {
        let at = *self.header.frames.get(index).ok_or("no such frame")?;
        let jpeg = self.blob(at)?;
        let img = image::load_from_memory_with_format(&jpeg, image::ImageFormat::Jpeg).map_err(|e| e.to_string())?.to_rgba8();
        Ok((img.width(), img.height(), img.into_raw()))
    }

    /// The sound track (a WAV file in memory).
    pub fn audio(&self) -> Option<Vec<u8>> {
        self.header.audio.and_then(|a| self.blob(a).ok())
    }
}

/// Writes a `.cryovid` from JPEG frames and an optional WAV sound track.
pub fn write(path: &Path, fps: f32, width: u32, height: u32, frames: &[Vec<u8>], audio: Option<&[u8]>) -> Result<(), String> {
    let mut offset = 0u64;
    let mut index = Vec::with_capacity(frames.len());
    for f in frames {
        index.push((offset, f.len() as u64));
        offset += f.len() as u64;
    }
    let header = Header { fps, width, height, frames: index, audio: audio.map(|a| (offset, a.len() as u64)) };
    let json = serde_json::to_vec(&header).map_err(|e| e.to_string())?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let mut out = std::io::BufWriter::new(std::fs::File::create(path).map_err(|e| e.to_string())?);
    let mut put = |bytes: &[u8]| out.write_all(bytes).map_err(|e| e.to_string());
    put(MAGIC)?;
    put(&(json.len() as u32).to_le_bytes())?;
    put(&json)?;
    for f in frames {
        put(f)?;
    }
    if let Some(a) = audio {
        put(a)?;
    }
    Ok(())
}

fn ffmpeg() -> Option<PathBuf> {
    // apps started from Finder do not see the terminal's PATH: also look where Homebrew
    // and the usual installers put it
    let mut candidates = vec![PathBuf::from("ffmpeg")];
    for dir in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/opt/local/bin"] {
        candidates.push(Path::new(dir).join("ffmpeg"));
    }
    if let Some(custom) = std::env::var_os("CRYO_FFMPEG") {
        candidates.insert(0, PathBuf::from(custom));
    }
    candidates.into_iter().find(|c| {
        std::process::Command::new(c)
            .arg("-version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    })
}

/// How to get ffmpeg, for the message shown when it is missing.
pub fn ffmpeg_hint() -> &'static str {
    if cfg!(target_os = "macos") {
        "install it with `brew install ffmpeg` (https://brew.sh)"
    } else if cfg!(target_os = "windows") {
        "install it with `winget install ffmpeg`"
    } else {
        "install it with your package manager, for example `sudo apt install ffmpeg`"
    }
}

/// Converts a normal video into a `.cryovid`: frames at most 720 lines high, 24 a second,
/// and the sound as 44.1 kHz stereo. Returns (frames, seconds).
pub fn import(source: &Path, dest: &Path) -> Result<(usize, f32), String> {
    if source.is_dir() {
        return import_frames(source, dest, 24.0);
    }
    let gif = source.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("gif")).unwrap_or(false);
    let Some(ffmpeg) = ffmpeg() else {
        // without ffmpeg: animated GIFs and folders of pictures still work
        if gif {
            return import_gif(source, dest);
        }
        return Err(format!(
            "converting this kind of video needs ffmpeg: {}. Without it you can import an animated GIF, or a folder of numbered pictures (PNG or JPEG, with an optional sound.wav) exported from any video editor.",
            ffmpeg_hint()
        ));
    };
    let work = std::env::temp_dir().join(format!("cryo_video_{}_{}", std::process::id(), dest.file_stem().and_then(|s| s.to_str()).unwrap_or("v")));
    std::fs::create_dir_all(&work).map_err(|e| e.to_string())?;
    let result = (|| {
        let fps = 24.0;
        let run = |args: &[&std::ffi::OsStr]| {
            std::process::Command::new(&ffmpeg)
                .args(["-y", "-loglevel", "error", "-i"])
                .arg(source)
                .args(args)
                .output()
                .map_err(|e| format!("could not run ffmpeg ({e})"))
        };
        let pattern = work.join("f_%06d.jpg");
        let out = run(&[
            "-an".as_ref(),
            "-vf".as_ref(),
            // at most 1280 x 720, keeping the shape, with even sides
            "fps=24,scale='min(1280,iw)':'min(720,ih)':force_original_aspect_ratio=decrease:force_divisible_by=2".as_ref(),
            "-q:v".as_ref(),
            "4".as_ref(),
            pattern.as_os_str(),
        ])?;
        if !out.status.success() {
            return Err(format!("ffmpeg could not read the video: {}", String::from_utf8_lossy(&out.stderr).trim()));
        }
        let mut names: Vec<PathBuf> = std::fs::read_dir(&work)
            .map_err(|e| e.to_string())?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("jpg"))
            .collect();
        names.sort();
        if names.is_empty() {
            return Err("the video has no picture".into());
        }
        let frames: Vec<Vec<u8>> = names.iter().map(|p| std::fs::read(p).map_err(|e| e.to_string())).collect::<Result<_, _>>()?;
        let first = image::load_from_memory(&frames[0]).map_err(|e| e.to_string())?;
        // the sound is optional: a silent video simply has none
        let wav = work.join("audio.wav");
        let audio = match run(&["-vn".as_ref(), "-ac".as_ref(), "2".as_ref(), "-ar".as_ref(), "44100".as_ref(), wav.as_os_str()]) {
            Ok(o) if o.status.success() => std::fs::read(&wav).ok().filter(|a| a.len() > 1024),
            _ => None,
        };
        write(dest, fps, first.width(), first.height(), &frames, audio.as_deref())?;
        Ok((frames.len(), frames.len() as f32 / fps))
    })();
    // the working files are ours: remove them whatever happened
    if let Ok(entries) = std::fs::read_dir(&work) {
        for e in entries.flatten() {
            let _ = std::fs::remove_file(e.path());
        }
    }
    let _ = std::fs::remove_dir(&work);
    result
}

/// Largest picture kept in a video: bigger ones are scaled down to fit.
const MAX_W: u32 = 1280;
const MAX_H: u32 = 720;

fn to_jpeg(img: image::DynamicImage) -> Result<Vec<u8>, String> {
    let img = if img.width() > MAX_W || img.height() > MAX_H { img.resize(MAX_W, MAX_H, image::imageops::FilterType::Triangle) } else { img };
    let mut out = std::io::Cursor::new(vec![]);
    image::DynamicImage::ImageRgb8(img.to_rgb8())
        .write_to(&mut out, image::ImageFormat::Jpeg)
        .map_err(|e| e.to_string())?;
    Ok(out.into_inner())
}

/// An animated GIF, decoded here (no ffmpeg needed). The frame delays become a fixed rate.
pub fn import_gif(source: &Path, dest: &Path) -> Result<(usize, f32), String> {
    use image::AnimationDecoder;
    let file = std::io::BufReader::new(std::fs::File::open(source).map_err(|e| e.to_string())?);
    let decoder = image::codecs::gif::GifDecoder::new(file).map_err(|e| format!("not a GIF ({e})"))?;
    let frames = decoder.into_frames().collect_frames().map_err(|e| e.to_string())?;
    if frames.is_empty() {
        return Err("the GIF has no frames".into());
    }
    let total: f32 = frames.iter().map(|f| { let (n, d) = f.delay().numer_denom_ms(); n as f32 / d.max(1) as f32 / 1000.0 }).sum();
    // GIFs often say 0 ms: browsers show those at 10 a second
    let seconds = if total > 0.01 { total } else { frames.len() as f32 * 0.1 };
    let fps = (frames.len() as f32 / seconds).clamp(1.0, 60.0);
    let mut jpegs = Vec::with_capacity(frames.len());
    for f in frames {
        jpegs.push(to_jpeg(image::DynamicImage::ImageRgba8(f.into_buffer()))?);
    }
    let first = image::load_from_memory(&jpegs[0]).map_err(|e| e.to_string())?;
    write(dest, fps, first.width(), first.height(), &jpegs, None)?;
    Ok((jpegs.len(), jpegs.len() as f32 / fps))
}

/// A folder of pictures in name order (frame0001.png, frame0002.png...), as any video editor
/// can export, plus an optional `sound.wav` (or any single .wav) for the sound track.
pub fn import_frames(dir: &Path, dest: &Path, fps: f32) -> Result<(usize, f32), String> {
    let mut pictures: Vec<PathBuf> = vec![];
    let mut sound = None;
    for e in std::fs::read_dir(dir).map_err(|e| e.to_string())?.flatten() {
        let p = e.path();
        match p.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref() {
            Some("png" | "jpg" | "jpeg") => pictures.push(p),
            Some("wav") => sound = Some(p),
            _ => {}
        }
    }
    pictures.sort();
    if pictures.is_empty() {
        return Err("the folder has no PNG or JPEG pictures".into());
    }
    let mut jpegs = Vec::with_capacity(pictures.len());
    for p in &pictures {
        let img = image::open(p).map_err(|e| format!("{}: {e}", p.display()))?;
        jpegs.push(to_jpeg(img)?);
    }
    let first = image::load_from_memory(&jpegs[0]).map_err(|e| e.to_string())?;
    let audio = sound.and_then(|p| std::fs::read(p).ok());
    write(dest, fps, first.width(), first.height(), &jpegs, audio.as_deref())?;
    Ok((jpegs.len(), jpegs.len() as f32 / fps))
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn jpeg(w: u32, h: u32, rgb: [u8; 3]) -> Vec<u8> {
        let img = image::RgbImage::from_pixel(w, h, image::Rgb(rgb));
        let mut out = std::io::Cursor::new(vec![]);
        img.write_to(&mut out, image::ImageFormat::Jpeg).unwrap();
        out.into_inner()
    }

    #[test]
    fn gifs_and_picture_folders_import_without_ffmpeg() {
        let dir = std::env::temp_dir().join(format!("cryo_vidimp_{}", std::process::id()));
        std::fs::create_dir_all(dir.join("frames")).unwrap();
        // a 3-frame GIF, 50 ms a frame
        {
            use image::codecs::gif::{GifEncoder, Repeat};
            let f = std::fs::File::create(dir.join("anim.gif")).unwrap();
            let mut enc = GifEncoder::new(f);
            enc.set_repeat(Repeat::Infinite).unwrap();
            for c in [[255u8, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255]] {
                let buf = image::RgbaImage::from_pixel(40, 30, image::Rgba(c));
                enc.encode_frame(image::Frame::from_parts(buf, 0, 0, image::Delay::from_numer_denom_ms(50, 1))).unwrap();
            }
        }
        let (n, secs) = import_gif(&dir.join("anim.gif"), &dir.join("a.cryovid")).unwrap();
        assert_eq!(n, 3);
        assert!((secs - 0.15).abs() < 0.02, "{secs}");
        let v = Video::open(&dir.join("a.cryovid")).unwrap();
        let (_, _, px) = v.frame(2).unwrap();
        assert!(px[2] > 200 && px[0] < 60, "the last frame is blue");
        // a folder of numbered pictures with a sound, through the same entry point as videos
        for (i, c) in [[200u8, 200, 0], [0, 200, 200]].iter().enumerate() {
            image::RgbImage::from_pixel(2000, 1000, image::Rgb(*c)).save(dir.join(format!("frames/f{i:03}.png"))).unwrap();
        }
        std::fs::write(dir.join("frames/sound.wav"), crate::audio::synth_wav(0.1, |_| 0.0)).unwrap();
        let (n, _) = import(&dir.join("frames"), &dir.join("b.cryovid")).unwrap();
        assert_eq!(n, 2);
        let v = Video::open(&dir.join("b.cryovid")).unwrap();
        assert_eq!(v.size(), (1280, 640), "big pictures are scaled down to fit 1280 x 720");
        assert!(v.audio().is_some());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_video_is_written_and_read_back_frame_by_frame() {
        let dir = std::env::temp_dir().join(format!("cryo_vid_{}", std::process::id()));
        let path = dir.join("clip.cryovid");
        let frames = vec![jpeg(32, 18, [250, 10, 10]), jpeg(32, 18, [10, 250, 10]), jpeg(32, 18, [10, 10, 250])];
        let sound = crate::audio::synth_wav(0.2, |t| (t * 440.0 * std::f32::consts::TAU).sin());
        write(&path, 12.0, 32, 18, &frames, Some(&sound)).unwrap();
        let v = Video::open(&path).unwrap();
        assert_eq!((v.frame_count(), v.size(), v.fps()), (3, (32, 18), 12.0));
        assert!((v.duration() - 0.25).abs() < 1e-6);
        assert_eq!((v.frame_at(0.0), v.frame_at(0.09), v.frame_at(0.2), v.frame_at(9.0)), (0, 1, 2, 2));
        let (w, h, px) = v.frame(1).unwrap();
        assert_eq!((w, h, px.len()), (32, 18, 32 * 18 * 4));
        assert!(px[1] > 200 && px[0] < 60 && px[2] < 60, "the second frame is green: {:?}", &px[..4]);
        assert_eq!(v.audio().unwrap(), sound);
        // anything else is refused with a clear message
        std::fs::write(dir.join("fake.cryovid"), b"hello").unwrap();
        assert!(Video::open(&dir.join("fake.cryovid")).is_err());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn ffmpeg_converts_a_normal_video_when_it_is_installed() {
        let Some(ffmpeg) = ffmpeg() else {
            eprintln!("ffmpeg not installed: conversion not tested");
            return;
        };
        let dir = std::env::temp_dir().join(format!("cryo_vidimport_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("test.mp4");
        // one second of a test pattern with a tone
        let made = std::process::Command::new(&ffmpeg)
            .args(["-y", "-loglevel", "error", "-f", "lavfi", "-i", "testsrc=duration=1:size=320x180:rate=30", "-f", "lavfi", "-i", "sine=frequency=440:duration=1", "-shortest"])
            .arg(&src)
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !made {
            eprintln!("this ffmpeg cannot make the test video: conversion not tested");
            return;
        }
        let dest = dir.join("out.cryovid");
        let (frames, seconds) = import(&src, &dest).unwrap();
        assert!((22..=26).contains(&frames), "{frames}");
        assert!((seconds - 1.0).abs() < 0.1);
        let v = Video::open(&dest).unwrap();
        assert_eq!(v.size(), (320, 180));
        assert!(v.frame(0).is_ok() && v.audio().is_some());
        std::fs::remove_dir_all(dir).ok();
    }
}
