//! On-disk cache for downloaded media so it is not fetched twice. Keys are hashed message
//! ids; eviction is LRU-ish (by mtime) and runs lazily on a background thread. Each entry
//! may carry a `<file>.meta` JSON sidecar (account, chat, mimetype…) for the Media screen.

use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    time::SystemTime,
};
use tauri::{
    ipc::{InvokeBody, Request, Response},
    AppHandle, Manager,
};

// ── Media cache (downloaded media kept on disk so it is not fetched twice) ──

fn cache_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_cache_dir()
        .map_err(|e| e.to_string())?
        .join("media");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

/// Keys are message ids + extension. Message ids contain `@`, `:` and other characters
/// that are not filesystem-safe; a lossy substitution would let two distinct ids collide
/// (`…@c.us_x` vs `…_c_us_x`), so hash the id and keep only the extension readable.
fn safe_key(key: &str) -> String {
    // FNV-1a 64-bit: tiny, dependency-free, plenty for a local cache namespace.
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in key.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    let ext = key
        .rsplit_once('.')
        .map(|(_, e)| e)
        .filter(|e| !e.is_empty() && e.len() <= 8 && e.chars().all(|c| c.is_ascii_alphanumeric()))
        .unwrap_or("bin");
    format!("{h:016x}.{ext}")
}

#[derive(Serialize)]
pub struct CacheStats {
    bytes: u64,
    files: u64,
    path: String,
}

const META_EXT: &str = "meta";

fn meta_path(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(".");
    s.push(META_EXT);
    PathBuf::from(s)
}

/// Removes a cached file and its metadata sidecar.
fn remove_entry(path: &Path) -> std::io::Result<()> {
    let _ = fs::remove_file(meta_path(path));
    fs::remove_file(path)
}

