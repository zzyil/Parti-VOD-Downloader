use chrono::{NaiveDateTime, TimeZone, Utc};
use eframe::{egui, App, Frame};
use regex::Regex;
use reqwest::blocking::Client;
use serde_json::Value;
use std::fs::File;
use std::io::{self, BufRead, BufWriter};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use url::Url;
use which::which;

// --- NEW: For icon loading ---
use egui::IconData;
use image::GenericImageView;
// -----------------------------

fn sanitize_filename(s: &str) -> String {
    let re = Regex::new(r"[^\w\d]+").unwrap();
    let s = re.replace_all(s, "_");
    let s = s.trim_matches('_');
    let s = Regex::new(r"_+").unwrap().replace_all(&s, "_");
    s.to_string()
}

fn get_ffmpeg_path() -> Result<PathBuf, String> {
    if let Ok(path) = which("ffmpeg") {
        return Ok(path);
    }
    let local_path = if cfg!(windows) {
        PathBuf::from("./ffmpeg-bin/ffmpeg.exe")
    } else {
        PathBuf::from("./ffmpeg-bin/ffmpeg")
    };
    if local_path.exists() {
        return Ok(local_path);
    }
    println!("ffmpeg not found, downloading static binary...");
    std::fs::create_dir_all("./ffmpeg-bin").map_err(|e| e.to_string())?;

    #[cfg(target_os = "macos")]
    let (url, bin_name) = (
        "https://evermeet.cx/ffmpeg/ffmpeg-6.1.1.zip",
        "ffmpeg"
    );
    #[cfg(target_os = "linux")]
    let (url, bin_name) = (
        "https://johnvansickle.com/ffmpeg/releases/ffmpeg-release-amd64-static.tar.xz",
        "ffmpeg"
    );
    #[cfg(target_os = "windows")]
    let (url, bin_name) = (
        "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip",
        "ffmpeg.exe"
    );

    let archive_path = "./ffmpeg-bin/ffmpeg_download";
    let mut resp = reqwest::blocking::get(url).map_err(|e| e.to_string())?;
    let mut out = std::fs::File::create(archive_path).map_err(|e| e.to_string())?;
    std::io::copy(&mut resp, &mut out).map_err(|e| e.to_string())?;

    #[cfg(target_os = "macos")]
    {
        let file = std::fs::File::open(archive_path).map_err(|e| e.to_string())?;
        let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
        for i in 0..archive.len() {
            let mut file = archive.by_index(i).unwrap();
            let outpath = file.sanitized_name();
            if outpath.file_name().map(|n| n == bin_name).unwrap_or(false) {
                let mut out_bin = std::fs::File::create("./ffmpeg-bin/ffmpeg").map_err(|e| e.to_string())?;
                std::io::copy(&mut file, &mut out_bin).map_err(|e| e.to_string())?;
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions("./ffmpeg-bin/ffmpeg", std::fs::Permissions::from_mode(0o755)).ok();
                break;
            }
        }
    }
    #[cfg(target_os = "windows")]
    {
        let file = std::fs::File::open(archive_path).map_err(|e| e.to_string())?;
        let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
        for i in 0..archive.len() {
            let mut file = archive.by_index(i).unwrap();
            let outpath = file.sanitized_name();
            if outpath.file_name().map(|n| n == bin_name).unwrap_or(false) {
                let mut out_bin = std::fs::File::create("./ffmpeg-bin/ffmpeg.exe").map_err(|e| e.to_string())?;
                std::io::copy(&mut file, &mut out_bin).map_err(|e| e.to_string())?;
                break;
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        let file = std::fs::File::open(archive_path).map_err(|e| e.to_string())?;
        let decompressor = xz2::read::XzDecoder::new(file);
        let mut archive = tar::Archive::new(decompressor);
        for entry in archive.entries().map_err(|e| e.to_string())? {
            let mut entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path().map_err(|e| e.to_string())?;
            if path.file_name().map(|n| n == bin_name).unwrap_or(false) {
                let mut out_bin = std::fs::File::create("./ffmpeg-bin/ffmpeg").map_err(|e| e.to_string())?;
                std::io::copy(&mut entry, &mut out_bin).map_err(|e| e.to_string())?;
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions("./ffmpeg-bin/ffmpeg", std::fs::Permissions::from_mode(0o755)).ok();
                break;
            }
        }
    }

    std::fs::remove_file(archive_path).ok();
    if local_path.exists() {
        Ok(local_path)
    } else {
        Err("Failed to download and unpack ffmpeg".to_string())
    }
}

fn convert_with_ffmpeg(input: &str, output: &str, format: &str) -> Result<(), String> {
    let ffmpeg_path = get_ffmpeg_path()?;
    println!("[DEBUG] Using ffmpeg at: {:?}", ffmpeg_path);
    let mut cmd = std::process::Command::new(ffmpeg_path.clone());
    cmd.arg("-y").arg("-i").arg(input);

    match format {
        "mp3" => { cmd.args(&["-vn", "-acodec", "libmp3lame"]); }
        "wav" => { cmd.args(&["-vn", "-acodec", "pcm_s16le"]); }
        _ => {}
    }

    cmd.arg(output);

    println!("[DEBUG] Running: {:?} {:?}", cmd.get_program(), cmd.get_args());

    let output = cmd.output().map_err(|e| format!("Failed to run ffmpeg: {e}"))?;
    if !output.status.success() {
        println!("[ERROR] ffmpeg stderr: {}", String::from_utf8_lossy(&output.stderr));
        return Err(format!(
            "ffmpeg failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

fn download_video(
    client: &Client,
    url: &str,
    status: &Arc<Mutex<String>>,
    progress: &Arc<Mutex<f32>>,
    output_format: &str,
    abort_flag: &Arc<AtomicBool>,
    download_folder: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("[DEBUG] download_video called with url: {url}");
    let re = Regex::new(r"/video/(\d+)")?;
    let caps = re.captures(url).ok_or("Could not extract video ID from URL")?;
    let video_id = &caps[1];

    let api_url = format!(
        "https://api-backend.parti.com/parti_v2/profile/get_livestream_channel_info/recent/{}",
        video_id
    );
    println!("[DEBUG] Fetching API: {api_url}");
    let json: Value = client.get(&api_url).send()?.json()?;
    println!("[DEBUG] API JSON: {json:#}");

    let recording_path = json.get("livestream_recording")
        .or_else(|| json.get("playback_url"))
        .or_else(|| json.get("recording_url"))
        .and_then(|v| v.as_str());

    let recording_path = match recording_path {
        Some(path) => path,
        None => {
            *status.lock().unwrap() = "Could not find a video playlist field in API response.".to_string();
            return Err("No playlist field in API response".into());
        }
    };

    let playback_url = if recording_path.starts_with("http") {
        recording_path.to_string()
    } else {
        format!("https://watch.parti.com/{}", recording_path)
    };

    let title = json.get("event_title").and_then(|v| v.as_str()).unwrap_or("parti_video");
    let timestamp = json.get("event_start_ts").and_then(|v| v.as_i64()).unwrap_or(0);
    let date = if timestamp > 0 {
        let dt = NaiveDateTime::from_timestamp(timestamp, 0);
        Utc.from_utc_datetime(&dt).format("%Y-%m-%d").to_string()
    } else {
        "unknown_date".to_string()
    };
    let filename = format!("{}_{}.ts", sanitize_filename(title), date);
    let filepath = if let Some(folder) = download_folder {
        Path::new(folder).join(&filename)
    } else {
        PathBuf::from(&filename)
    };

    *status.lock().unwrap() = format!("Fetching playlist for '{}'", title);

    println!("[DEBUG] Fetching master playlist: {playback_url}");
    let playlist = client.get(&playback_url).send()?.text()?;

    let mut variant_urls = Vec::new();
    for line in playlist.lines() {
        let line = line.trim();
        if line.ends_with("/playlist.m3u8") {
            let full_url = if line.starts_with("http") {
                line.to_string()
            } else {
                let base = Url::parse(&playback_url)?;
                base.join(line)?.to_string()
            };
            variant_urls.push(full_url);
        }
    }

    if variant_urls.is_empty() {
        variant_urls.push(playback_url.clone());
    }

    let variant_url = &variant_urls[0];
    *status.lock().unwrap() = format!("Fetching segments for '{}'", title);
    println!("[DEBUG] Fetching variant playlist: {variant_url}");
    let resp = client.get(variant_url).send()?;
    println!("[DEBUG] Variant playlist HTTP status: {}", resp.status());
    let text = resp.text()?;
    println!("[DEBUG] Variant playlist content (first 500 chars):\n{}", &text[..text.len().min(500)]);
    if text.trim().is_empty() {
        *status.lock().unwrap() = "Variant playlist is empty or not found.".to_string();
        return Err("Variant playlist is empty".into());
    }
    let variant_playlist = text;

    // Robust .ts segment extraction
    let mut ts_urls = Vec::new();
    let base = Url::parse(variant_url)?;
    for line in variant_playlist.lines() {
        let line = line.trim();
        if line.ends_with(".ts") {
            let seg_url = if line.starts_with("http") {
                line.to_string()
            } else {
                base.join(line)?.to_string()
            };
            ts_urls.push(seg_url);
        }
    }

    *status.lock().unwrap() = format!("Downloading {} segments...", ts_urls.len());
    *progress.lock().unwrap() = 0.0;

    let mut out = BufWriter::new(File::create(&filepath)?);

    for (i, seg_url) in ts_urls.iter().enumerate() {
        if abort_flag.load(Ordering::Relaxed) {
            *status.lock().unwrap() = "Aborted by user.".to_string();
            *progress.lock().unwrap() = 1.0;
            return Ok(());
        }
        {
            let mut progress_guard = progress.lock().unwrap();
            *progress_guard = (i + 1) as f32 / ts_urls.len() as f32;
        }
        {
            let mut status_guard = status.lock().unwrap();
            *status_guard = format!(
                "Downloading segment {}/{}...",
                i + 1,
                ts_urls.len()
            );
        }
        let mut resp = client.get(seg_url).send()?;
        std::io::copy(&mut resp, &mut out)?;
    }
    *progress.lock().unwrap() = 1.0;
    *status.lock().unwrap() = format!("Saved to {}", filepath.display());

    // Convert if needed
    if output_format != "ts" && !abort_flag.load(Ordering::Relaxed) {
        let out_name = format!("{}_{}.{}", sanitize_filename(title), date, output_format);
        let out_path = if let Some(folder) = download_folder {
            Path::new(folder).join(&out_name)
        } else {
            PathBuf::from(&out_name)
        };
        *status.lock().unwrap() = format!("Converting to {}...", output_format);
        match convert_with_ffmpeg(
            &filepath.to_string_lossy(),
            &out_path.to_string_lossy(),
            output_format,
        ) {
            Ok(_) => {
                *status.lock().unwrap() = format!("Saved to {}", out_path.display());
            }
            Err(e) => {
                *status.lock().unwrap() = format!("Conversion failed: {}", e);
            }
        }
    }

    Ok(())
}

struct PartiGuiApp {
    url_input: String,
    status: Arc<Mutex<String>>,
    progress: Arc<Mutex<f32>>,
    batch_video_status: Vec<Arc<Mutex<String>>>,
    batch_video_progress: Vec<Arc<Mutex<f32>>>,
    batch_video_urls: Vec<String>,
    output_format: Arc<Mutex<String>>,
    download_folder: Arc<Mutex<Option<String>>>,
    is_downloading: bool,
    is_batch_downloading: bool,
    abort_single: Arc<AtomicBool>,
    abort_batch: Arc<AtomicBool>,
}

impl Default for PartiGuiApp {
    fn default() -> Self {
        Self {
            url_input: String::new(),
            status: Arc::new(Mutex::new(String::new())),
            progress: Arc::new(Mutex::new(0.0)),
            batch_video_status: Vec::new(),
            batch_video_progress: Vec::new(),
            batch_video_urls: Vec::new(),
            output_format: Arc::new(Mutex::new("ts".to_string())),
            download_folder: Arc::new(Mutex::new(None)),
            is_downloading: false,
            is_batch_downloading: false,
            abort_single: Arc::new(AtomicBool::new(false)),
            abort_batch: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl App for PartiGuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.heading("🎉 Parti Video Downloader");
                ui.add_space(10.0);

                // Download folder picker
                ui.horizontal(|ui| {
                    let folder = self.download_folder.lock().unwrap();
                    let folder_display = folder.as_deref().unwrap_or("[Not set]");
                    ui.label(format!("Download folder: {}", folder_display));
                    drop(folder);
                    if ui.button("Choose Folder...").clicked() {
                        if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                            *self.download_folder.lock().unwrap() = Some(folder.display().to_string());
                        }
                    }
                });

                // Output format dropdown
                ui.horizontal(|ui| {
                    ui.label("Output format:");
                    let mut format = self.output_format.lock().unwrap();
                    egui::ComboBox::from_id_source("format_combo")
                        .selected_text(format.as_str())
                        .show_ui(ui, |ui| {
                            for f in ["ts", "mp4", "mp3", "wav", "wmv", "mov", "webm"] {
                                ui.selectable_value(&mut *format, f.to_string(), f);
                            }
                        });
                });

                ui.group(|ui| {
                    ui.label("Download a single video:");
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.url_input)
                                .hint_text("https://parti.com/video/..."),
                        );
                        if ui.button("Download Video").clicked() && !self.is_downloading {
                            let url = self.url_input.trim().to_string();
                            let format = self.output_format.lock().unwrap().clone();
                            let download_folder = self.download_folder.lock().unwrap().clone();
                            if url.is_empty() {
                                *self.status.lock().unwrap() = "Please enter a video URL.".to_string();
                            } else {
                                *self.status.lock().unwrap() = "Starting download...".to_string();
                                *self.progress.lock().unwrap() = 0.0;
                                self.is_downloading = true;
                                self.abort_single.store(false, Ordering::Relaxed);
                                let status = self.status.clone();
                                let progress = self.progress.clone();
                                let abort_flag = self.abort_single.clone();
                                let url_clone = url.clone();
                                std::thread::spawn(move || {
                                    let client = Client::builder()
                                        .user_agent("Mozilla/5.0 (compatible; parti_video_dl/1.0)")
                                        .build()
                                        .unwrap();
                                    let result = download_video(
                                        &client,
                                        &url_clone,
                                        &status,
                                        &progress,
                                        &format,
                                        &abort_flag,
                                        download_folder.as_deref(),
                                    );
                                    if let Err(e) = result {
                                        println!("[ERROR] Download thread: {e}");
                                        *status.lock().unwrap() = format!("Error: {}", e);
                                        *progress.lock().unwrap() = 1.0;
                                    }
                                });
                            }
                        }
                    });
                    if self.is_downloading {
                        ui.add(egui::ProgressBar::new(*self.progress.lock().unwrap()).show_percentage());
                        if ui.button("Abort").clicked() {
                            self.abort_single.store(true, Ordering::Relaxed);
                        }
                        if *self.progress.lock().unwrap() >= 1.0 {
                            self.is_downloading = false;
                        }
                    }
                    let status = self.status.lock().unwrap();
                    if !status.is_empty() {
                        ui.label(&*status);
                    }
                });

                ui.add_space(20.0);

                ui.group(|ui| {
                    ui.label("Download a list of videos:");
                    if ui.button("Choose .txt File...").clicked() && !self.is_batch_downloading {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("Text", &["txt"])
                            .pick_file()
                        {
                            let file = File::open(&path).unwrap();
                            let reader = io::BufReader::new(file);
                            let urls: Vec<String> = reader
                                .lines()
                                .filter_map(|l| l.ok())
                                .filter(|l| !l.trim().is_empty())
                                .collect();

                            self.batch_video_status = urls.iter().map(|_| Arc::new(Mutex::new(String::new()))).collect();
                            self.batch_video_progress = urls.iter().map(|_| Arc::new(Mutex::new(0.0))).collect();
                            self.batch_video_urls = urls.clone();
                            self.is_batch_downloading = true;
                            self.abort_batch.store(false, Ordering::Relaxed);

                            let status_vec = self.batch_video_status.clone();
                            let progress_vec = self.batch_video_progress.clone();
                            let format = self.output_format.lock().unwrap().clone();
                            let abort_flag = self.abort_batch.clone();
                            let download_folder = self.download_folder.lock().unwrap().clone();

                            std::thread::spawn(move || {
                                let client = Client::builder()
                                    .user_agent("Mozilla/5.0 (compatible; parti_video_dl/1.0)")
                                    .build()
                                    .unwrap();
                                for (i, url) in urls.iter().enumerate() {
                                    if abort_flag.load(Ordering::Relaxed) {
                                        *status_vec[i].lock().unwrap() = "Aborted by user.".to_string();
                                        *progress_vec[i].lock().unwrap() = 1.0;
                                        continue;
                                    }
                                    let status = status_vec[i].clone();
                                    let progress = progress_vec[i].clone();
                                    *status.lock().unwrap() = "Starting...".to_string();
                                    let result = download_video(
                                        &client,
                                        url,
                                        &status,
                                        &progress,
                                        &format,
                                        &abort_flag,
                                        download_folder.as_deref(),
                                    );
                                    if let Err(e) = result {
                                        *status.lock().unwrap() = format!("Error: {}", e);
                                        *progress.lock().unwrap() = 1.0;
                                    }
                                }
                            });
                        }
                    }

                    if self.is_batch_downloading {
                        if ui.button("Abort Batch").clicked() {
                            self.abort_batch.store(true, Ordering::Relaxed);
                        }
                    }

                    if !self.batch_video_urls.is_empty() {
                        egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                            for (i, url) in self.batch_video_urls.iter().enumerate() {
                                let status = self.batch_video_status[i].lock().unwrap();
                                let progress = *self.batch_video_progress[i].lock().unwrap();
                                ui.group(|ui| {
                                    ui.label(format!("Video {}: {}", i + 1, url));
                                    ui.add(egui::ProgressBar::new(progress).show_percentage());
                                    ui.label(&*status);
                                });
                            }
                        });
                        if self.is_batch_downloading
                            && self.batch_video_progress.iter().all(|p| *p.lock().unwrap() >= 1.0)
                        {
                            self.is_batch_downloading = false;
                        }
                    }
                });

                ui.add_space(20.0);
                ui.label("Made with \u{2665} in Rust + egui");
            });
        });
    }
}

// --- main() with icon fix ---
fn main() -> eframe::Result<()> {
    // Load the icon image from assets
    let icon_bytes = include_bytes!("../assets/Icon.png");
    let image = image::load_from_memory(icon_bytes)
        .expect("Failed to load icon")
        .to_rgba8();
    let (width, height) = image.dimensions();
    let rgba = image.into_raw();

    let icon_data = IconData {
        rgba,
        width,
        height,
    };

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([480.0, 480.0])
            .with_icon(icon_data),
        ..Default::default()
    };
    

    eframe::run_native(
        "Parti Video Downloader",
        native_options,
        Box::new(|_cc| Box::new(PartiGuiApp::default())),
    )
}