/// Media files only: sidecars are not counted as entries.
fn scan(dir: &PathBuf) -> Vec<(PathBuf, u64, SystemTime)> {
    fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter_map(|e| {
                    let md = e.metadata().ok()?;
                    if !md.is_file() || e.path().extension().is_some_and(|x| x == META_EXT) {
                        return None;
                    }
                    Some((
                        e.path(),
                        md.len(),
                        md.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

// Media cache commands are `async` so their file IO runs on the async runtime's thread
// pool, not on the main thread inside the webview's IPC callback (a multi-MB video read
// there stalls the UI, and on Windows blocks WebView2 message pumping).

#[tauri::command]
pub async fn media_cache_has(app: AppHandle, key: String) -> Result<bool, String> {
    Ok(cache_dir(&app)?.join(safe_key(&key)).is_file())
}

/// Returns the cached bytes as a raw ArrayBuffer (no JSON overhead).
#[tauri::command]
pub async fn media_cache_get(app: AppHandle, key: String) -> Result<Response, String> {
    let path = cache_dir(&app)?.join(safe_key(&key));
    let bytes = fs::read(&path).map_err(|e| e.to_string())?;
    // Touch mtime so eviction is LRU-ish.
    // (Needs a writable handle: on Windows `set_modified` fails on a read-only one.)
    let _ = fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .and_then(|f| f.set_modified(SystemTime::now()));
    Ok(Response::new(bytes))
}

/// Body is the raw file; `x-key` header names it, `x-limit` (bytes) caps the cache size,
/// and an optional `x-meta` (ASCII-escaped JSON) is kept beside it as a sidecar.
#[tauri::command]
pub fn media_cache_put(app: AppHandle, request: Request<'_>) -> Result<(), String> {
    let header = |name: &str| {
        request
            .headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
    };
    let key = header("x-key").ok_or("missing x-key")?;
    let limit: u64 = header("x-limit").and_then(|s| s.parse().ok()).unwrap_or(0);
    let bytes = match request.body() {
        InvokeBody::Raw(b) => b.clone(),
        InvokeBody::Json(_) => return Err("expected raw body".into()),
    };
    let dir = cache_dir(&app)?;
    let written = bytes.len() as u64;
    // `Request<'_>` borrows, so this command must stay sync: keep the fast write here and
    // push the directory scan / eviction onto a background thread.
    let path = dir.join(safe_key(&key));
    fs::write(&path, &bytes).map_err(|e| e.to_string())?;
    if let Some(meta) = header("x-meta") {
        let _ = fs::write(meta_path(&path), meta);
    }
    if limit > 0 {
        maybe_evict(dir, limit, written);
    }
    Ok(())
}

/// Bytes written since the last full scan; eviction runs when this exceeds a slice of the
/// limit (hysteresis), never on every put.
static CACHE_UNSCANNED: AtomicU64 = AtomicU64::new(u64::MAX / 2);
static CACHE_EVICTING: AtomicBool = AtomicBool::new(false);

fn maybe_evict(dir: PathBuf, limit: u64, written: u64) {
    let pending = CACHE_UNSCANNED.fetch_add(written, Ordering::Relaxed) + written;
    // Scan at most once per (limit / 16, but ≥ 8 MB) written.
    if pending < (limit / 16).max(8 * 1024 * 1024) {
        return;
    }
    if CACHE_EVICTING.swap(true, Ordering::AcqRel) {
        return; // one at a time
    }
    CACHE_UNSCANNED.store(0, Ordering::Relaxed);
    std::thread::spawn(move || {
        let mut files = scan(&dir);
        let mut total: u64 = files.iter().map(|f| f.1).sum();
        if total > limit {
            files.sort_by_key(|f| f.2); // oldest first
                                        // Evict down to 90% so the next few puts don't immediately trigger another scan.
            let target = limit / 10 * 9;
            for (path, size, _) in files {
                if total <= target {
                    break;
                }
                if remove_entry(&path).is_ok() {
                    total -= size;
                }
            }
        }
        CACHE_EVICTING.store(false, Ordering::Release);
    });
}

#[tauri::command]
pub async fn media_cache_stats(app: AppHandle) -> Result<CacheStats, String> {
    let dir = cache_dir(&app)?;
    let files = scan(&dir);
    Ok(CacheStats {
        bytes: files.iter().map(|f| f.1).sum(),
        files: files.len() as u64,
        path: dir.to_string_lossy().into_owned(),
    })
}

#[derive(Serialize)]
pub struct CacheEntry {
    file: String,
    bytes: u64,
    /// When it was cached (unix ms); creation time where the OS reports one.
    saved: u64,
    meta: Option<serde_json::Value>,
}

fn unix_ms(t: SystemTime) -> u64 {
    t.duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Every cached file with its size, time and metadata sidecar (when it was saved with one).
#[tauri::command]
pub async fn media_cache_list(app: AppHandle) -> Result<Vec<CacheEntry>, String> {
    let dir = cache_dir(&app)?;
    Ok(scan(&dir)
        .into_iter()
        .filter_map(|(path, bytes, modified)| {
            let file = path.file_name()?.to_str()?.to_string();
            let created = fs::metadata(&path).and_then(|m| m.created()).ok();
            let meta = fs::read(meta_path(&path))
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok());
            Some(CacheEntry {
                file,
                bytes,
                saved: unix_ms(created.unwrap_or(modified)),
                meta,
            })
        })
        .collect())
}

/// A listed file name, refused unless it is a plain name inside the cache directory.
fn entry_path(dir: &Path, file: &str) -> Result<PathBuf, String> {
    if file.is_empty()
        || !file.chars().all(|c| c.is_ascii_alphanumeric() || c == '.')
        || file.starts_with('.')
    {
        return Err("invalid file name".into());
    }
    Ok(dir.join(file))
}

/// The bytes of a listed file, by its file name (not its key).
#[tauri::command]
pub async fn media_cache_read(app: AppHandle, file: String) -> Result<Response, String> {
    let path = entry_path(&cache_dir(&app)?, &file)?;
    fs::read(path).map(Response::new).map_err(|e| e.to_string())
}

/// Deletes listed files (and their sidecars); returns how many were removed.
#[tauri::command]
pub async fn media_cache_delete(app: AppHandle, files: Vec<String>) -> Result<u32, String> {
    let dir = cache_dir(&app)?;
    let mut removed = 0;
    for file in files {
        if remove_entry(&entry_path(&dir, &file)?).is_ok() {
            removed += 1;
        }
    }
    Ok(removed)
}

#[tauri::command]
pub async fn media_cache_clear(app: AppHandle) -> Result<(), String> {
    let dir = cache_dir(&app)?;
    fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())
}

/// Turns any PNG/JPEG/GIF/WebP body into a 512×512 transparent-padded WebP, the shape
/// WhatsApp expects for a sticker. A WebP already at that size passes through untouched,
/// which keeps animated stickers intact.
#[tauri::command]
pub fn sticker_from_image(request: Request<'_>) -> Result<tauri::ipc::Response, String> {
    let bytes = match request.body() {
        InvokeBody::Raw(b) => b.clone(),
        InvokeBody::Json(_) => return Err("expected raw body".into()),
    };
    if bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        let img = image::load_from_memory(&bytes).map_err(|e| e.to_string())?;
        if img.width() == 512 && img.height() == 512 {
            return Ok(tauri::ipc::Response::new(bytes));
        }
    }
    let img = image::load_from_memory(&bytes).map_err(|e| format!("unreadable image: {e}"))?;
    let fitted = img
        .resize(512, 512, image::imageops::FilterType::Lanczos3)
        .to_rgba8();
    let mut canvas = image::RgbaImage::new(512, 512);
    let x = (512 - fitted.width()) / 2;
    let y = (512 - fitted.height()) / 2;
    image::imageops::overlay(&mut canvas, &fitted, x as i64, y as i64);
    let mut out = std::io::Cursor::new(Vec::new());
    canvas
        .write_to(&mut out, image::ImageFormat::WebP)
        .map_err(|e| e.to_string())?;
    Ok(tauri::ipc::Response::new(out.into_inner()))
}
