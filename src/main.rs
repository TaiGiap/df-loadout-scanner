#![windows_subsystem = "windows"]

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, OnceLock, RwLock};
use std::thread::{self, sleep};
use std::time::{Duration, Instant};

use arboard::Clipboard;
use eframe::egui::{self, Align, Color32, Frame, Layout, Margin, RichText, Rounding, Stroke, Vec2};
use ort::{
    environment::Environment,
    session::{builder::GraphOptimizationLevel, Session},
    value::Tensor,
};
use serde::{Deserialize, Serialize};
use windows::{
    Win32::Foundation::{BOOL, HWND},
    Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST},
    Win32::UI::WindowsAndMessaging::GetForegroundWindow,
};

// Win32 API functions
extern "system" {
    fn MessageBeep(u_type: u32) -> BOOL;
    fn GetLocalTime(lp_system_time: *mut SystemTime);
    fn GetAsyncKeyState(v_key: i32) -> i16;
}

// Application & Author identity constants
const AUTHOR_NAME: &str = "Anh Bảy Chọ / Tài Giáp";
const APP_NAME: &str = "DF Loadout Scanner";
const APP_VERSION: &str = "v2.0.2-beta";

// VietQR donation constants
const DONATE_BANK_ID: &str = "TCB";
const DONATE_BANK_NAME: &str = "Techcombank (TCB)";
const DONATE_ACCOUNT_NO: &str = "19039673096012";
const DONATE_ACCOUNT_NO_DISPLAY: &str = "1903 9673 0960 12";
const DONATE_ACCOUNT_NAME: &str = "GIAP DUC TAI";
const DONATE_ACCOUNT_NAME_DISPLAY: &str = "GIÁP ĐỨC TÀI";

// Official seasonal database URL
const OFFICIAL_UPDATE_URL: &str =
    "https://raw.githubusercontent.com/TaiGiap/df-loadout-scanner/main/builds.json";

/// Transparently displays community source attribution for builds.
#[inline(always)]
fn format_display_classification(classification: &str, _lang: AppLang) -> String {
    classification.to_string()
}

/// Computes CRC-16/CCITT-FALSE (poly 0x1021, init 0xFFFF, no reflection)
fn crc16_ccitt_false(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &byte in data {
        crc ^= (byte as u16) << 8;
        for _ in 0..8 {
            if (crc & 0x8000) != 0 {
                crc = (crc << 1) ^ 0x1021;
            } else {
                crc <<= 1;
            }
        }
    }
    crc
}

/// Builds EMVCo compliant VietQR NAPAS 247 payload for Techcombank
fn build_vietqr_payload(amount: u64, add_info: &str) -> String {
    // 00: Payload Format Indicator (Fixed "01")
    let mut payload = String::from("000201");
    // 01: Point of Initiation Method (12 = Dynamic QR)
    payload.push_str("010212");
    // 38: Merchant Account Information - NAPAS Beneficiary (TCB BIN 970407, STK 19039673096012)
    payload.push_str("38580010A000000727012800069704070114190396730960120208QRIBFTTA");
    // 53: Transaction Currency (704 = VND)
    payload.push_str("5303704");
    // 54: Transaction Amount
    if amount > 0 {
        let amt_str = amount.to_string();
        payload.push_str(&format!("54{:02}{}", amt_str.len(), amt_str));
    }
    // 58: Country Code (VN)
    payload.push_str("5802VN");
    // 62: Additional Data Field (addInfo)
    if !add_info.is_empty() {
        let sub08 = format!("08{:02}{}", add_info.len(), add_info);
        payload.push_str(&format!("62{:02}{}", sub08.len(), sub08));
    }
    // 63: CRC calculation
    payload.push_str("6304");
    let crc = crc16_ccitt_false(payload.as_bytes());
    payload.push_str(&format!("{:04X}", crc));
    payload
}

/// Generates a sharp egui::ColorImage from EMVCo VietQR payload (100% offline)
fn generate_vietqr_image(amount: u64, add_info: &str) -> Option<egui::ColorImage> {
    let payload = build_vietqr_payload(amount, add_info);
    let code = qrcode::QrCode::with_error_correction_level(payload.as_bytes(), qrcode::EcLevel::M).ok()?;

    let qr_width = code.width();
    let quiet_zone = 4;
    let total_modules = qr_width + quiet_zone * 2;
    let module_pixels = (240 / total_modules).max(1);
    let img_size = total_modules * module_pixels;

    let mut pixels = vec![Color32::WHITE; img_size * img_size];
    for y in 0..qr_width {
        for x in 0..qr_width {
            if code[(x, y)] == qrcode::Color::Dark {
                let px_start = (x + quiet_zone) * module_pixels;
                let py_start = (y + quiet_zone) * module_pixels;
                for dy in 0..module_pixels {
                    for dx in 0..module_pixels {
                        let idx = (py_start + dy) * img_size + (px_start + dx);
                        pixels[idx] = Color32::BLACK;
                    }
                }
            }
        }
    }

    Some(egui::ColorImage {
        size: [img_size, img_size],
        pixels,
    })
}

#[repr(C)]
#[derive(Default, Copy, Clone)]
struct SystemTime {
    w_year: u16,
    w_month: u16,
    w_day_of_week: u16,
    w_day: u16,
    w_hour: u16,
    w_minute: u16,
    w_second: u16,
    w_milliseconds: u16,
}

fn get_current_time_str() -> String {
    let mut st = SystemTime::default();
    unsafe { GetLocalTime(&mut st) };
    format!("{:02}:{:02}", st.w_hour, st.w_minute)
}

const MB_OK: u32 = 0x00000000;
const MB_ICONERROR: u32 = 0x00000010;
const MB_ICONWARNING: u32 = 0x00000030;

// Delta Force Tactical Color Palette
const COLOR_BG: Color32 = Color32::from_rgb(11, 16, 21);          // #0B1015
const COLOR_CARD: Color32 = Color32::from_rgb(19, 27, 36);        // #131B24
const COLOR_CARD_HOVER: Color32 = Color32::from_rgb(26, 37, 50);  // #1A2532
const COLOR_ACCENT: Color32 = Color32::from_rgb(0, 229, 160);      // #00E5A0
const COLOR_ACCENT_DIM: Color32 = Color32::from_rgb(0, 77, 54);   // #004D36
const COLOR_TEXT_MAIN: Color32 = Color32::from_rgb(230, 237, 243);// #E6EDF3
const COLOR_TEXT_MUTED: Color32 = Color32::from_rgb(139, 148, 158);// #8B949E
const COLOR_BORDER: Color32 = Color32::from_rgb(35, 48, 62);      // #23303E
const COLOR_AMBER: Color32 = Color32::from_rgb(255, 190, 40);      // #FFBE28
const COLOR_RED: Color32 = Color32::from_rgb(255, 92, 92);         // #FF5C5C

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AppLang {
    Vi,
    En,
}

impl AppLang {
    fn t<'a>(&self, vi: &'a str, en: &'a str) -> &'a str {
        match self {
            AppLang::Vi => vi,
            AppLang::En => en,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ServerFilter {
    Garena,
    Infinity,
}

impl ServerFilter {
    fn as_str(&self) -> &'static str {
        match self {
            ServerFilter::Garena => "Garena",
            ServerFilter::Infinity => "Infinity",
        }
    }

    fn label(&self) -> &'static str {
        match self {
            ServerFilter::Garena => "Garena",
            ServerFilter::Infinity => "Infinity (Global)",
        }
    }

    fn matches(&self, s: &str) -> bool {
        s.is_empty() || s.eq_ignore_ascii_case(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModeFilter {
    All,
    Warfare,
    Operations,
}

impl ModeFilter {
    fn as_str(&self) -> &'static str {
        match self {
            ModeFilter::All => "All",
            ModeFilter::Warfare => "Warfare",
            ModeFilter::Operations => "Operations",
        }
    }

    fn label(&self, lang: AppLang) -> &'static str {
        match self {
            ModeFilter::All => lang.t("Tất cả chế độ", "All Modes"),
            ModeFilter::Warfare => lang.t("Chiến Trường (Warfare)", "Warfare"),
            ModeFilter::Operations => lang.t("Sinh Tồn (Operations)", "Operations"),
        }
    }

    fn matches(&self, m: &str) -> bool {
        match self {
            ModeFilter::All => true,
            _ => m.is_empty() || m.eq_ignore_ascii_case(self.as_str()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppTab {
    Scanner,
    Favorites,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourceFilter {
    All,
    DFTools,
    DFBuilds,
    FPSMeta,
}

impl SourceFilter {
    fn as_str(&self) -> &'static str {
        match self {
            SourceFilter::All => "ALL",
            SourceFilter::DFTools => "DFTools",
            SourceFilter::DFBuilds => "DFBuilds",
            SourceFilter::FPSMeta => "FPSMeta",
        }
    }

    fn from_str(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "DFTOOLS" => SourceFilter::DFTools,
            "DFBUILDS" => SourceFilter::DFBuilds,
            "FPSMETA" => SourceFilter::FPSMeta,
            _ => SourceFilter::All,
        }
    }

    fn label(&self, lang: AppLang) -> &'static str {
        match self {
            SourceFilter::All => lang.t("Tất cả nguồn", "All Sources"),
            SourceFilter::DFTools => "DFTools",
            SourceFilter::DFBuilds => "DFBuilds",
            SourceFilter::FPSMeta => "FPSMeta",
        }
    }

    fn matches(&self, b: &BuildItem) -> bool {
        match self {
            SourceFilter::All => true,
            SourceFilter::DFTools => {
                b.tags.iter().any(|t| t.eq_ignore_ascii_case("DFTools"))
                    || b.classification.contains("DFTools")
            }
            SourceFilter::DFBuilds => {
                b.tags.iter().any(|t| t.eq_ignore_ascii_case("DFBuilds"))
                    || b.classification.contains("DFBuilds")
            }
            SourceFilter::FPSMeta => {
                b.tags.iter().any(|t| t.eq_ignore_ascii_case("FPSMeta"))
                    || b.classification.contains("FPSMeta")
            }
        }
    }
}

#[derive(Debug, Clone)]
struct ActiveFilters {
    server: ServerFilter,
    mode: ModeFilter,
    source: SourceFilter,
    selected_monitor_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
struct BuildItem {
    #[serde(default)]
    server: String,
    #[serde(default)]
    mode: String,
    #[serde(default)]
    weapon_type: String,
    weapon: String,
    #[serde(default)]
    classification: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    copy_count: u64,
    loadout_code: String,
    #[serde(default)]
    is_custom: bool,
    #[serde(default)]
    is_favorite: bool,
}

impl BuildItem {
    fn source_name(&self) -> &'static str {
        if self.tags.iter().any(|t| t.eq_ignore_ascii_case("DFTools")) || self.classification.contains("DFTools") {
            "DFTools"
        } else if self.tags.iter().any(|t| t.eq_ignore_ascii_case("DFBuilds")) || self.classification.contains("DFBuilds") {
            "DFBuilds"
        } else if self.tags.iter().any(|t| t.eq_ignore_ascii_case("FPSMeta")) || self.classification.contains("FPSMeta") {
            "FPSMeta"
        } else {
            "Other"
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct HistoryItem {
    timestamp: String,
    weapon: String,
    classification: String,
    #[serde(default)]
    copy_count: u64,
    loadout_code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UserConfig {
    #[serde(default = "default_server")]
    server: String,
    #[serde(default = "default_mode")]
    mode: String,
    #[serde(default = "default_source")]
    source: String,
    #[serde(default = "default_lang")]
    lang: String,
    #[serde(default = "default_hotkey_vk")]
    hotkey_vk: i32,
    #[serde(default = "default_hotkey_name")]
    hotkey_name: String,
    #[serde(default)]
    selected_monitor_id: Option<String>,
    #[serde(default = "default_true")]
    auto_check_update: bool,
}

fn default_server() -> String { "Garena".to_string() }
fn default_mode() -> String { "Warfare".to_string() }
fn default_source() -> String { "ALL".to_string() }
fn default_lang() -> String { "vi".to_string() }
fn default_hotkey_vk() -> i32 { 0x78 } // VK_F9
fn default_hotkey_name() -> String { "F9".to_string() }
fn default_true() -> bool { true }

impl Default for UserConfig {
    fn default() -> Self {
        Self {
            server: default_server(),
            mode: default_mode(),
            source: default_source(),
            lang: default_lang(),
            hotkey_vk: default_hotkey_vk(),
            hotkey_name: default_hotkey_name(),
            selected_monitor_id: None,
            auto_check_update: true,
        }
    }
}

fn format_number_with_commas(n: u64) -> String {
    let s = n.to_string();
    let bytes = s.as_bytes();
    let len = bytes.len();
    let mut result = String::with_capacity(len + len / 3);
    for (i, &b) in bytes.iter().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            result.push(',');
        }
        result.push(b as char);
    }
    result
}

/// Detects if a supported hotkey or mouse button is currently pressed.
/// Returns:
/// - Some(None) if Escape is pressed (cancel rebinding)
/// - Some(Some((vk, name))) if a valid hotkey is detected
/// - None if no supported key is pressed
fn detect_pressed_hotkey() -> Option<Option<(i32, &'static str)>> {
    // Check Escape first -> cancel rebinding
    if unsafe { (GetAsyncKeyState(0x1B) as u16 & 0x8000) != 0 } {
        return Some(None);
    }

    const CANDIDATES: &[(i32, &'static str)] = &[
        // Mouse side buttons
        (0x05, "Mouse 4"),
        (0x06, "Mouse 5"),
        // Function keys F1 - F12
        (0x70, "F1"),
        (0x71, "F2"),
        (0x72, "F3"),
        (0x73, "F4"),
        (0x74, "F5"),
        (0x75, "F6"),
        (0x76, "F7"),
        (0x77, "F8"),
        (0x78, "F9"),
        (0x79, "F10"),
        (0x7A, "F11"),
        (0x7B, "F12"),
        // Navigation & Editing
        (0x2D, "Insert"),
        (0x2E, "Delete"),
        (0x24, "Home"),
        (0x23, "End"),
        (0x21, "PageUp"),
        (0x22, "PageDown"),
        // Numpad 0 - 9 & arithmetic
        (0x60, "Numpad 0"),
        (0x61, "Numpad 1"),
        (0x62, "Numpad 2"),
        (0x63, "Numpad 3"),
        (0x64, "Numpad 4"),
        (0x65, "Numpad 5"),
        (0x66, "Numpad 6"),
        (0x67, "Numpad 7"),
        (0x68, "Numpad 8"),
        (0x69, "Numpad 9"),
        (0x6A, "Numpad *"),
        (0x6B, "Numpad +"),
        (0x6D, "Numpad -"),
        (0x6E, "Numpad ."),
        (0x6F, "Numpad /"),
        // Additional keys
        (0x14, "CapsLock"),
        (0x09, "Tab"),
        (0x13, "Pause"),
        (0x91, "ScrollLock"),
        (0x2C, "PrintScreen"),
        (0xC0, "` (Tilde)"),
    ];

    for &(vk, name) in CANDIDATES {
        if unsafe { (GetAsyncKeyState(vk) as u16 & 0x8000) != 0 } {
            return Some(Some((vk, name)));
        }
    }

    None
}

fn app_data_dir() -> PathBuf {
    let base_dir = if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            parent.to_path_buf()
        } else {
            PathBuf::from(".")
        }
    } else {
        PathBuf::from(".")
    };

    let data_dir = base_dir.join("DF-Loadout-Scanner-Data");
    let _ = fs::create_dir_all(&data_dir);

    // Migration logic: move legacy JSON files from base_dir into data_dir if present
    for filename in &["builds_local.json", "config.json", "df_history.json", "favorites.json"] {
        let old_file = base_dir.join(filename);
        let new_file = data_dir.join(filename);
        if old_file.exists() && !new_file.exists() {
            let _ = fs::rename(&old_file, &new_file);
        }
    }

    data_dir
}

/// Atomically writes `content` to `target_path` via a `.tmp` sibling file,
/// falling back to `fs::copy` if `fs::rename` fails, verifying the target file exists
/// with size > 0 bytes before deleting `.tmp`, and cleaning `.tmp` on failure.
fn atomic_write_verified(target_path: &std::path::Path, content: &str) -> Result<(), String> {
    if let Some(parent) = target_path.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            return Err(format!("Không thể tạo thư mục lưu dữ liệu: {}", e));
        }
    }

    let tmp_path = PathBuf::from(format!("{}.tmp", target_path.display()));
    if let Err(e) = fs::write(&tmp_path, content) {
        let _ = fs::remove_file(&tmp_path);
        return Err(format!("Lỗi ghi file tạm: {}", e));
    }

    if fs::rename(&tmp_path, target_path).is_ok() {
        match fs::metadata(target_path) {
            Ok(meta) if meta.len() > 0 => return Ok(()),
            Ok(_) => return Err("File đích rỗng (0 bytes) sau khi rename".to_string()),
            Err(e) => return Err(format!("Không thể xác minh file đích sau rename: {}", e)),
        }
    }

    match fs::copy(&tmp_path, target_path) {
        Ok(bytes_copied) if bytes_copied > 0 => match fs::metadata(target_path) {
            Ok(meta) if meta.len() > 0 => {
                let _ = fs::remove_file(&tmp_path);
                Ok(())
            }
            Ok(_) => {
                let _ = fs::remove_file(&tmp_path);
                Err("File đích rỗng (0 bytes) sau khi sao chép".to_string())
            }
            Err(e) => {
                let _ = fs::remove_file(&tmp_path);
                Err(format!("Không thể xác minh file đích sau sao chép: {}", e))
            }
        },
        Ok(_) => {
            let _ = fs::remove_file(&tmp_path);
            Err("Sao chép từ file tạm tạo ra file 0 bytes".to_string())
        }
        Err(copy_err) => {
            let _ = fs::remove_file(&tmp_path);
            Err(format!("Không thể ghi đè file đích (có thể bị khóa): {}", copy_err))
        }
    }
}

fn favorites_file_path() -> PathBuf {
    app_data_dir().join("favorites.json")
}

fn load_favorites() -> Vec<BuildItem> {
    let path = favorites_file_path();
    if let Ok(content) = fs::read_to_string(&path) {
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        Vec::new()
    }
}

fn save_favorites(favorites: &[BuildItem]) -> Result<(), String> {
    let path = favorites_file_path();
    let content = serde_json::to_string_pretty(favorites)
        .map_err(|e| format!("Lỗi tuần tự hóa favorites: {}", e))?;
    atomic_write_verified(&path, &content)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfidenceTier {
    High,       // Đủ điều kiện tự động sao chép vào Clipboard
    Suspicious, // Vùng nghi ngờ: cần người dùng bấm [✔ XÁC NHẬN & COPY]
    Rejected,   // Từ chối / không nhận diện (< 0.75)
}

pub fn classify_confidence(score: f64) -> ConfidenceTier {
    if score >= 0.90 {
        ConfidenceTier::High
    } else if score >= 0.75 {
        ConfidenceTier::Suspicious
    } else {
        ConfidenceTier::Rejected
    }
}

/// Evaluates v2.0.2-beta `>= 0.75` auto-copy gate guarded by 3 mandatory safety barriers:
/// 1) Weapon category locked (or explicit special override like `Cung Phức Hợp` / `Lever-action` / `Shotgun Hai Nòng`)
/// 2) Top-1 vs Top-2 score margin (`>= 0.05` when `score >= 0.80`, otherwise `>= 0.12`)
/// 3) Extracted code has `>= 2` alphanumeric characters AND `ctc_confidence >= 0.55`
pub fn evaluate_auto_copy_guards(
    score: f64,
    margin: f64,
    category_locked: bool,
    code_alnum_len: usize,
    ctc_confidence: f32,
) -> ConfidenceTier {
    let min_margin = if score >= 0.80 { 0.05 } else { 0.12 };
    if score < 0.75 {
        ConfidenceTier::Rejected
    } else if category_locked
        && margin >= min_margin
        && code_alnum_len >= 2
        && ctc_confidence >= 0.55
    {
        ConfidenceTier::High
    } else {
        ConfidenceTier::Suspicious
    }
}

fn config_file_path() -> PathBuf {
    app_data_dir().join("config.json")
}

fn load_user_config() -> UserConfig {
    let path = config_file_path();
    if let Ok(content) = fs::read_to_string(&path) {
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        UserConfig::default()
    }
}

fn save_user_config(config: &UserConfig) -> Result<(), String> {
    let path = config_file_path();
    let content = serde_json::to_string_pretty(config)
        .map_err(|e| format!("Lỗi tuần tự hóa config: {}", e))?;
    atomic_write_verified(&path, &content)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReleaseUpdateInfo {
    tag_name: String,
    name: String,
    summary: String,
    html_url: String,
    zip_download_url: Option<String>,
    zip_file_name: Option<String>,
    is_prerelease: bool,
}

enum WorkerEvent {
    WeaponScanned {
        weapon: String,
        score: f64,
        raw_ocr: String,
        builds: Vec<BuildItem>,
        is_uncertain: bool,
        clipboard_error: Option<String>,
        clipboard_report: Option<(usize, usize)>,
    },
    ScanFailed {
        raw_ocr: String,
        cleaned: String,
    },
    WinVCopyCompleted {
        top_build: Option<BuildItem>,
        msg: String,
        is_error: bool,
    },
    OcrUnavailable(String),
    UpdateCheckFinished {
        manual: bool,
        result: Result<Option<ReleaseUpdateInfo>, String>,
    },
    UpdateDownloadFinished {
        target_tag: String,
        result: Result<PathBuf, String>,
    },
}

enum DbUpdateResult {
    Success { builds: Vec<BuildItem>, count: usize },
    Error(String),
}

const BUILDS_JSON: &str = include_str!("../builds.json");

/// Extracts the weapon name segment preceding mode keywords or the last 2 `-` segments.
fn extract_weapon_prefix_from_loadout_code(code: &str) -> String {
    let trimmed = code.trim();
    let lower = trimmed.to_lowercase();
    for marker in [
        "-warfare-",
        "-operations",
        "-detonation",
        "-extraction",
        "-krieg-",
        "-conflicto",
        "-operaci",
        "-chiến trường",
        "-chiến dịch",
        "-sinh tồn",
        "-全面",
        "-烽火",
        "-전면전",
        "-сражени",
        "-операци",
        "-«операци",
        "-savaş",
        "-الحرب",
        "-العمليات",
    ] {
        if let Some(pos) = lower.find(marker) {
            return trimmed[..pos].trim().to_string();
        }
    }

    // Fallback: strip trailing alphanumeric share-code token, then trailing mode token
    if let Some((before_id, last_id)) = trimmed.rsplit_once('-') {
        if last_id.len() >= 12 && last_id.chars().all(|c| c.is_ascii_alphanumeric()) {
            if let Some((weapon_part, _mode_part)) = before_id.rsplit_once('-') {
                return weapon_part.trim().to_string();
            }
            return before_id.trim().to_string();
        }
    }

    let parts: Vec<&str> = trimmed.rsplitn(3, '-').collect();
    if parts.len() == 3 {
        parts[2].trim().to_string()
    } else {
        trimmed.to_string()
    }
}

/// Semantically checks that the weapon inside `loadout_code` matches the declared `weapon` field.
fn code_matches_declared_weapon(
    code: &str,
    declared_weapon: &str,
    candidates: &[String],
) -> bool {
    let raw_prefix = extract_weapon_prefix_from_loadout_code(code).replace('\u{00A0}', " ");
    if raw_prefix.trim().is_empty() {
        return false;
    }

    let declared_upper = declared_weapon.trim().to_uppercase();
    let declared_canon = resolve_alias(&declared_upper)
        .map(|s| s.to_string())
        .unwrap_or(declared_upper);
    let declared_stripped = ocr_strip_separators(&declared_canon);

    const LOCALIZED_PREFIX_MAP: &[(&str, &str)] = &[
        ("騰龍", "CI-19"),
        ("데저트 이글", "DESERT EAGLE"),
        ("АШ-12", "ASH-12"),
        ("АК-12", "AK-12"),
        ("АКМ", "AKM"),
        ("СР-3М", "SR-3M"),
        ("СР-3M", "SR-3M"),
        ("ВСС", "VSS"),
        ("ВИТЯЗЬ", "VITYAZ"),
        ("АС «ВАЛ»", "AS VAL"),
        ("АС ВАЛ", "AS VAL"),
    ];
    let prefix_upper_raw = raw_prefix.to_uppercase();
    for &(loc, canon) in LOCALIZED_PREFIX_MAP {
        if prefix_upper_raw.contains(loc) {
            return ocr_strip_separators(canon) == declared_stripped;
        }
    }

    let ascii_only: String = raw_prefix
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' || c == ' ' {
                c
            } else {
                ' '
            }
        })
        .collect();
    let mut ascii_clean = ascii_only.split_whitespace().collect::<Vec<_>>().join(" ");
    // Separate glued English/German category suffixes (e.g., "M4A1AssaultRifle", "CAR-15-Sturmgewehr")
    for suffix in [
        "CompactAssaultRifle",
        "GeneralMachineGun",
        "LightMachineGun",
        "AssaultRifle",
        "BattleRifle",
        "MarksmanRifle",
        "SniperRifle",
        "SubmachineGun",
        "-Maschinenpistole",
        "-Sturmgewehr",
        "-Kampfgewehr",
        "-Präzisionsgewehr",
        "-Prazisionsgewehr",
        "-Scharfschützengewehr",
        "-Scharfschutzengewehr",
        "-Leichtes Maschinengewehr",
        "-Schrotflinte",
    ] {
        ascii_clean = ascii_clean.replace(suffix, " ");
    }

    // Exact separator-stripped token / 2-word phrase match (handles Spanish/Turkish/Arabic prefixes like "Fusil de asalto KC17" or "SG 552 Taarruz")
    let ascii_tokens: Vec<&str> = ascii_clean.split_whitespace().collect();
    for i in 0..ascii_tokens.len() {
        let tok_upper = ascii_tokens[i].to_uppercase();
        let tok_canon = resolve_alias(&tok_upper).unwrap_or(tok_upper.as_str());
        if ocr_strip_separators(tok_canon) == declared_stripped {
            return true;
        }
        if i + 1 < ascii_tokens.len() {
            let pair = format!("{} {}", ascii_tokens[i], ascii_tokens[i + 1]).to_uppercase();
            let pair_canon = resolve_alias(&pair).unwrap_or(pair.as_str());
            if ocr_strip_separators(pair_canon) == declared_stripped {
                return true;
            }
        }
    }

    let cleaned = clean_ocr_text(if ascii_clean.is_empty() {
        &raw_prefix
    } else {
        &ascii_clean
    });

    if cleaned.is_empty() {
        return false;
    }

    if let Some((matched, _)) = match_weapon(&cleaned, candidates) {
        let matched_canon = resolve_alias(&matched.to_uppercase())
            .map(|s| s.to_string())
            .unwrap_or_else(|| matched.to_uppercase());
        return ocr_strip_separators(&matched_canon) == declared_stripped;
    }

    false
}

/// Validates downloaded database records semantically & structurally before persisting:
/// - Non-empty payload
/// - Valid `weapon` and `loadout_code` (`>= 18` chars, `>= 2` dashes, single-line)
/// - Cross-checks weapon name inside `loadout_code` against `weapon` field
/// - Deduplicates strictly by trimmed `loadout_code`
fn validate_downloaded_builds(raw_items: Vec<BuildItem>) -> Result<Vec<BuildItem>, String> {
    if raw_items.is_empty() {
        return Err("Dữ liệu rỗng / Empty database".to_string());
    }

    // Build candidate pool from known aliases + embedded weapons + incoming weapon names
    let mut cand_set: HashSet<String> = HashSet::new();
    for &(_, canon) in WEAPON_ALIASES {
        cand_set.insert(canon.to_string());
    }
    if let Ok(embedded) = serde_json::from_str::<Vec<BuildItem>>(BUILDS_JSON) {
        for b in embedded {
            let w = b.weapon.trim().to_string();
            if !w.is_empty() {
                cand_set.insert(w);
            }
        }
    }
    for b in &raw_items {
        let w = b.weapon.trim().to_string();
        if !w.is_empty() {
            cand_set.insert(w);
        }
    }
    let candidates: Vec<String> = cand_set.into_iter().collect();

    let mut seen_codes: HashSet<String> = HashSet::new();
    let valid: Vec<BuildItem> = raw_items
        .into_iter()
        .filter_map(|mut item| {
            let weapon_trimmed = item.weapon.trim().to_string();
            let code_trimmed = item.loadout_code.trim().to_string();
            if weapon_trimmed.is_empty()
                || code_trimmed.len() < 18
                || code_trimmed.matches('-').count() < 2
                || code_trimmed.contains('\n')
                || code_trimmed.contains('\r')
            {
                return None;
            }
            if !code_matches_declared_weapon(&code_trimmed, &weapon_trimmed, &candidates) {
                return None;
            }
            if !seen_codes.insert(code_trimmed.clone()) {
                return None;
            }
            item.weapon = weapon_trimmed;
            item.loadout_code = code_trimmed;
            Some(item)
        })
        .collect();

    if valid.is_empty() {
        return Err("Không có bản ghi hợp lệ sau kiểm duyệt ngữ nghĩa / No valid builds after semantic validation".to_string());
    }

    Ok(valid)
}

fn history_file_path() -> PathBuf {
    app_data_dir().join("df_history.json")
}

fn local_builds_file_path() -> PathBuf {
    app_data_dir().join("builds_local.json")
}

fn load_builds_data() -> (Vec<BuildItem>, bool, bool) {
    let local_path = local_builds_file_path();
    let mut was_corrupted = false;

    if local_path.exists() {
        let is_valid = match fs::read_to_string(&local_path) {
            Ok(content) => match serde_json::from_str::<Vec<BuildItem>>(&content) {
                Ok(items) if !items.is_empty() => Some(items),
                _ => None,
            },
            Err(_) => None,
        };

        if let Some(items) = is_valid {
            return (items, true, false);
        } else {
            // File hỏng hoặc rỗng -> tự động xóa file và khôi phục từ bản nhúng
            let _ = fs::remove_file(&local_path);
            was_corrupted = true;
        }
    }

    let embedded: Vec<BuildItem> = serde_json::from_str(BUILDS_JSON).unwrap_or_default();
    (embedded, false, was_corrupted)
}

const PADDLE_REC_MODEL_BYTES: &[u8] = include_bytes!("../assets/models/en_PP-OCRv4_rec.onnx");
const PADDLE_EN_DICT: &str = include_str!("../assets/models/en_dict.txt");

static ORT_ENV: OnceLock<Result<Environment, String>> = OnceLock::new();

/// Resolves `onnxruntime.dll` next to the executable (with test-harness & env fallbacks).
fn resolve_onnxruntime_dll_path() -> PathBuf {
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let direct = exe_dir.join("onnxruntime.dll");
            if direct.is_file() {
                return direct;
            }
            if cfg!(test) {
                for ancestor in exe_dir.ancestors().skip(1).take(6) {
                    let cand = ancestor.join("onnxruntime.dll");
                    if cand.is_file() {
                        return cand;
                    }
                }
            }
        }
    }
    if cfg!(test) {
        if let Some(manifest_dir) = option_env!("CARGO_MANIFEST_DIR") {
            let dev_dll = PathBuf::from(manifest_dir).join("target").join("debug").join("onnxruntime.dll");
            if dev_dll.is_file() {
                return dev_dll;
            }
        }
    }
    let cwd_dll = PathBuf::from("onnxruntime.dll");
    if cwd_dll.is_file() {
        return cwd_dll;
    }
    if let Some(env_path) = std::env::var_os("ORT_DYLIB_PATH") {
        let p = PathBuf::from(env_path);
        if p.is_file() {
            return p;
        }
    }
    PathBuf::from("onnxruntime.dll")
}

fn get_ort_environment() -> Result<&'static Environment, String> {
    let res = ORT_ENV.get_or_init(|| {
        let dll_path = resolve_onnxruntime_dll_path();
        ort::init_from(&dll_path)
            .map_err(|e| format!("Không nạp được {}: {}", dll_path.display(), e))?
            .with_name("df_loadout_scanner")
            .build()
            .map_err(|e| format!("Lỗi khởi tạo ONNX Runtime environment: {}", e))
    });
    match res {
        Ok(env) => Ok(env),
        Err(e) => Err(e.clone()),
    }
}

struct PaddleOcrEngine {
    session: Session,
    dict: Vec<&'static str>,
}

impl PaddleOcrEngine {
    fn new() -> Result<Self, String> {
        let env = get_ort_environment()?;
        let session = Session::builder(env)
            .map_err(|e| format!("Lỗi tạo ONNX SessionBuilder: {}", e))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| format!("Lỗi tối ưu ONNX graph: {}", e))?
            .with_intra_threads(2)
            .map_err(|e| format!("Lỗi thiết lập CPU threads: {}", e))?
            .commit_from_memory(PADDLE_REC_MODEL_BYTES)
            .map_err(|e| format!("Lỗi nạp mô hình en_PP-OCRv4_rec.onnx: {}", e))?;

        let dict: Vec<&'static str> = PADDLE_EN_DICT.lines().collect();
        Ok(Self { session, dict })
    }

    fn recognize(&mut self, img: &xcap::image::RgbaImage) -> Result<(String, f32), String> {
        let (target_w, nchw_data) = preprocess_for_paddle_ocr(img);
        let input_tensor = Tensor::from_array(([1usize, 3usize, 48usize, target_w], nchw_data))
            .map_err(|e| format!("Lỗi tạo tensor đầu vào: {}", e))?;
        let outputs = self
            .session
            .run(ort::inputs![input_tensor])
            .map_err(|e| format!("Lỗi suy luận ONNX Runtime: {}", e))?;
        let (out_shape, out_data) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| format!("Lỗi trích xuất tensor đầu ra: {}", e))?;
        ctc_greedy_decode(out_shape, out_data, &self.dict)
    }
}

/// Decodes `[1, T, C]` CTC output probabilities/logits into trimmed text and mean confidence.
fn ctc_greedy_decode(shape: &[i64], data: &[f32], dict: &[&str]) -> Result<(String, f32), String> {
    if shape.len() != 3 || shape[0] < 1 || shape[1] <= 0 || shape[2] <= 1 {
        return Err(format!("Kích thước tensor CTC không hợp lệ: {:?}", shape));
    }
    let seq_len = shape[1] as usize;
    let num_classes = shape[2] as usize;
    if data.len() < seq_len * num_classes {
        return Err("Dữ liệu tensor CTC bị thiếu".to_string());
    }

    let mut text = String::with_capacity(seq_len);
    let mut prev_idx: usize = 0;
    let mut conf_sum = 0.0_f32;
    let mut conf_count = 0usize;

    for t in 0..seq_len {
        let row = &data[t * num_classes..(t + 1) * num_classes];
        let mut best_idx = 0usize;
        let mut best_val = f32::NEG_INFINITY;
        for (c, &val) in row.iter().enumerate() {
            if val > best_val {
                best_val = val;
                best_idx = c;
            }
        }

        if best_idx != 0 && best_idx != prev_idx {
            let dict_idx = best_idx - 1;
            if let Some(&ch) = dict.get(dict_idx) {
                text.push_str(ch);
            } else if dict_idx == dict.len() {
                text.push(' ');
            }
            conf_sum += best_val.clamp(0.0, 1.0);
            conf_count += 1;
        }
        prev_idx = best_idx;
    }

    let avg_conf = if conf_count > 0 {
        conf_sum / conf_count as f32
    } else {
        0.0
    };
    Ok((text.trim().to_string(), avg_conf))
}

/// Boosts local brightness and contrast on the left `left_ratio` fraction (e.g. `0.30` for Pass 1B)
/// using histogram percentile stretch + gamma correction (`gamma = 0.55`) to recover dark text under the 18+ overlay.
fn boost_left_region_gamma(
    img: &xcap::image::RgbaImage,
    left_ratio: f32,
    gamma: f32,
) -> xcap::image::RgbaImage {
    let w = img.width().max(1);
    let h = img.height().max(1);
    let left_end = (((w as f32) * left_ratio).round() as u32).clamp(1, w);

    let mut hist = [0u32; 256];
    let mut total = 0u32;
    for y in 0..h {
        for x in 0..left_end {
            let p = img.get_pixel(x, y);
            let luma = (p[0] as f32 * 0.299 + p[1] as f32 * 0.587 + p[2] as f32 * 0.114)
                .round()
                .clamp(0.0, 255.0) as usize;
            hist[luma] += 1;
            total += 1;
        }
    }

    let p_low_target = ((total as f32) * 0.05) as u32;
    let p_high_target = ((total as f32) * 0.98).max(1.0) as u32;
    let mut acc = 0u32;
    let mut lo = 0.0_f32;
    let mut hi = 255.0_f32;
    let mut found_lo = false;
    for (val, &cnt) in hist.iter().enumerate() {
        acc += cnt;
        if !found_lo && acc >= p_low_target {
            lo = val as f32;
            found_lo = true;
        }
        if acc >= p_high_target {
            hi = val as f32;
            break;
        }
    }

    let mut out = img.clone();
    let span = hi - lo;
    for y in 0..h {
        for x in 0..left_end {
            let p = img.get_pixel(x, y);
            let mut rgb = [0u8; 3];
            for c in 0..3 {
                let norm = if span >= 18.0 {
                    ((p[c] as f32 - lo) / span).clamp(0.0, 1.0)
                } else {
                    (p[c] as f32) / 255.0
                };
                rgb[c] = (norm.powf(gamma) * 255.0).round().clamp(0.0, 255.0) as u8;
            }
            out.put_pixel(x, y, xcap::image::Rgba([rgb[0], rgb[1], rgb[2], 255]));
        }
    }
    out
}

/// Preprocesses a clean RGB crop for PaddleOCR v4 (`[1, 3, 48, W]` NCHW):
/// - Preserves full RGB intensity gradients (no hard binary white thresholding that erases shadowed glyphs under 18+ box)
/// - Bilinearly resizes preserving aspect ratio to height `48px`
/// - Right-pads with dark HUD background (`-1.0`) up to `max(resized_w, 320)` (multiple of 8)
/// - Normalizes pixel channels to `[-1.0, 1.0]` via `(v / 255.0 - 0.5) / 0.5`.
fn preprocess_for_paddle_ocr(img: &xcap::image::RgbaImage) -> (usize, Vec<f32>) {
    let w = img.width().max(1);
    let h = img.height().max(1);

    let dst_h: usize = 48;
    let resized_w = ((w as f64 * dst_h as f64) / h as f64)
        .round()
        .clamp(16.0, 640.0) as usize;
    let target_w = ((resized_w.max(320) + 7) / 8) * 8;

    let plane_size = dst_h * target_w;
    // Pad extra right width with dark background (`0` -> -1.0 normalized) matching HUD contrast
    let pad_norm = -1.0_f32;
    let mut nchw = vec![pad_norm; 3 * plane_size];

    let scale_x = w as f64 / resized_w as f64;
    let scale_y = h as f64 / dst_h as f64;

    for dy in 0..dst_h {
        let sy = ((dy as f64 + 0.5) * scale_y - 0.5).clamp(0.0, (h - 1) as f64);
        let y0 = sy.floor() as u32;
        let y1 = (y0 + 1).min(h - 1);
        let fy = (sy - y0 as f64) as f32;
        let row_off = dy * target_w;

        for dx in 0..resized_w {
            let sx = ((dx as f64 + 0.5) * scale_x - 0.5).clamp(0.0, (w - 1) as f64);
            let x0 = sx.floor() as u32;
            let x1 = (x0 + 1).min(w - 1);
            let fx = (sx - x0 as f64) as f32;

            let p00 = img.get_pixel(x0, y0);
            let p10 = img.get_pixel(x1, y0);
            let p01 = img.get_pixel(x0, y1);
            let p11 = img.get_pixel(x1, y1);

            let idx = row_off + dx;
            for ch in 0..3 {
                let top = p00[ch] as f32 * (1.0 - fx) + p10[ch] as f32 * fx;
                let bot = p01[ch] as f32 * (1.0 - fx) + p11[ch] as f32 * fx;
                let val = top * (1.0 - fy) + bot * fy;
                nchw[ch * plane_size + idx] = (val / 255.0_f32 - 0.5_f32) / 0.5_f32;
            }
        }
    }

    (target_w, nchw)
}

fn check_ocr_available() -> Result<(), String> {
    PaddleOcrEngine::new().map(|_| ())
}

fn load_history() -> Vec<HistoryItem> {
    let path = history_file_path();
    if let Ok(content) = fs::read_to_string(&path) {
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        Vec::new()
    }
}

fn save_history(history: &[HistoryItem]) -> Result<(), String> {
    let path = history_file_path();
    let content = serde_json::to_string_pretty(history)
        .map_err(|e| format!("Lỗi tuần tự hóa history: {}", e))?;
    atomic_write_verified(&path, &content)
}

/// Strips Vietnamese accents and diacritics, returning ASCII uppercase string.
fn strip_vietnamese_diacritics(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        let ch = match c {
            'á' | 'à' | 'ả' | 'ã' | 'ạ' |
            'ă' | 'ắ' | 'ằ' | 'ẳ' | 'ẵ' | 'ặ' |
            'â' | 'ấ' | 'ầ' | 'ẩ' | 'ẫ' | 'ậ' |
            'Á' | 'À' | 'Ả' | 'Ã' | 'Ạ' |
            'Ă' | 'Ắ' | 'Ằ' | 'Ẳ' | 'Ẵ' | 'Ặ' |
            'Â' | 'Ấ' | 'Ầ' | 'Ẩ' | 'Ẫ' | 'Ậ' => 'A',

            'đ' | 'Đ' => 'D',

            'é' | 'è' | 'ẻ' | 'ẽ' | 'ẹ' |
            'ê' | 'ế' | 'ề' | 'ể' | 'ễ' | 'ệ' |
            'É' | 'È' | 'Ẻ' | 'Ẽ' | 'Ẹ' |
            'Ê' | 'Ế' | 'Ề' | 'Ể' | 'Ễ' | 'Ệ' => 'E',

            'í' | 'ì' | 'ỉ' | 'ĩ' | 'ị' |
            'Í' | 'Ì' | 'Ỉ' | 'Ĩ' | 'Ị' => 'I',

            'ó' | 'ò' | 'ỏ' | 'õ' | 'ọ' |
            'ô' | 'ố' | 'ồ' | 'ổ' | 'ỗ' | 'ộ' |
            'ơ' | 'ớ' | 'ờ' | 'ở' | 'ỡ' | 'ợ' |
            'Ó' | 'Ò' | 'Ỏ' | 'Õ' | 'Ọ' |
            'Ô' | 'Ố' | 'Ồ' | 'Ổ' | 'Ỗ' | 'Ộ' |
            'Ơ' | 'Ớ' | 'Ờ' | 'Ở' | 'Ỡ' | 'Ợ' => 'O',

            'ú' | 'ù' | 'ủ' | 'ũ' | 'ụ' |
            'ư' | 'ứ' | 'ừ' | 'ử' | 'ữ' | 'ự' |
            'Ú' | 'Ù' | 'Ủ' | 'Ũ' | 'Ụ' |
            'Ư' | 'Ứ' | 'Ừ' | 'Ử' | 'Ữ' | 'Ự' => 'U',

            'ý' | 'ỳ' | 'ỷ' | 'ỹ' | 'ỵ' |
            'Ý' | 'Ỳ' | 'Ỷ' | 'Ỹ' | 'Ỵ' => 'Y',

            other => other.to_ascii_uppercase(),
        };
        out.push(ch);
    }
    out
}

/// Stop words appearing on the 18+ game health advisory banner
const BANNER_18_STOP_WORDS: &[&str] = &[
    "CHOI", "QUA", "180", "PHUT", "MOI", "MOT", "NGAY", "SE",
    "ANH", "HUONG", "XAU", "DEN", "SUC", "KHOE", "18+", "18",
];

/// In-game weapon category phrases stripped during text cleaning (longest first).
const CATEGORY_PHRASES: &[&str] = &[
    "COMPACT ASSAULT RIFLE",
    "GENERAL MACHINE GUN",
    "LIGHT MACHINE GUN",
    "SUBMACHINE GUN",
    "MARKSMAN RIFLE",
    "ASSAULT RIFLE",
    "ASSULT RIFLE",
    "BATTLE RIFLE",
    "SNIPER RIFLE",
    "MACHINE GUN",
    "HANDGUN",
    "REVOLVER",
    "CARBINE",
    "SHOTGUN",
    "PISTOL",
    "CROSSBOW",
    "RIFLE",
];

/// Technical weapon code normalizations allowed inside OCR recognition (NO Vietnamese nicknames).
const OCR_CODE_NORMALIZATIONS: &[(&str, &str)] = &[
    ("725 SHOTGUN", "725 DOUBLE-BARREL"),
    ("725 DOUBLE BARREL", "725 DOUBLE-BARREL"),
    ("725 DOUBLE-BARREL", "725 DOUBLE-BARREL"),
    ("DOUBLE BARREL", "725 DOUBLE-BARREL"),
    ("DOUBLE-BARREL", "725 DOUBLE-BARREL"),
    ("725", "725 DOUBLE-BARREL"),
    ("AR-57", "AR-57"),
    ("AR57", "AR-57"),
    ("PSG-1", "PSG-1"),
    ("PSG1", "PSG-1"),
    ("PEG-1", "PSG-1"),
    ("PEG1", "PSG-1"),
    ("M82A1", "M82"),
    ("M82", "M82"),
    ("882", "M82"),
    ("B82", "M82"),
    ("CL1G", "CI-19"),
    ("C1-19", "CI-19"),
    ("CL-19", "CI-19"),
    ("CI19", "CI-19"),
    ("CI-19", "CI-19"),
    ("TEZG5-1", "QBZ95-1"),
    ("QBZ951", "QBZ95-1"),
    ("QBZ-95-1", "QBZ95-1"),
    ("QBZ95-1", "QBZ95-1"),
    ("ECS52", "SG552"),
    ("SG-552", "SG552"),
    ("SG552", "SG552"),
    ("CVECTOR", "VECTOR"),
    ("VECTOR", "VECTOR"),
    ("SR-3M", "SR-3M"),
    ("SR3M", "SR-3M"),
    ("P-90", "P90"),
    ("P90", "P90"),
    ("G3", "G3"),
    ("M7", "M7"),
    ("MK4", "MK4"),
    ("R93", "R93"),
    ("COMPOUND BOW", "COMPOUND BOW"),
    ("DEAGLE", "DESERT EAGLE"),
    ("GLOCK 17", "G-17"),
    ("GLOCK17", "G-17"),
    ("GLOCK 18", "G18"),
    ("GLOCK18", "G18"),
    ("G17", "G-17"),
    ("KC17", "KC-17"),
    ("QJB201", "QJB-201"),
    ("LEVER-ACTION", "MARLIN LEVER-ACTION"),
    ("LEVER ACTION", "MARLIN LEVER-ACTION"),
    ("MARLIN", "MARLIN LEVER-ACTION"),
    ("UZL", "UZI"),
    ("357 REVOLVER", ".357"),
    ("357", ".357"),
    ("MINI 14", "MINI-14"),
    ("MINI14", "MINI-14"),
];

/// Full weapon alias mapping used ONLY by manual UI search and DB semantic cross-validation.
/// Vietnamese nicknames (`hai nong`, `cung`, ...) are intentionally excluded from OCR matching.
const WEAPON_ALIASES: &[(&str, &str)] = &[
    ("SHOTGUN HAI NONG", "725 DOUBLE-BARREL"),
    ("SUNG SAN HAI NONG", "725 DOUBLE-BARREL"),
    ("HAI NONG", "725 DOUBLE-BARREL"),
    ("725 SHOTGUN", "725 DOUBLE-BARREL"),
    ("725 DOUBLE BARREL", "725 DOUBLE-BARREL"),
    ("725 DOUBLE-BARREL", "725 DOUBLE-BARREL"),
    ("DOUBLE BARREL", "725 DOUBLE-BARREL"),
    ("DOUBLE-BARREL", "725 DOUBLE-BARREL"),
    ("725", "725 DOUBLE-BARREL"),
    ("AR-57", "AR-57"),
    ("AR57", "AR-57"),
    ("PSG-1", "PSG-1"),
    ("PSG1", "PSG-1"),
    ("PEG-1", "PSG-1"),
    ("PEG1", "PSG-1"),
    ("M82A1", "M82"),
    ("M82", "M82"),
    ("882", "M82"),
    ("B82", "M82"),
    ("CL1G", "CI-19"),
    ("C1-19", "CI-19"),
    ("CL-19", "CI-19"),
    ("CI19", "CI-19"),
    ("CI-19", "CI-19"),
    ("TEZG5-1", "QBZ95-1"),
    ("QBZ951", "QBZ95-1"),
    ("QBZ-95-1", "QBZ95-1"),
    ("QBZ95-1", "QBZ95-1"),
    ("ECS52", "SG552"),
    ("SG-552", "SG552"),
    ("SG552", "SG552"),
    ("CVECTOR", "VECTOR"),
    ("VECTOR", "VECTOR"),
    ("SR-3M", "SR-3M"),
    ("SR3M", "SR-3M"),
    ("P-90", "P90"),
    ("P90", "P90"),
    ("G3", "G3"),
    ("M7", "M7"),
    ("MK4", "MK4"),
    ("R93", "R93"),
    ("CUNG PHUC HOP", "COMPOUND BOW"),
    ("NO PHUC HOP", "COMPOUND BOW"),
    ("COMPOUND BOW", "COMPOUND BOW"),
    ("CUNG TEN", "COMPOUND BOW"),
    ("CUNG", "COMPOUND BOW"),
    ("NO", "COMPOUND BOW"),
    ("DEAGLE", "DESERT EAGLE"),
    ("GLOCK 17", "G-17"),
    ("GLOCK17", "G-17"),
    ("GLOCK 18", "G18"),
    ("GLOCK18", "G18"),
    ("GLOCK", "G-17"),
    ("SUNG CAN LEVER", "MARLIN LEVER-ACTION"),
    ("CAN LEVER", "MARLIN LEVER-ACTION"),
    ("LEVER-ACTION", "MARLIN LEVER-ACTION"),
    ("LEVER ACTION", "MARLIN LEVER-ACTION"),
    ("MARLIN", "MARLIN LEVER-ACTION"),
    ("UZL", "UZI"),
    ("357 REVOLVER", ".357"),
    ("357", ".357"),
    ("MINI 14", "MINI-14"),
    ("MINI14", "MINI-14"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OcrWeaponCategory {
    BattleRifle,
    AssaultRifle,
    SubmachineGun,
    MarksmanRifle,
    SniperRifle,
    MachineGun,
    Shotgun,
    Pistol,
}

impl OcrWeaponCategory {
    fn contains_weapon(self, weapon_upper: &str) -> bool {
        match self {
            Self::BattleRifle => matches!(weapon_upper, "M7" | "G3" | "SCAR-H" | "ASH-12"),
            Self::AssaultRifle => matches!(
                weapon_upper,
                "AK-12"
                    | "AKM"
                    | "AKS-74"
                    | "AN-94"
                    | "AR-57"
                    | "AS VAL"
                    | "ASH-12"
                    | "AUG"
                    | "CAR-15"
                    | "CI-19"
                    | "G3"
                    | "K416"
                    | "K437"
                    | "KC-17"
                    | "M16A4"
                    | "M4A1"
                    | "M7"
                    | "MCX LT"
                    | "MDR"
                    | "MK47"
                    | "PTR-32"
                    | "QBZ95-1"
                    | "RM277"
                    | "SCAR-H"
                    | "SG552"
                    | "SR-3M"
            ),
            Self::SubmachineGun => matches!(
                weapon_upper,
                "AKS-74U"
                    | "AR-57"
                    | "BIZON"
                    | "MK4"
                    | "MP5"
                    | "MP7"
                    | "P90"
                    | "QCQ171"
                    | "SMG-45"
                    | "SR9"
                    | "THOMPSON"
                    | "UZI"
                    | "VECTOR"
                    | "VITYAZ"
            ),
            Self::MarksmanRifle => matches!(
                weapon_upper,
                "M14" | "MINI-14" | "PSG-1" | "SKS" | "SR-25" | "SR9" | "SVCH" | "VSS"
            ),
            Self::SniperRifle => {
                matches!(weapon_upper, "AWM" | "M700" | "M82" | "R93" | "SV-98" | "SVD")
            }
            Self::MachineGun => matches!(weapon_upper, "M249" | "M250" | "PKM" | "QJB-201"),
            Self::Shotgun => matches!(
                weapon_upper,
                "725 DOUBLE-BARREL" | "FS-12" | "M1014" | "M870" | "S12K"
            ),
            Self::Pistol => matches!(
                weapon_upper,
                ".357" | "93R" | "DESERT EAGLE" | "G-17" | "G18" | "M1911" | "QSZ-92G"
            ),
        }
    }
}

/// Ordered longest-first so multi-word English categories match before short ones.
const OCR_CATEGORY_RULES: &[(&str, OcrWeaponCategory)] = &[
    ("COMPACT ASSAULT RIFLE", OcrWeaponCategory::AssaultRifle),
    ("GENERAL MACHINE GUN", OcrWeaponCategory::MachineGun),
    ("LIGHT MACHINE GUN", OcrWeaponCategory::MachineGun),
    ("SUBMACHINE GUN", OcrWeaponCategory::SubmachineGun),
    ("MARKSMAN RIFLE", OcrWeaponCategory::MarksmanRifle),
    ("ASSAULT RIFLE", OcrWeaponCategory::AssaultRifle),
    ("ASSULT RIFLE", OcrWeaponCategory::AssaultRifle),
    ("BATTLE RIFLE", OcrWeaponCategory::BattleRifle),
    ("SNIPER RIFLE", OcrWeaponCategory::SniperRifle),
    ("MACHINE GUN", OcrWeaponCategory::MachineGun),
    ("HANDGUN", OcrWeaponCategory::Pistol),
    ("REVOLVER", OcrWeaponCategory::Pistol),
    ("CARBINE", OcrWeaponCategory::AssaultRifle),
    ("SHOTGUN", OcrWeaponCategory::Shotgun),
    ("PISTOL", OcrWeaponCategory::Pistol),
    ("LMG", OcrWeaponCategory::MachineGun),
    ("SMG", OcrWeaponCategory::SubmachineGun),
    ("DMR", OcrWeaponCategory::MarksmanRifle),
    ("BR", OcrWeaponCategory::BattleRifle),
    ("SR", OcrWeaponCategory::SniperRifle),
    ("AR", OcrWeaponCategory::AssaultRifle),
];

/// Strips skin suffixes separated by hyphens/dashes (e.g. " - ", " – ", " — ").
fn strip_skin_suffix(s: &str) -> String {
    let mut earliest_pos: Option<usize> = None;
    for sep in &[" - ", " – ", " — ", " –", "– ", " —", "— ", "–", "—", " -"] {
        if let Some(pos) = s.find(sep) {
            earliest_pos = Some(match earliest_pos {
                Some(cur) => cur.min(pos),
                None => pos,
            });
        }
    }
    if let Some(pos) = earliest_pos {
        return s[..pos].trim().to_string();
    }

    // Handle "- " when OCR dropped the leading space after a category/word
    for (idx, _) in s.match_indices("- ") {
        let prefix = s[..idx].trim();
        let suffix = s[idx + 2..].trim();
        if prefix.contains(' ')
            && !prefix.ends_with("Double")
            && !prefix.ends_with("Lever")
            && !suffix.is_empty()
            && !suffix.starts_with(|c: char| c.is_ascii_digit())
            && suffix.chars().count() >= 2
        {
            return prefix.to_string();
        }
    }

    s.to_string()
}

fn resolve_table_alias(text: &str, table: &[(&'static str, &'static str)]) -> Option<&'static str> {
    let trimmed = text.trim();
    for &(alias, canonical) in table {
        if trimmed == alias {
            return Some(canonical);
        }
        let prefix = format!("{} ", alias);
        if trimmed.starts_with(&prefix) {
            return Some(canonical);
        }
        let suffix = format!(" {}", alias);
        if trimmed.ends_with(&suffix) {
            return Some(canonical);
        }
    }
    None
}

fn resolve_alias(text: &str) -> Option<&'static str> {
    resolve_table_alias(text, WEAPON_ALIASES)
}

fn resolve_ocr_code_normalization(text: &str) -> Option<&'static str> {
    resolve_table_alias(text, OCR_CODE_NORMALIZATIONS)
}

/// Scores how well a weapon name matches a user search query.
/// Higher score means better match:
/// - Exact match (ignoring case, diacritics, dashes): 100
/// - Exact alias match via WEAPON_ALIASES: 90
/// - Starts-with query: 80
/// - Alias starts-with query: 75
/// - Word boundary starts-with (e.g. "BOW" in "COMPOUND BOW"): 70
/// - Alias word starts-with query: 65
/// - Contains query: 60
/// - Alias contains query: 55
/// - Query contains alias: 50
fn score_weapon_search(weapon_name: &str, query: &str) -> Option<i32> {
    let q_clean = strip_vietnamese_diacritics(query).trim().to_uppercase();
    if q_clean.is_empty() {
        return Some(0);
    }

    let q_no_dash = q_clean.replace(['-', '_'], " ");
    let q_compact = q_clean.replace(['-', '_', ' '], "");

    let w_upper = weapon_name.to_uppercase();
    let w_clean = strip_vietnamese_diacritics(&w_upper);
    let w_no_dash = w_clean.replace(['-', '_'], " ");
    let w_compact = w_clean.replace(['-', '_', ' '], "");

    // 1. Direct exact match (score 100)
    if w_clean == q_clean || w_compact == q_compact || w_no_dash == q_no_dash {
        return Some(100);
    }

    // 2. Direct starts-with (score 80)
    if w_clean.starts_with(&q_clean) || w_compact.starts_with(&q_compact) {
        return Some(80);
    }

    // 3. Word starts-with (e.g. "BOW" in "COMPOUND BOW", "EAGLE" in "DESERT EAGLE") (score 70)
    if w_no_dash.split_whitespace().any(|word| word.starts_with(&q_clean) || (!q_compact.is_empty() && word.starts_with(&q_compact))) {
        return Some(70);
    }

    // 4. Direct contains (score 60)
    if w_clean.contains(&q_clean) || w_no_dash.contains(&q_no_dash) || (!q_compact.is_empty() && w_compact.contains(&q_compact)) {
        return Some(60);
    }

    // 5. Match via WEAPON_ALIASES (Vietnamese aliases, abbreviations, nicknames)
    for &(alias, canonical) in WEAPON_ALIASES {
        let canonical_upper = canonical.to_uppercase();
        let canonical_clean = strip_vietnamese_diacritics(&canonical_upper);
        let canonical_compact = canonical_clean.replace(['-', '_', ' '], "");

        let is_this_weapon = canonical_upper == w_upper
            || canonical_clean == w_clean
            || canonical_compact == w_compact;

        if is_this_weapon {
            let alias_clean = strip_vietnamese_diacritics(alias).trim().to_uppercase();
            let alias_no_dash = alias_clean.replace(['-', '_'], " ");
            let alias_compact = alias_clean.replace(['-', '_', ' '], "");

            // Exact alias match (score 90)
            if alias_clean == q_clean || alias_compact == q_compact || alias_no_dash == q_no_dash {
                return Some(90);
            }
            // Alias starts-with query (score 75)
            if alias_clean.starts_with(&q_clean) || alias_compact.starts_with(&q_compact) {
                return Some(75);
            }
            // Alias word starts-with query (score 65)
            if alias_no_dash.split_whitespace().any(|word| word.starts_with(&q_clean)) {
                return Some(65);
            }
            // Alias contains query (score 55)
            if alias_clean.contains(&q_clean) || (!q_compact.is_empty() && alias_compact.contains(&q_compact)) {
                return Some(55);
            }
            // Query contains alias (score 50)
            if q_clean.contains(&alias_clean) {
                return Some(50);
            }
        }
    }

    None
}

/// Checks whether a weapon matches the given search query.
fn matches_weapon_search(weapon_name: &str, query: &str) -> bool {
    score_weapon_search(weapon_name, query).is_some()
}

/// Finds the single best matching weapon from a list for the given query.
fn find_best_matching_weapon<'a>(weapons: &'a [String], query: &str) -> Option<&'a String> {
    if query.trim().is_empty() {
        return None;
    }
    let mut best: Option<(&'a String, i32)> = None;
    for w in weapons {
        if let Some(score) = score_weapon_search(w, query) {
            match best {
                None => best = Some((w, score)),
                Some((_, best_score)) if score > best_score => {
                    best = Some((w, score));
                }
                _ => {}
            }
        }
    }
    best.map(|(w, _)| w)
}

/// Determines default weapon on startup: prioritizes "M4A1" if available,
/// or the first weapon with at least 1 build matching the default server & mode.
fn determine_startup_weapon(
    unique_weapons: &[String],
    builds: &[BuildItem],
    server: ServerFilter,
    mode: ModeFilter,
) -> Option<String> {
    // 1. Prioritize M4A1 if it has builds matching the server & mode
    if unique_weapons.iter().any(|w| w == "M4A1") {
        let has_server_mode = builds.iter().any(|b| {
            b.weapon == "M4A1" && server.matches(&b.server) && mode.matches(&b.mode)
        });
        if has_server_mode {
            return Some("M4A1".to_string());
        }
        let has_server = builds.iter().any(|b| b.weapon == "M4A1" && server.matches(&b.server));
        if has_server {
            return Some("M4A1".to_string());
        }
    }

    // 2. Otherwise find the first weapon in unique_weapons that has at least 1 build matching server & mode
    if let Some(w) = unique_weapons.iter().find(|w| {
        builds
            .iter()
            .any(|b| &b.weapon == *w && server.matches(&b.server) && mode.matches(&b.mode))
    }) {
        return Some(w.clone());
    }

    // 3. Fallback: first weapon matching server
    if let Some(w) = unique_weapons.iter().find(|w| {
        builds
            .iter()
            .any(|b| &b.weapon == *w && server.matches(&b.server))
    }) {
        return Some(w.clone());
    }

    // 4. Ultimate fallback: first unique weapon
    unique_weapons.first().cloned()
}

fn strip_category_phrases(mut text: String) -> String {
    for &phrase in CATEGORY_PHRASES {
        if text == phrase {
            return String::new();
        }
        let prefix = format!("{} ", phrase);
        if text.starts_with(&prefix) {
            text = text[prefix.len()..].trim().to_string();
        }
        let suffix = format!(" {}", phrase);
        if text.ends_with(&suffix) {
            text = text[..text.len() - suffix.len()].trim().to_string();
        }
        let middle = format!(" {} ", phrase);
        if text.contains(&middle) {
            text = text.replace(&middle, " ").trim().to_string();
        }
    }

    // Strip trailing standalone abbreviations (BR, LMG, SMG, AR, DMR, SR, SG)
    // Only if there are at least 2 tokens (e.g. "M249 LMG" -> "M249", "G3 BR" -> "G3")
    let mut tokens: Vec<&str> = text.split_whitespace().collect();
    if tokens.len() > 1 {
        if let Some(&last) = tokens.last() {
            if matches!(last, "BR" | "LMG" | "SMG" | "AR" | "DMR" | "SR" | "SG") {
                tokens.pop();
                text = tokens.join(" ");
            }
        }
    }

    if CATEGORY_PHRASES.contains(&text.as_str()) {
        return String::new();
    }

    text
}

/// Sanitizes leading/trailing 18+ occlusion artifacts (e.g., `...7` -> `7`, while preserving `.357`).
fn sanitize_ocr_code_token(raw_code: &str) -> String {
    let mut s = raw_code.trim();
    loop {
        let prev = s;
        s = s.trim_start_matches(|c: char| matches!(c, ' ' | '+' | '-' | '_' | ':' | ';' | ',' | '/' | '|'));
        if let Some(after_18) = s.strip_prefix("18+") {
            s = after_18.trim_start();
        } else if s.starts_with("18 ") {
            s = s[3..].trim_start();
        }
        if s.starts_with("..") {
            s = s.trim_start_matches('.');
        } else if let Some(rest) = s.strip_prefix('.') {
            // Preserve legitimate `.357` caliber prefix, but drop stray leading dot from occluded text like `.7` or `.K4`
            if !rest.starts_with("357") {
                s = rest;
            }
        }
        if s == prev {
            break;
        }
    }
    s = s.trim_end_matches(|c: char| matches!(c, ' ' | '+' | '-' | '_' | '.'));
    if !s.chars().any(|c| c.is_ascii_alphanumeric()) {
        return String::new();
    }
    s.to_string()
}

#[derive(Debug, Clone)]
struct ParsedWeaponHeader {
    code: String,
    category: Option<OcrWeaponCategory>,
    special_weapon: Option<&'static str>,
}

/// Checks unique OCR title exceptions outside generic Vietnamese aliases:
/// 1. `Cung Phức Hợp` (`Jng Phurc Hop`, `Cung Phuc Hop`, ...) -> `COMPOUND BOW`
/// 2. Truncated `Lever-action Rifle` (`aver-action Rifle`, ...) -> `MARLIN LEVER-ACTION`
/// 3. Vietnamese `Shotgun Hai Nòng` (`SHOTGUN HAI NONG`, `HAI NONG U`, ...) -> `725 DOUBLE-BARREL`
fn detect_special_title_override(text_upper: &str) -> Option<&'static str> {
    let norm = text_upper.replace(['_', '.'], " ");
    for pat in [
        "CUNG PHUC",
        "PHUC HOP",
        "PHURC HOP",
        "PHUC H0P",
        "PHURC H0P",
        "JNG PHURC",
        "JNG PHUC",
    ] {
        if norm.contains(pat) {
            return Some("COMPOUND BOW");
        }
    }

    for pat in [
        "LEVER-ACTION",
        "LEVER ACTION",
        "AVER-ACTION",
        "AVER ACTION",
        "VER-ACTION",
        "VER ACTION",
    ] {
        if norm.contains(pat) {
            return Some("MARLIN LEVER-ACTION");
        }
    }

    for pat in [
        "SHOTGUN HAI NONG",
        "HAI NONG",
        "HAI N0NG",
        "HAINONG",
    ] {
        if norm.contains(pat) {
            return Some("725 DOUBLE-BARREL");
        }
    }

    None
}

/// Normalizes a raw OCR center-header string and extracts `(code, detected_category, special_weapon)`.
/// Supports glued category phrases (`4A1Assault Rifle`), sticky trailing noise (`S552 Assault Rifleof oadu`),
/// and 1-2 char OCR typos in category names (`Sniger Rifle`, `Assauit Rifle`, `Submachlne Gun`, `Marksmam Rifle`).
fn parse_weapon_header(raw: &str) -> ParsedWeaponHeader {
    let normalized_unicode = raw.replace('…', "...");
    let raw_skin_stripped = strip_skin_suffix(&normalized_unicode);

    // Find the first non-empty line after trimming banner stop words
    let mut best_line = String::new();
    for line in raw_skin_stripped.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let mut raw_tokens: Vec<&str> = line.split_whitespace().collect();
        while !raw_tokens.is_empty() {
            let t = raw_tokens[0];
            let t_upper = strip_vietnamese_diacritics(t);
            if BANNER_18_STOP_WORDS.contains(&t_upper.as_str()) {
                raw_tokens.remove(0);
            } else if t.chars().all(|c| c.is_lowercase())
                && t.len() <= 2
                && raw_tokens.len() > 1
                && raw_tokens[1]
                    .chars()
                    .next()
                    .map_or(false, |c| c.is_uppercase() || c.is_ascii_digit())
            {
                raw_tokens.remove(0);
            } else {
                break;
            }
        }

        let clean_cand = raw_tokens.join(" ");
        if !clean_cand.trim().is_empty() {
            best_line = clean_cand;
            break;
        }
    }

    if best_line.is_empty() {
        best_line = raw_skin_stripped;
    }

    let upper = strip_vietnamese_diacritics(&best_line);
    let filtered: String = upper
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                ' '
            }
        })
        .collect();

    let tokens: Vec<&str> = filtered
        .split_whitespace()
        .filter(|t| !BANNER_18_STOP_WORDS.contains(t))
        .collect();
    let mut text = tokens.join(" ");

    if let Some(special) = detect_special_title_override(&text) {
        return ParsedWeaponHeader {
            code: special.to_string(),
            category: None,
            special_weapon: Some(special),
        };
    }

    let mut detected_cat: Option<OcrWeaponCategory> = None;

    // 1. Direct substring match for long multi-word category phrases (supports glued prefix & sticky trailing noise)
    for &(phrase, cat) in OCR_CATEGORY_RULES {
        if phrase.len() >= 10 {
            if let Some(pos) = text.find(phrase) {
                detected_cat = Some(cat);
                text = text[..pos].trim().to_string();
                break;
            }
        }
    }

    // 2. Fuzzy 3-word / 2-word category matching (handles "Sniger Rifle", "Sniner Rifle", "Assauit Rifle", "Submachlne Gun", "Marksmam Rifle")
    if detected_cat.is_none() {
        let words: Vec<&str> = text.split_whitespace().collect();
        'outer_fuzzy: for win_len in [3usize, 2usize] {
            if words.len() < win_len {
                continue;
            }
            for i in 0..=words.len() - win_len {
                if words[i].len() < 4 {
                    continue;
                }
                let mut win_parts: Vec<String> = words[i..i + win_len]
                    .iter()
                    .map(|w| (*w).to_string())
                    .collect();
                if let Some(last) = win_parts.last_mut() {
                    if last.starts_with("RIFLE") && last.len() > 5 {
                        *last = "RIFLE".to_string();
                    } else if last.starts_with("GUN") && last.len() > 3 {
                        *last = "GUN".to_string();
                    }
                }
                let candidate_phrase = win_parts.join(" ");
                for &(phrase, cat) in OCR_CATEGORY_RULES {
                    let phrase_words = phrase.split_whitespace().count();
                    if phrase_words != win_len {
                        continue;
                    }
                    let max_dist = if phrase.len() >= 14 { 2 } else { 1 };
                    if strsim::levenshtein(&candidate_phrase, phrase) <= max_dist {
                        detected_cat = Some(cat);
                        text = words[..i].join(" ");
                        break 'outer_fuzzy;
                    }
                }
            }
        }
    }

    // 3. Single-word category & trailing abbreviation match
    if detected_cat.is_none() {
        for &(phrase, cat) in OCR_CATEGORY_RULES {
            if phrase.split_whitespace().count() > 1 {
                continue;
            }
            let is_short_abbr = phrase.len() <= 3;
            if !is_short_abbr {
                if text.starts_with(phrase) {
                    detected_cat = Some(cat);
                    text = text[phrase.len()..].trim().to_string();
                    break;
                }
                let needle = format!(" {}", phrase);
                if let Some(pos) = text.find(&needle) {
                    detected_cat = Some(cat);
                    text = text[..pos].trim().to_string();
                    break;
                }
            } else {
                let suffix = format!(" {}", phrase);
                if text.ends_with(&suffix) {
                    detected_cat = Some(cat);
                    text = text[..text.len() - suffix.len()].trim().to_string();
                    break;
                }
                let middle = format!(" {} ", phrase);
                if let Some(pos) = text.find(&middle) {
                    detected_cat = Some(cat);
                    text = text[..pos].trim().to_string();
                    break;
                }
            }
        }
    }

    // Strip any generic non-specific trailing/only words (e.g. "RIFLE", "CROSSBOW")
    if detected_cat.is_none() {
        text = strip_category_phrases(text);
    }

    let code_part = sanitize_ocr_code_token(&text);
    if CATEGORY_PHRASES.contains(&code_part.as_str()) {
        return ParsedWeaponHeader {
            code: String::new(),
            category: detected_cat,
            special_weapon: None,
        };
    }

    ParsedWeaponHeader {
        code: code_part,
        category: detected_cat,
        special_weapon: None,
    }
}

fn clean_ocr_text(raw: &str) -> String {
    let parsed = parse_weapon_header(raw);
    if parsed.code.is_empty() {
        return String::new();
    }
    if let Some(canonical) = resolve_ocr_code_normalization(&parsed.code) {
        return canonical.to_string();
    }
    parsed.code
}

/// Normalizes character pairs commonly misidentified by Windows OCR on game fonts:
/// '0' <-> 'O', '1' / 'L' <-> 'I', '5' <-> 'S', '8' <-> 'B'
fn ocr_normalize_homoglyphs(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '0' => 'O',
            '1' | 'L' => 'I',
            '5' => 'S',
            '8' => 'B',
            other => other,
        })
        .collect()
}

/// Strips all whitespace, hyphens, underscores and dots to avoid separator mismatch
fn ocr_strip_separators(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace() && *c != '-' && *c != '_' && *c != '.')
        .collect()
}

/// Compares two semver strings (e.g. "v1.3.3" vs "v1.3.2")
#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedSemVer {
    major: u32,
    minor: u32,
    patch: u32,
    prerelease: Option<String>,
}

/// Parses a SemVer string with optional leading `v`/`V`, pre-release suffix (`-beta`), and `+build` metadata.
fn parse_semver(raw: &str) -> Option<ParsedSemVer> {
    let stripped = raw.trim().trim_start_matches(|c: char| c == 'v' || c == 'V');
    let without_build = stripped.split('+').next().unwrap_or(stripped).trim();
    if without_build.is_empty() {
        return None;
    }

    let (core, prerelease) = match without_build.split_once('-') {
        Some((c, pre)) => {
            let pre_trimmed = pre.trim();
            (c.trim(), if pre_trimmed.is_empty() { None } else { Some(pre_trimmed.to_string()) })
        }
        None => (without_build, None),
    };

    let mut nums = core.split('.').map(|s| s.trim().parse::<u32>().ok());
    let major = nums.next()??;
    let minor = nums.next()??;
    let patch = nums.next()??;
    if nums.next().is_some() {
        return None;
    }

    Some(ParsedSemVer {
        major,
        minor,
        patch,
        prerelease,
    })
}

fn compare_prerelease(a: &str, b: &str) -> std::cmp::Ordering {
    let a_parts: Vec<&str> = a.split('.').collect();
    let b_parts: Vec<&str> = b.split('.').collect();
    for (pa, pb) in a_parts.iter().zip(b_parts.iter()) {
        let ord = match (pa.parse::<u64>(), pb.parse::<u64>()) {
            (Ok(na), Ok(nb)) => na.cmp(&nb),
            (Ok(_), Err(_)) => std::cmp::Ordering::Less,
            (Err(_), Ok(_)) => std::cmp::Ordering::Greater,
            (Err(_), Err(_)) => pa.to_ascii_lowercase().cmp(&pb.to_ascii_lowercase()),
        };
        if ord != std::cmp::Ordering::Equal {
            return ord;
        }
    }
    a_parts.len().cmp(&b_parts.len())
}

fn compare_semver(a: &ParsedSemVer, b: &ParsedSemVer) -> std::cmp::Ordering {
    (a.major, a.minor, a.patch)
        .cmp(&(b.major, b.minor, b.patch))
        .then_with(|| match (&a.prerelease, &b.prerelease) {
            (None, None) => std::cmp::Ordering::Equal,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (Some(_), None) => std::cmp::Ordering::Less,
            (Some(pa), Some(pb)) => compare_prerelease(pa, pb),
        })
}

/// Returns `true` when `remote` is strictly newer than `current` under SemVer + pre-release precedence.
/// Stable builds ignore remote pre-releases; pre-release builds accept both newer pre-releases and stable releases.
fn is_newer_version(remote: &str, current: &str) -> bool {
    let Some(r_ver) = parse_semver(remote) else {
        return false;
    };
    let Some(c_ver) = parse_semver(current) else {
        return false;
    };
    if c_ver.prerelease.is_none() && r_ver.prerelease.is_some() {
        return false;
    }
    compare_semver(&r_ver, &c_ver) == std::cmp::Ordering::Greater
}

fn extract_release_summary(body: &str) -> String {
    for raw_line in body.lines() {
        let line = raw_line.trim();
        if line.is_empty()
            || line.starts_with('#')
            || line.starts_with("---")
            || line.starts_with('|')
            || line.starts_with('*') && line.ends_with('*') && line.len() > 20
        {
            continue;
        }
        let cleaned = line
            .trim_start_matches(|c: char| c == '-' || c == '*' || c == ' ')
            .replace("**", "")
            .replace('`', "");
        let cleaned = cleaned.trim();
        if !cleaned.is_empty() {
            return truncate_ocr_preview(cleaned, 120);
        }
    }
    String::new()
}

/// Selects the highest eligible update from GitHub `/repos/.../releases` JSON payload.
fn select_latest_update_from_releases(
    releases_json: &serde_json::Value,
    current_version: &str,
) -> Option<ReleaseUpdateInfo> {
    let current_ver = parse_semver(current_version)?;
    let arr = releases_json.as_array()?;

    let mut best: Option<(ParsedSemVer, ReleaseUpdateInfo)> = None;
    for item in arr {
        if item["draft"].as_bool().unwrap_or(false) {
            continue;
        }
        let Some(tag_name) = item["tag_name"].as_str().map(|s| s.trim()).filter(|s| !s.is_empty()) else {
            continue;
        };
        let Some(parsed_tag) = parse_semver(tag_name) else {
            continue;
        };

        let is_prerelease = item["prerelease"].as_bool().unwrap_or(false) || parsed_tag.prerelease.is_some();
        if current_ver.prerelease.is_none() && is_prerelease {
            continue;
        }
        if !is_newer_version(tag_name, current_version) {
            continue;
        }

        let name = item["name"]
            .as_str()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| tag_name.to_string());
        let summary = item["body"]
            .as_str()
            .map(extract_release_summary)
            .unwrap_or_default();
        let html_url = item["html_url"]
            .as_str()
            .map(|s| s.trim().to_string())
            .filter(|s| s.starts_with("https://"))
            .unwrap_or_else(|| "https://github.com/TaiGiap/df-loadout-scanner/releases/latest".to_string());

        let mut zip_download_url: Option<String> = None;
        let mut zip_file_name: Option<String> = None;
        if let Some(assets) = item["assets"].as_array() {
            for asset in assets {
                let asset_name = asset["name"].as_str().unwrap_or("").trim();
                let dl_url = asset["browser_download_url"].as_str().unwrap_or("").trim();
                if asset_name.to_ascii_lowercase().ends_with(".zip") && dl_url.starts_with("https://") {
                    zip_download_url = Some(dl_url.to_string());
                    zip_file_name = Some(asset_name.to_string());
                    break;
                }
            }
        }

        let candidate = ReleaseUpdateInfo {
            tag_name: tag_name.to_string(),
            name,
            summary,
            html_url,
            zip_download_url,
            zip_file_name,
            is_prerelease,
        };

        let replace = match &best {
            None => true,
            Some((best_ver, _)) => compare_semver(&parsed_tag, best_ver) == std::cmp::Ordering::Greater,
        };
        if replace {
            best = Some((parsed_tag, candidate));
        }
    }

    best.map(|(_, info)| info)
}

fn fetch_latest_release_update(current_version: &str) -> Result<Option<ReleaseUpdateInfo>, String> {
    let url = "https://api.github.com/repos/TaiGiap/df-loadout-scanner/releases";
    match ureq::get(url)
        .set("User-Agent", "DF-Loadout-Scanner")
        .set("Accept", "application/vnd.github+json")
        .timeout(Duration::from_secs(9))
        .call()
    {
        Ok(resp) => {
            let json = resp
                .into_json::<serde_json::Value>()
                .map_err(|e| format!("Lỗi phân tích phản hồi GitHub API: {}", e))?;
            Ok(select_latest_update_from_releases(&json, current_version))
        }
        Err(ureq::Error::Status(403 | 429, _)) => {
            Err("GitHub API đang giới hạn tần suất (Rate Limit). Vui lòng thử lại sau ít phút.".to_string())
        }
        Err(ureq::Error::Status(code, _)) => {
            Err(format!("GitHub API trả về mã lỗi HTTP {}", code))
        }
        Err(ureq::Error::Transport(t)) => {
            Err(format!("Lỗi kết nối / quá thời gian chờ GitHub API: {}", t))
        }
    }
}

fn spawn_update_check(tx: Sender<WorkerEvent>, ctx: egui::Context, manual: bool) {
    thread::spawn(move || {
        let result = fetch_latest_release_update(APP_VERSION);
        let _ = tx.send(WorkerEvent::UpdateCheckFinished { manual, result });
        ctx.request_repaint();
    });
}

fn inspect_zip_entries(bytes: &[u8]) -> Result<Vec<String>, String> {
    if bytes.len() < 22 || !bytes.starts_with(&[0x50, 0x4B, 0x03, 0x04]) {
        return Err("Tệp tải về không phải định dạng .zip hợp lệ (sai chữ ký PK)".to_string());
    }

    let min_eocd_search = bytes.len().saturating_sub(65558);
    let mut eocd_pos: Option<usize> = None;
    let mut i = bytes.len() - 22;
    loop {
        if bytes[i..i + 4] == [0x50, 0x4B, 0x05, 0x06] {
            eocd_pos = Some(i);
            break;
        }
        if i <= min_eocd_search {
            break;
        }
        i -= 1;
    }

    let eocd = eocd_pos.ok_or_else(|| "Tệp .zip bị lỗi cấu trúc (thiếu End of Central Directory)".to_string())?;
    let total_entries = u16::from_le_bytes([bytes[eocd + 10], bytes[eocd + 11]]) as usize;
    let cd_size = u32::from_le_bytes([bytes[eocd + 12], bytes[eocd + 13], bytes[eocd + 14], bytes[eocd + 15]]) as usize;
    let cd_offset = u32::from_le_bytes([bytes[eocd + 16], bytes[eocd + 17], bytes[eocd + 18], bytes[eocd + 19]]) as usize;

    if cd_offset.checked_add(cd_size).map_or(true, |end| end > eocd) {
        return Err("Tệp .zip bị hỏng (Central Directory vượt quá kích thước tệp)".to_string());
    }

    let mut entries = Vec::with_capacity(total_entries);
    let mut cursor = cd_offset;
    for _ in 0..total_entries {
        if cursor + 46 > eocd || bytes[cursor..cursor + 4] != [0x50, 0x4B, 0x01, 0x02] {
            return Err("Tệp .zip bị hỏng trong danh sách Central Directory".to_string());
        }
        let name_len = u16::from_le_bytes([bytes[cursor + 28], bytes[cursor + 29]]) as usize;
        let extra_len = u16::from_le_bytes([bytes[cursor + 30], bytes[cursor + 31]]) as usize;
        let comment_len = u16::from_le_bytes([bytes[cursor + 32], bytes[cursor + 33]]) as usize;
        let name_start = cursor + 46;
        let name_end = name_start + name_len;
        let next_cursor = name_end + extra_len + comment_len;
        if next_cursor > eocd {
            return Err("Tệp .zip bị cắt cụt (entry vượt quá Central Directory)".to_string());
        }
        let entry_name = String::from_utf8_lossy(&bytes[name_start..name_end]).to_string();
        entries.push(entry_name);
        cursor = next_cursor;
    }

    Ok(entries)
}

fn validate_update_zip_archive(zip_path: &Path) -> Result<Vec<String>, String> {
    let meta = fs::metadata(zip_path).map_err(|e| format!("Không đọc được tệp .zip: {}", e))?;
    if meta.len() == 0 {
        return Err("Tệp .zip rỗng (0 bytes)".to_string());
    }
    let bytes = fs::read(zip_path).map_err(|e| format!("Không mở được tệp .zip: {}", e))?;
    let entries = inspect_zip_entries(&bytes)?;
    let has_exe = entries
        .iter()
        .any(|e| !e.ends_with('/') && e.to_ascii_lowercase().ends_with(".exe"));
    if !has_exe {
        return Err("Gói .zip không chứa tệp thực thi .exe hợp lệ".to_string());
    }
    Ok(entries)
}

fn updates_dir_path() -> PathBuf {
    app_data_dir().join("updates")
}

fn just_updated_flag_path() -> PathBuf {
    updates_dir_path().join("just_updated_to.txt")
}

#[cfg(test)]
fn write_just_updated_flag_at(path: &Path, version: &str) -> Result<(), String> {
    atomic_write_verified(path, version.trim())
}

fn consume_just_updated_flag_at(path: &Path) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    let _ = fs::remove_file(path);
    let trimmed = content.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

const APPLY_UPDATE_PS1: &str = r#"param(
    [int]$AppPid,
    [string]$ExeDir,
    [string]$CurrentExe,
    [string]$ZipPath,
    [string]$DataDir,
    [string]$TargetVersion,
    [string]$LogPath,
    [string]$FlagPath
)

$ErrorActionPreference = 'Stop'

function Write-UpdateLog([string]$Message) {
    $ts = Get-Date -Format 'yyyy-MM-dd HH:mm:ss'
    $line = "[$ts] $Message`r`n"
    $parent = Split-Path -Parent $LogPath
    if ($parent -and -not (Test-Path -LiteralPath $parent)) {
        New-Item -ItemType Directory -Force -Path $parent | Out-Null
    }
    [System.IO.File]::AppendAllText($LogPath, $line, [System.Text.UTF8Encoding]::new($false))
}

function Copy-WithRetry([string]$Source, [string]$Destination, [bool]$Recurse = $false) {
    $maxRetries = 6
    for ($attempt = 1; $attempt -le $maxRetries; $attempt++) {
        try {
            if ($Recurse) {
                Copy-Item -LiteralPath $Source -Destination $Destination -Recurse -Force
            } else {
                Copy-Item -LiteralPath $Source -Destination $Destination -Force
            }
            return
        } catch {
            if ($attempt -eq $maxRetries) {
                throw
            }
            Start-Sleep -Milliseconds 400
        }
    }
}

$StageDir = Join-Path (Split-Path -Parent $LogPath) ("stage_" + $AppPid)

try {
    Write-UpdateLog "Bat dau cap nhat len $TargetVersion (PID=$AppPid)"

    for ($i = 0; $i -lt 60; $i++) {
        $proc = Get-Process -Id $AppPid -ErrorAction SilentlyContinue
        if (-not $proc) {
            break
        }
        Start-Sleep -Milliseconds 500
    }
    Start-Sleep -Milliseconds 400

    if (Test-Path -LiteralPath $StageDir) {
        Remove-Item -LiteralPath $StageDir -Recurse -Force -ErrorAction SilentlyContinue
    }
    New-Item -ItemType Directory -Force -Path $StageDir | Out-Null

    Write-UpdateLog "Giai nen $ZipPath vao $StageDir"
    Expand-Archive -LiteralPath $ZipPath -DestinationPath $StageDir -Force

    $FoundExe = Get-ChildItem -LiteralPath $StageDir -Filter '*.exe' -File -Recurse | Select-Object -First 1
    if (-not $FoundExe) {
        throw "Khong tim thay tep thuc thi .exe trong goi cap nhat"
    }
    $SourceRoot = $FoundExe.DirectoryName
    Write-UpdateLog "Da xac nhan file .exe moi: $($FoundExe.FullName)"

    $ProtectedNames = @(
        'config.json',
        'favorites.json',
        'builds_local.json',
        'df_history.json',
        'DF-Loadout-Scanner-Data'
    )

    Copy-WithRetry -Source $FoundExe.FullName -Destination $CurrentExe -Recurse $false

    $Items = Get-ChildItem -LiteralPath $SourceRoot -Force
    foreach ($item in $Items) {
        if ($ProtectedNames -contains $item.Name) {
            continue
        }
        if ($item.FullName -eq $FoundExe.FullName) {
            continue
        }
        $destPath = Join-Path $ExeDir $item.Name
        if ($item.PSIsContainer) {
            Copy-WithRetry -Source $item.FullName -Destination $ExeDir -Recurse $true
        } else {
            Copy-WithRetry -Source $item.FullName -Destination $destPath -Recurse $false
        }
    }

    $flagParent = Split-Path -Parent $FlagPath
    if ($flagParent -and -not (Test-Path -LiteralPath $flagParent)) {
        New-Item -ItemType Directory -Force -Path $flagParent | Out-Null
    }
    [System.IO.File]::WriteAllText($FlagPath, $TargetVersion, [System.Text.UTF8Encoding]::new($false))

    Remove-Item -LiteralPath $StageDir -Recurse -Force -ErrorAction SilentlyContinue
    Write-UpdateLog "Cap nhat thanh cong. Khoi chay lai $CurrentExe"
    Start-Process -FilePath $CurrentExe -WorkingDirectory $ExeDir
} catch {
    Write-UpdateLog "LOI CAP NHAT: $($_.Exception.Message)"
    if (Test-Path -LiteralPath $StageDir) {
        Remove-Item -LiteralPath $StageDir -Recurse -Force -ErrorAction SilentlyContinue
    }
    exit 1
}
"#;

fn launch_update_installer_script(zip_path: &Path, target_version: &str) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let current_exe = std::env::current_exe()
        .map_err(|e| format!("Không xác định được đường dẫn .exe hiện tại: {}", e))?;
    let exe_dir = current_exe
        .parent()
        .ok_or_else(|| "Không xác định được thư mục cài đặt hiện tại".to_string())?;
    let data_dir = app_data_dir();
    let updates_dir = updates_dir_path();
    fs::create_dir_all(&updates_dir)
        .map_err(|e| format!("Không tạo được thư mục updates: {}", e))?;

    let script_path = updates_dir.join("apply_update.ps1");
    let log_path = updates_dir.join("update_last.log");
    let flag_path = just_updated_flag_path();

    atomic_write_verified(&script_path, APPLY_UPDATE_PS1)?;

    std::process::Command::new("powershell.exe")
        .creation_flags(CREATE_NO_WINDOW)
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-WindowStyle",
            "Hidden",
            "-File",
        ])
        .arg(&script_path)
        .args(["-AppPid", &std::process::id().to_string()])
        .arg("-ExeDir")
        .arg(exe_dir)
        .arg("-CurrentExe")
        .arg(&current_exe)
        .arg("-ZipPath")
        .arg(zip_path)
        .arg("-DataDir")
        .arg(&data_dir)
        .args(["-TargetVersion", target_version])
        .arg("-LogPath")
        .arg(&log_path)
        .arg("-FlagPath")
        .arg(&flag_path)
        .spawn()
        .map_err(|e| format!("Không thể khởi chạy trình cập nhật tự động: {}", e))?;

    Ok(())
}

fn spawn_update_zip_download(
    tx: Sender<WorkerEvent>,
    ctx: egui::Context,
    url: String,
    file_name: String,
    target_tag: String,
) {
    thread::spawn(move || {
        let res = (|| -> Result<PathBuf, String> {
            let downloads_dir = app_data_dir().join("downloads");
            fs::create_dir_all(&downloads_dir)
                .map_err(|e| format!("Không tạo được thư mục downloads: {}", e))?;

            let safe_name: String = file_name
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
                .collect();
            let final_name = if safe_name.is_empty() {
                "DF-Loadout-Scanner-update.zip".to_string()
            } else {
                safe_name
            };
            let target_path = downloads_dir.join(&final_name);
            let tmp_path = downloads_dir.join(format!("{}.tmp", final_name));

            let resp = ureq::get(&url)
                .set("User-Agent", "DF-Loadout-Scanner")
                .timeout(Duration::from_secs(60))
                .call()
                .map_err(|e| format!("Lỗi tải gói cập nhật: {}", e))?;

            let mut reader = resp.into_reader();
            let mut out_file = fs::File::create(&tmp_path)
                .map_err(|e| format!("Không tạo được tệp tạm: {}", e))?;
            let bytes_copied = std::io::copy(&mut reader, &mut out_file)
                .map_err(|e| {
                    let _ = fs::remove_file(&tmp_path);
                    format!("Lỗi ghi dữ liệu tải về: {}", e)
                })?;
            if bytes_copied == 0 {
                let _ = fs::remove_file(&tmp_path);
                return Err("Tệp tải về rỗng (0 bytes)".to_string());
            }
            drop(out_file);

            if let Err(rename_err) = fs::rename(&tmp_path, &target_path) {
                fs::copy(&tmp_path, &target_path).map_err(|copy_err| {
                    let _ = fs::remove_file(&tmp_path);
                    format!("Không thể lưu tệp ({}; {})", rename_err, copy_err)
                })?;
                let _ = fs::remove_file(&tmp_path);
            }

            validate_update_zip_archive(&target_path)?;
            Ok(target_path)
        })();

        let _ = tx.send(WorkerEvent::UpdateDownloadFinished {
            target_tag,
            result: res,
        });
        ctx.request_repaint();
    });
}

/// Scores a single candidate weapon against pre-normalized OCR text.
/// Returns `Some((score, is_fuzzy))`.
fn score_weapon_candidate(
    cleaned_upper: &str,
    cleaned_stripped: &str,
    cleaned_homo: &str,
    tokens: &[&str],
    first_token_stripped: &str,
    first_two_stripped: &str,
    candidate: &str,
) -> Option<(f64, bool)> {
    let cand_upper = candidate.to_uppercase();
    let cand_stripped = ocr_strip_separators(&cand_upper);
    let cand_homo = ocr_normalize_homoglyphs(&cand_stripped);

    // 1. Direct exact match (case-insensitive)
    if cleaned_upper == cand_upper {
        return Some((1.0, false));
    }

    // 2. Stripped exact match (ignoring whitespace, hyphens, underscores, dots)
    if !cleaned_stripped.is_empty() && cleaned_stripped == cand_stripped {
        return Some((0.99, false));
    }

    // 3. Word-boundary / Token exact matches
    if !tokens.is_empty() {
        // 3a. First two tokens match (e.g. "DESERT EAGLE CUSTOM" -> "DESERT EAGLE")
        if !first_two_stripped.is_empty() && first_two_stripped == cand_stripped {
            return Some((0.97, false));
        }

        // 3b. First token matches (e.g. "M249 LVL 50" -> "M249", "M14 CUSTOM" -> "M14")
        if !first_token_stripped.is_empty() && first_token_stripped == cand_stripped {
            return Some((0.96, false));
        }

        // 3c. Any token matches a candidate exactly (word boundary)
        for tok in tokens {
            let tok_stripped = ocr_strip_separators(tok);
            if !tok_stripped.is_empty() && tok_stripped == cand_stripped {
                return Some((0.95, false));
            }
        }
    }

    // 4. Homoglyph matches (0<->O, 1/L<->I, 5<->S, 8<->B)
    if !cleaned_homo.is_empty() && cleaned_homo == cand_homo {
        return Some((0.94, false));
    }

    // 4b. First token homoglyph match
    if let Some(&first_tok) = tokens.first() {
        let first_tok_homo = ocr_normalize_homoglyphs(&ocr_strip_separators(first_tok));
        if !first_tok_homo.is_empty() && first_tok_homo == cand_homo {
            return Some((0.93, false));
        }
    }

    // 5. Strict Jaro-Winkler fuzzy match
    let len_diff = (cleaned_stripped.len() as isize - cand_stripped.len() as isize).abs();
    let max_allowed_len_diff = if cand_stripped.len() <= 3 { 1 } else { 3 };
    if len_diff > max_allowed_len_diff {
        return None;
    }

    let score_full = strsim::jaro_winkler(cleaned_upper, &cand_upper);
    let score_stripped = strsim::jaro_winkler(cleaned_stripped, &cand_stripped);
    let score_homo = strsim::jaro_winkler(cleaned_homo, &cand_homo);
    let max_score = score_full.max(score_stripped).max(score_homo);

    if max_score >= 0.75 {
        Some((max_score, true))
    } else {
        None
    }
}

fn match_weapon_in_slice<'a>(cleaned: &str, candidates: &[&'a String]) -> Option<(&'a str, f64)> {
    if cleaned.trim().is_empty() {
        return None;
    }

    let cleaned_upper = cleaned.trim().to_uppercase();
    let cleaned_stripped = ocr_strip_separators(&cleaned_upper);
    let cleaned_homo = ocr_normalize_homoglyphs(&cleaned_stripped);

    let tokens: Vec<&str> = cleaned_upper.split_whitespace().collect();
    let first_token_stripped = tokens
        .first()
        .map(|t| ocr_strip_separators(t))
        .unwrap_or_default();
    let first_two_stripped = if tokens.len() >= 2 {
        ocr_strip_separators(&format!("{}{}", tokens[0], tokens[1]))
    } else {
        String::new()
    };

    // Score every candidate to determine both Top-1 (`best`) and Top-2 (`second_best`)
    let mut scored: Vec<(&'a str, f64, bool, usize, usize)> = Vec::new();
    for candidate in candidates {
        if let Some((score, is_fuzzy)) = score_weapon_candidate(
            &cleaned_upper,
            &cleaned_stripped,
            &cleaned_homo,
            &tokens,
            &first_token_stripped,
            &first_two_stripped,
            candidate,
        ) {
            let stripped_len = ocr_strip_separators(&candidate.to_uppercase()).len();
            let alnum_len = candidate.chars().filter(|c| c.is_alphanumeric()).count();
            scored.push((candidate.as_str(), score, is_fuzzy, stripped_len, alnum_len));
        }
    }

    if scored.is_empty() {
        return None;
    }

    // Sort descending by score, preferring longer stripped candidate names on ties
    scored.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.3.cmp(&a.3))
    });

    let (best_cand, mut best_score, best_fuzzy, _, best_alnum_len) = scored[0];
    let second_score = scored.get(1).map(|s| s.1).unwrap_or(0.0);

    // Short-weapon (<= 3 alphanumeric chars) safety gate:
    // Preserve absolute exact matches (>= 0.99); otherwise if matched via fuzzy branch
    // or Top-1 vs Top-2 margin < 0.08, clamp into manual confirmation zone (0.85).
    if best_alnum_len <= 3 && best_score < 0.99 {
        let margin = best_score - second_score;
        if best_fuzzy || margin < 0.08 {
            best_score = best_score.min(0.85);
        }
    }

    Some((best_cand, best_score))
}

fn match_weapon<'a>(cleaned: &str, candidates: &'a [String]) -> Option<(&'a str, f64)> {
    let refs: Vec<&'a String> = candidates.iter().collect();
    match_weapon_in_slice(cleaned, &refs)
}

#[derive(Debug, Clone)]
struct OcrRecognitionDecision<'a> {
    weapon: Option<&'a str>,
    score: f64,
    margin: f64,
    category: Option<OcrWeaponCategory>,
    category_locked: bool,
    code_alnum_len: usize,
    ctc_confidence: f32,
    can_auto_copy: bool,
    cleaned: String,
}

impl<'a> OcrRecognitionDecision<'a> {
    fn debug_summary(&self) -> String {
        format!(
            "weapon={:?} score={:.3} margin={:.3} locked={} len={} ctc={:.3} auto={} clean='{}'",
            self.weapon,
            self.score,
            self.margin,
            self.category_locked,
            self.code_alnum_len,
            self.ctc_confidence,
            self.can_auto_copy,
            self.cleaned
        )
    }
}

fn common_suffix_len(a: &str, b: &str) -> usize {
    a.chars()
        .rev()
        .zip(b.chars().rev())
        .take_while(|(ca, cb)| ca == cb)
        .count()
}

/// Scores an occluded/corrupted OCR weapon code against a candidate within the locked category.
/// Prioritizes intact occluded-prefix suffixes (`K47` -> `MK47` over `K437`) and handles
/// leading half-cut glyphs (`3552` -> `SG552`, `317` -> `KC-17`, `JG` -> `AUG`, `7D` -> `SVD`)
/// as well as length-adaptive Levenshtein matching (`<= 1` short, `<= 2` when len >= 6).
fn score_occluded_candidate(code_stripped: &str, cand_stripped: &str) -> f64 {
    if code_stripped.is_empty() || cand_stripped.is_empty() {
        return 0.0;
    }
    if code_stripped == cand_stripped {
        return 1.0;
    }

    let c_len = code_stripped.chars().count();
    let w_len = cand_stripped.chars().count();
    let sl = common_suffix_len(code_stripped, cand_stripped);

    // 1. Intact leading-occluded suffix (e.g. `4A1` -> `M4A1`, `K47` -> `MK47`, `57` -> `AR57`, `SS` -> `VSS`)
    if c_len >= 2 && cand_stripped.ends_with(code_stripped) {
        return 0.94 + 0.04 * (c_len as f64 / w_len as f64);
    }
    if c_len >= 3 {
        let code_homo = ocr_normalize_homoglyphs(code_stripped);
        let cand_homo = ocr_normalize_homoglyphs(cand_stripped);
        if cand_homo.ends_with(&code_homo) {
            return 0.93 + 0.03 * (c_len as f64 / w_len as f64);
        }
    }

    // 2. Single-char suffix match when candidate ends cleanly with it (always guarded by len >= 2 rule)
    if c_len == 1 && cand_stripped.ends_with(code_stripped) {
        return 0.82;
    }

    // 2b. Single stray trailing noise character after an intact weapon code (e.g. `S12KU` -> `S12K`)
    if w_len >= 3 && c_len == w_len + 1 && code_stripped.starts_with(cand_stripped) {
        return 0.95;
    }

    // 3. Strong body/prefix 1-edit match (`S552` -> `SG552`, `QZ951` -> `QBZ951`) vs interior mismatch (`K47` vs `K437`)
    let dist = strsim::levenshtein(code_stripped, cand_stripped);
    if c_len >= 3 && w_len >= 3 && dist <= 1 {
        if sl >= 2 || c_len >= 4 {
            return 0.94;
        }
        return 0.82;
    }

    // 4. Corrupted-leading-glyph suffix match (`3552`->`SG552`, `3Z951`->`QBZ951`, `317`->`KC17`, `137`->`K437`, `116`->`K416`, `20171`->`QCQ171`, `JG`->`AUG`, `7D`->`SVD`, `VM`->`AWM`)
    let len_diff = (c_len as isize - w_len as isize).abs();
    if (sl >= 2 || (sl == 1 && c_len == 2 && w_len <= 3)) && len_diff <= 2 {
        let prefix_c_len = c_len.saturating_sub(sl);
        let prefix_w_len = w_len.saturating_sub(sl);
        if prefix_c_len <= prefix_w_len + 1 {
            if sl == 1 {
                return 0.80;
            }
            return 0.80 + 0.10 * (sl as f64 / w_len as f64);
        }
    }

    // 5. Longer weapon names (>= 6 chars, e.g. `tomoson` -> `THOMPSON`) allow Levenshtein <= 2 inside locked category
    if c_len >= 6 && dist <= 2 {
        return (0.88 - 0.04 * dist as f64).max(0.75);
    }

    0.0
}

/// Core matcher returning full decision details (`weapon`, `score`, `margin`, `category`, `can_auto_copy`).
/// Enforces hard category locking (no full-DB fallback once category is identified) and 3-guard auto-copy >= 0.75.
fn recognize_weapon_with_decision<'a>(
    raw_ocr: &str,
    ctc_confidence: f32,
    forced_category: Option<OcrWeaponCategory>,
    candidates: &'a [String],
) -> OcrRecognitionDecision<'a> {
    let parsed = parse_weapon_header(raw_ocr);
    let effective_cat = forced_category.or(parsed.category);

    // Handle explicit title overrides (`Cung Phức Hợp`, `Lever-action`)
    if let Some(spec_name) = parsed.special_weapon {
        if let Some(cand) = candidates
            .iter()
            .find(|c| c.eq_ignore_ascii_case(spec_name))
        {
            let ctc_clamped = (ctc_confidence as f64).clamp(0.40, 1.0);
            let mut score = 0.94 * (0.94 + 0.06 * ctc_clamped);
            if ctc_confidence < 0.55 {
                score = score.min(0.85);
            }
            let alnum_len = parsed.code.chars().filter(|c| c.is_ascii_alphanumeric()).count();
            let tier = evaluate_auto_copy_guards(score, score, true, alnum_len, ctc_confidence);
            return OcrRecognitionDecision {
                weapon: Some(cand.as_str()),
                score,
                margin: score,
                category: effective_cat,
                category_locked: true,
                code_alnum_len: alnum_len,
                ctc_confidence,
                can_auto_copy: tier == ConfidenceTier::High,
                cleaned: parsed.code,
            };
        }
    }

    if parsed.code.is_empty() {
        return OcrRecognitionDecision {
            weapon: None,
            score: 0.0,
            margin: 0.0,
            category: effective_cat,
            category_locked: effective_cat.is_some(),
            code_alnum_len: 0,
            ctc_confidence,
            can_auto_copy: false,
            cleaned: String::new(),
        };
    }

    let normalized_code = resolve_ocr_code_normalization(&parsed.code)
        .map(|s| s.to_string())
        .unwrap_or_else(|| parsed.code.clone());
    let code_upper = normalized_code.to_uppercase();
    let code_stripped = ocr_strip_separators(&code_upper);
    let code_homo = ocr_normalize_homoglyphs(&code_stripped);
    let code_alnum_len = code_stripped
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .count();
    let tokens: Vec<&str> = code_upper.split_whitespace().collect();
    let first_tok = tokens
        .first()
        .map(|t| ocr_strip_separators(t))
        .unwrap_or_default();
    let first_two = if tokens.len() >= 2 {
        ocr_strip_separators(&format!("{}{}", tokens[0], tokens[1]))
    } else {
        String::new()
    };
    let ctc_factor = 0.94 + 0.06 * (ctc_confidence as f64).clamp(0.40, 1.0);

    if let Some(cat) = effective_cat {
        // HARD CATEGORY LOCK: only evaluate weapons inside `cat`; never fallback to unrelated categories
        let cat_pool: Vec<&'a String> = candidates
            .iter()
            .filter(|c| cat.contains_weapon(&c.to_uppercase()))
            .collect();

        let mut scored: Vec<(&'a str, f64, usize)> = Vec::new();
        for &cand in &cat_pool {
            let cand_upper = cand.to_uppercase();
            let cand_stripped = ocr_strip_separators(&cand_upper);
            let occ_score = score_occluded_candidate(&code_stripped, &cand_stripped);
            // Only consult legacy Jaro-Winkler scoring when geometric suffix/edit scoring returned 0.0
            // (e.g. multi-word or extra-suffix tokens like `M82A1` / `CVECTOR`), so explicit penalties like `K47` vs `K437` hold.
            let best_s = if occ_score > 0.0 {
                occ_score
            } else {
                score_weapon_candidate(
                    &code_upper,
                    &code_stripped,
                    &code_homo,
                    &tokens,
                    &first_tok,
                    &first_two,
                    cand,
                )
                .map(|(s, _)| s * 0.96)
                .unwrap_or(0.0)
            };
            if best_s > 0.0 {
                scored.push((cand.as_str(), best_s, cand_stripped.len()));
            }
        }

        if scored.is_empty() {
            return OcrRecognitionDecision {
                weapon: None,
                score: 0.0,
                margin: 0.0,
                category: Some(cat),
                category_locked: true,
                code_alnum_len,
                ctc_confidence,
                can_auto_copy: false,
                cleaned: normalized_code,
            };
        }

        scored.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.2.cmp(&b.2))
                .then_with(|| a.0.cmp(b.0))
        });

        let top1_cand = scored[0].0;
        let top1_base = scored[0].1;
        let top2_base = scored.get(1).map(|s| s.1).unwrap_or(0.0);

        // Reject broad >= 3-way 1-char suffix ties inside large category pools
        if code_alnum_len == 1 {
            let tie_count = scored
                .iter()
                .filter(|s| (s.1 - top1_base).abs() < 1e-6)
                .count();
            if tie_count >= 3 {
                return OcrRecognitionDecision {
                    weapon: None,
                    score: 0.0,
                    margin: 0.0,
                    category: Some(cat),
                    category_locked: true,
                    code_alnum_len,
                    ctc_confidence,
                    can_auto_copy: false,
                    cleaned: normalized_code,
                };
            }
        }

        let mut final_score = top1_base * ctc_factor;
        let margin = (top1_base - top2_base).max(0.0) * ctc_factor;

        if code_alnum_len == 1 {
            final_score = final_score.min(0.82);
        }
        if ctc_confidence < 0.55 {
            final_score = final_score.min(0.85);
        }

        let tier = evaluate_auto_copy_guards(
            final_score,
            margin,
            true,
            code_alnum_len,
            ctc_confidence,
        );
        let weapon_opt = if tier != ConfidenceTier::Rejected {
            Some(top1_cand)
        } else {
            None
        };

        OcrRecognitionDecision {
            weapon: weapon_opt,
            score: final_score,
            margin,
            category: Some(cat),
            category_locked: true,
            code_alnum_len,
            ctc_confidence,
            can_auto_copy: tier == ConfidenceTier::High,
            cleaned: normalized_code,
        }
    } else {
        // No category detected: evaluate across all weapons with `category_locked = false`
        let all_refs: Vec<&'a String> = candidates.iter().collect();
        if let Some((cand, base_score)) = match_weapon_in_slice(&normalized_code, &all_refs) {
            let mut final_score = base_score * ctc_factor;
            if code_alnum_len == 1 {
                final_score = final_score.min(0.82);
            }
            if ctc_confidence < 0.55 {
                final_score = final_score.min(0.85);
            }
            let tier = if final_score >= 0.75 {
                ConfidenceTier::Suspicious
            } else {
                ConfidenceTier::Rejected
            };
            OcrRecognitionDecision {
                weapon: if tier != ConfidenceTier::Rejected {
                    Some(cand)
                } else {
                    None
                },
                score: final_score,
                margin: 0.0,
                category: None,
                category_locked: false,
                code_alnum_len,
                ctc_confidence,
                can_auto_copy: false,
                cleaned: normalized_code,
            }
        } else {
            OcrRecognitionDecision {
                weapon: None,
                score: 0.0,
                margin: 0.0,
                category: None,
                category_locked: false,
                code_alnum_len,
                ctc_confidence,
                can_auto_copy: false,
                cleaned: normalized_code,
            }
        }
    }
}

#[cfg(test)]
fn recognize_weapon_from_title_with_conf<'a>(
    raw_ocr: &str,
    ctc_confidence: f32,
    candidates: &'a [String],
) -> (Option<(&'a str, f64)>, String) {
    let d = recognize_weapon_with_decision(raw_ocr, ctc_confidence, None, candidates);
    (d.weapon.map(|w| (w, d.score)), d.cleaned)
}

#[cfg(test)]
fn recognize_weapon_from_title<'a>(
    raw_ocr: &str,
    candidates: &'a [String],
) -> (Option<(&'a str, f64)>, String) {
    recognize_weapon_from_title_with_conf(raw_ocr, 1.0, candidates)
}

/// Computes the HUD ROI rectangle using a centered 16:9 virtual viewport so 16:9, 21:9, and 16:10 displays align identically.
fn compute_hud_16_9_roi_rect(
    screen_w: u32,
    screen_h: u32,
    x_start_pct: f32,
    x_end_pct: f32,
    y_start_pct: f32,
    y_end_pct: f32,
) -> (u32, u32, u32, u32) {
    let virtual_w = (screen_h as f32 * 16.0 / 9.0).min(screen_w as f32);
    let offset_x = (screen_w as f32 - virtual_w) / 2.0;

    let raw_x0 = (offset_x + virtual_w * x_start_pct).round().max(0.0) as u32;
    let raw_x1 = (offset_x + virtual_w * x_end_pct).round().max(0.0) as u32;
    let raw_y0 = (screen_h as f32 * y_start_pct).round().max(0.0) as u32;
    let raw_y1 = (screen_h as f32 * y_end_pct).round().max(0.0) as u32;

    let crop_x = raw_x0.min(screen_w.saturating_sub(1));
    let crop_y = raw_y0.min(screen_h.saturating_sub(1));
    let crop_w = raw_x1
        .saturating_sub(crop_x)
        .min(screen_w.saturating_sub(crop_x))
        .max(1);
    let crop_h = raw_y1
        .saturating_sub(crop_y)
        .min(screen_h.saturating_sub(crop_y))
        .max(1);

    (crop_x, crop_y, crop_w, crop_h)
}

/// Crops the centered 16:9 HUD region from `full_image`.
fn crop_hud_16_9_region(
    full_image: &xcap::image::RgbaImage,
    x_start_pct: f32,
    x_end_pct: f32,
    y_start_pct: f32,
    y_end_pct: f32,
) -> xcap::image::RgbaImage {
    let (crop_x, crop_y, crop_w, crop_h) = compute_hud_16_9_roi_rect(
        full_image.width(),
        full_image.height(),
        x_start_pct,
        x_end_pct,
        y_start_pct,
        y_end_pct,
    );
    xcap::image::imageops::crop_imm(full_image, crop_x, crop_y, crop_w, crop_h).to_image()
}

/// Pass 2 helper: locates the active bright-white-bordered weapon card inside the centered 16:9
/// left weapon list (`x: 7.0%..23.6%`, `y: 13.0%..90.0%`) and returns ONLY the top-left code strip
/// (`x_card_left + 2px` to `min(x_card_right, offset_x + 0.170 * virtual_w)`) excluding skin names.
fn find_selected_card_prefix_rect_16_9(full_image: &xcap::image::RgbaImage) -> Option<(u32, u32, u32, u32)> {
    let img_w = full_image.width();
    let img_h = full_image.height();
    let virtual_w = (img_h as f32 * 16.0 / 9.0).min(img_w as f32);
    let offset_x = (img_w as f32 - virtual_w) / 2.0;

    let col_x = ((offset_x + virtual_w * 0.070).round().max(0.0) as u32).min(img_w);
    let col_x2 = ((offset_x + virtual_w * 0.236).round().max(0.0) as u32).min(img_w);
    let col_w = col_x2.saturating_sub(col_x);
    let col_y = ((img_h as f32 * 0.130).round().max(0.0) as u32).min(img_h);
    let col_y2 = ((img_h as f32 * 0.900).round().max(0.0) as u32).min(img_h);
    let col_h = col_y2.saturating_sub(col_y);

    if col_w < 50 || col_h < 100 {
        return None;
    }

    let cw = col_w as usize;
    let ch = col_h as usize;
    let mut row_white = vec![0u32; ch];
    let stride = ch + 1;
    let mut col_prefix = vec![0u32; cw * stride];

    for rx in 0..cw {
        let px = col_x + rx as u32;
        let col_offset = rx * stride;
        let mut acc = 0u32;
        for ry in 0..ch {
            let py = col_y + ry as u32;
            let p = full_image.get_pixel(px, py);
            let is_white = if p[0] >= 185 && p[1] >= 185 && p[2] >= 185 { 1u32 } else { 0u32 };
            row_white[ry] += is_white;
            acc += is_white;
            col_prefix[col_offset + ry + 1] = acc;
        }
    }

    let h_min = ((img_h as f64 * 0.095).round() as usize).max(50).min(ch);
    let h_max = ((img_h as f64 * 0.138).round() as usize).min(ch).max(h_min);
    let left_limit = (cw * 22 / 100).max(12).min(cw / 2);
    let right_start = cw.saturating_sub((cw * 22 / 100).max(12)).max(cw / 2);

    let mut best_y: usize = 0;
    let mut best_left_rx: usize = 0;
    let mut best_right_rx: usize = cw.saturating_sub(1);
    let mut max_score: f64 = 0.0;

    let max_ry = ch.saturating_sub(h_min);
    for ry in (0..=max_ry).step_by(2) {
        let top_end = (ry + 2).min(ch - 1);
        let mut best_top_ry = ry;
        let mut top_max = 0u32;
        for r in ry..=top_end {
            if row_white[r] > top_max {
                top_max = row_white[r];
                best_top_ry = r;
            }
        }
        let top_ratio = top_max as f64 / cw as f64;
        if top_ratio < 0.08 {
            continue;
        }

        let mut h = h_min;
        while h <= h_max {
            let bot_y = (best_top_ry + h).min(ch - 1);
            let span_u = bot_y.saturating_sub(best_top_ry);
            if span_u < (h_min * 3) / 4 {
                break;
            }
            let bot_start = bot_y.saturating_sub(2);
            let mut bot_max = 0u32;
            for r in bot_start..=bot_y {
                if row_white[r] > bot_max {
                    bot_max = row_white[r];
                }
            }
            let bottom_ratio = bot_max as f64 / cw as f64;
            let span = span_u as f64;

            let mut left_max = 0u32;
            let mut cand_left_rx = 0usize;
            for rx in 0..left_limit {
                let off = rx * stride;
                let cnt = col_prefix[off + bot_y] - col_prefix[off + best_top_ry];
                if cnt > left_max {
                    left_max = cnt;
                    cand_left_rx = rx;
                }
            }
            let left_ratio = left_max as f64 / span;

            let mut right_max = 0u32;
            let mut cand_right_rx = cw - 1;
            for rx in right_start..cw {
                let off = rx * stride;
                let cnt = col_prefix[off + bot_y] - col_prefix[off + best_top_ry];
                if cnt > right_max {
                    right_max = cnt;
                    cand_right_rx = rx;
                }
            }
            let right_ratio = right_max as f64 / span;

            let top_lr_min = top_ratio.min(left_ratio).min(right_ratio);
            let bot_lr_min = bottom_ratio.min(left_ratio).min(right_ratio);
            let score =
                top_ratio + bottom_ratio + left_ratio + right_ratio + 1.5 * top_lr_min + 1.0 * bot_lr_min;

            if score > max_score {
                max_score = score;
                best_y = best_top_ry;
                best_left_rx = cand_left_rx;
                best_right_rx = cand_right_rx;
            }

            if h == h_max || best_top_ry + h >= ch - 1 {
                break;
            }
            h = (h + 3).min(h_max);
        }
    }

    if max_score < 1.20 {
        return None;
    }

    let abs_y = col_y + best_y as u32;
    let card_left = col_x + best_left_rx as u32;
    let card_right = col_x + best_right_rx as u32;
    let x_cap = (offset_x + virtual_w * 0.170).round().max(0.0) as u32;

    let strip_x0 = (card_left + 2).min(img_w.saturating_sub(1));
    let strip_x1 = card_right.min(x_cap).min(img_w);
    let strip_w = strip_x1.saturating_sub(strip_x0).max(1);

    let title_y = (abs_y + ((img_h as f32 * 0.005).round().max(2.0) as u32)).min(img_h.saturating_sub(1));
    let title_h = ((img_h as f32 * 0.036).round().max(16.0) as u32)
        .min(img_h.saturating_sub(title_y))
        .max(1);

    Some((strip_x0, title_y, strip_w, title_h))
}

/// Cleans OCR text from Pass 2 top-left card crop by removing skin delimiters (`-`, `–`, `—`, `_`)
/// without breaking hyphenated weapon codes (e.g. `SR-3M`, `QBZ95-1`, `AR-57`).
fn clean_left_card_ocr_text(raw: &str) -> String {
    let with_spaces = raw.replace('_', " ");
    let stripped_skin = strip_skin_suffix(&with_spaces);
    stripped_skin
        .trim()
        .trim_end_matches(|c: char| matches!(c, '-' | '–' | '—' | '_' | ' '))
        .to_string()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MonitorDescriptor {
    stable_id: String,
    label: String,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    is_primary: bool,
}

fn format_monitor_stable_id(name: &str, width: u32, height: u32) -> String {
    format!("{} ({}x{})", name.trim(), width, height)
}

/// Enumerates connected monitors returning `(stable_id, display_label)` pairs.
fn enumerate_monitors(lang: AppLang) -> Vec<(String, String)> {
    let Ok(monitors) = xcap::Monitor::all() else {
        return Vec::new();
    };
    monitors
        .into_iter()
        .enumerate()
        .map(|(idx, m)| {
            let stable_id = format_monitor_stable_id(m.name(), m.width(), m.height());
            let primary_tag = if m.is_primary() {
                lang.t(" [Chính]", " [Primary]")
            } else {
                ""
            };
            let label = format!(
                "{} {}: {}{}",
                lang.t("Màn", "Display"),
                idx + 1,
                stable_id,
                primary_tag
            );
            (stable_id, label)
        })
        .collect()
}

/// Retrieves `(left, top, width, height)` of the monitor hosting the active foreground window.
fn get_foreground_monitor_rect() -> Option<(i32, i32, i32, i32)> {
    unsafe {
        let hwnd: HWND = GetForegroundWindow();
        if hwnd.0.is_null() {
            return None;
        }
        let hmon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        if hmon.0.is_null() {
            return None;
        }
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(hmon, &mut mi).as_bool() {
            let r = mi.rcMonitor;
            Some((r.left, r.top, r.right - r.left, r.bottom - r.top))
        } else {
            None
        }
    }
}

/// Pure monitor resolution logic:
/// 1. Matches `selected_monitor_id` by stable device+resolution ID if still connected.
/// 2. If `None` OR saved monitor disconnected, falls back automatically to the foreground window's monitor.
/// 3. Final fallback: primary monitor or index 0.
fn resolve_monitor_index(
    monitors: &[MonitorDescriptor],
    selected_monitor_id: Option<&str>,
    foreground_rect: Option<(i32, i32, i32, i32)>,
) -> Option<usize> {
    if monitors.is_empty() {
        return None;
    }

    if let Some(target_id) = selected_monitor_id {
        if let Some(pos) = monitors.iter().position(|m| m.stable_id == target_id) {
            return Some(pos);
        }
        // Saved monitor was unplugged -> fall back cleanly to foreground auto-detection
    }

    if let Some((fx, fy, fw, fh)) = foreground_rect {
        if let Some(pos) = monitors.iter().position(|m| {
            m.x == fx && m.y == fy && m.width as i32 == fw && m.height as i32 == fh
        }) {
            return Some(pos);
        }
        if let Some(pos) = monitors.iter().position(|m| m.x == fx && m.y == fy) {
            return Some(pos);
        }
        let f_right = fx as i64 + fw as i64;
        let f_bottom = fy as i64 + fh as i64;
        let mut best_pos: Option<usize> = None;
        let mut max_overlap: i64 = 0;
        for (i, m) in monitors.iter().enumerate() {
            let mx1 = m.x as i64;
            let my1 = m.y as i64;
            let mx2 = mx1 + m.width as i64;
            let my2 = my1 + m.height as i64;
            let ox = (f_right.min(mx2) - (fx as i64).max(mx1)).max(0);
            let oy = (f_bottom.min(my2) - (fy as i64).max(my1)).max(0);
            let area = ox * oy;
            if area > max_overlap {
                max_overlap = area;
                best_pos = Some(i);
            }
        }
        if let Some(pos) = best_pos {
            return Some(pos);
        }
    }

    monitors.iter().position(|m| m.is_primary).or(Some(0))
}

/// Selects the target monitor using stable device ID, falling back to foreground window monitor.
fn select_target_monitor(
    selected_monitor_id: Option<&str>,
) -> Result<xcap::Monitor, Box<dyn std::error::Error + Send + Sync>> {
    let mut monitors = xcap::Monitor::all()?;
    if monitors.is_empty() {
        return Err("Không tìm thấy màn hình nào".into());
    }

    let descriptors: Vec<MonitorDescriptor> = monitors
        .iter()
        .map(|m| {
            let stable_id = format_monitor_stable_id(m.name(), m.width(), m.height());
            MonitorDescriptor {
                label: stable_id.clone(),
                stable_id,
                x: m.x(),
                y: m.y(),
                width: m.width(),
                height: m.height(),
                is_primary: m.is_primary(),
            }
        })
        .collect();

    let idx = resolve_monitor_index(&descriptors, selected_monitor_id, get_foreground_monitor_rect())
        .ok_or("Không tìm thấy màn hình phù hợp")?;
    Ok(monitors.swap_remove(idx))
}

/// Truncates raw OCR debug text on Unicode scalar boundaries for compact status bar display.
fn truncate_ocr_preview(s: &str, max_chars: usize) -> String {
    let trimmed = s.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    if max_chars <= 3 {
        return trimmed.chars().take(max_chars).collect();
    }
    let take_chars = max_chars - 3;
    let prefix: String = trimmed.chars().take(take_chars).collect();
    format!("{}...", prefix.trim_end())
}

/// Initializes `arboard::Clipboard` retrying up to `max_retries` times (35ms apart)
/// to overcome transient Windows clipboard locks from other processes.
fn init_clipboard_with_retry(max_retries: usize) -> Result<arboard::Clipboard, String> {
    let attempts = max_retries.max(1);
    let mut last_err = String::new();
    for attempt in 0..attempts {
        match Clipboard::new() {
            Ok(cb) => return Ok(cb),
            Err(e) => {
                last_err = e.to_string();
                if attempt + 1 < attempts {
                    sleep(Duration::from_millis(35));
                }
            }
        }
    }
    Err(last_err)
}

/// Writes text to the Windows Clipboard retrying up to `max_retries` times (35ms apart),
/// re-initializing `arboard::Clipboard` via `init_clipboard_with_retry` if all attempts fail.
fn copy_to_clipboard_with_retry(
    clipboard: &mut arboard::Clipboard,
    text: &str,
    max_retries: usize,
) -> Result<(), String> {
    let attempts = max_retries.max(1);
    let mut last_err = String::new();

    for attempt in 0..attempts {
        match clipboard.set_text(text.to_string()) {
            Ok(()) => return Ok(()),
            Err(e) => {
                last_err = e.to_string();
                if attempt + 1 < attempts {
                    sleep(Duration::from_millis(35));
                }
            }
        }
    }

    match init_clipboard_with_retry(2) {
        Ok(mut fresh_cb) => match fresh_cb.set_text(text.to_string()) {
            Ok(()) => {
                *clipboard = fresh_cb;
                Ok(())
            }
            Err(e) => Err(format!("{} (retry failed: {})", last_err, e)),
        },
        Err(e) => Err(format!("{} (reinit failed: {})", last_err, e)),
    }
}

#[derive(Debug, Clone)]
struct ClipboardBatchReport {
    success_count: usize,
    total_count: usize,
}

/// Writes a priority-ordered slice of codes (top build at index 0) into Clipboard/Win+V
/// in reverse order so index 0 ends up on `Ctrl + V`.
/// Succeeds iff index 0 is confirmed written, reporting exact `(success_count, total_count)`.
fn copy_codes_sequence_to_clipboard(
    cb_opt: &mut Option<Clipboard>,
    codes: &[String],
    delay_ms: u64,
) -> Result<ClipboardBatchReport, String> {
    let total_count = codes.len();
    if total_count == 0 {
        return Ok(ClipboardBatchReport {
            success_count: 0,
            total_count: 0,
        });
    }

    if cb_opt.is_none() {
        *cb_opt = Some(init_clipboard_with_retry(3)?);
    }
    let cb = cb_opt
        .as_mut()
        .ok_or_else(|| "Không thể truy cập Windows Clipboard".to_string())?;

    let mut success_count = 0usize;
    let mut top_result: Result<(), String> = Err("Không có dữ liệu sao chép".to_string());
    for (rev_idx, code) in codes.iter().rev().enumerate() {
        let res = copy_to_clipboard_with_retry(cb, code, 3);
        let is_top_priority = rev_idx + 1 == total_count;
        if res.is_ok() {
            success_count += 1;
        }
        if is_top_priority {
            top_result = res;
        } else {
            sleep(Duration::from_millis(delay_ms));
        }
    }

    top_result?;
    Ok(ClipboardBatchReport {
        success_count,
        total_count,
    })
}

struct CapturedScanOutcome<'a> {
    weapon: Option<&'a str>,
    score: f64,
    can_auto_copy: bool,
    raw_ocr: String,
    cleaned: String,
}

/// Executes Pass 1A -> Pass 1B (left 30% gamma boost) -> Pass 2 (left-card prefix + locked category)
/// on a captured screen image. Saves intermediate crops ONLY when `DF_OCR_DEBUG=1`.
fn recognize_from_captured_image<'a>(
    ocr_engine: &mut PaddleOcrEngine,
    full_image: &xcap::image::RgbaImage,
    candidates: &'a [String],
) -> Result<CapturedScanOutcome<'a>, Box<dyn std::error::Error + Send + Sync>> {
    let debug_dir = if std::env::var("DF_OCR_DEBUG").as_deref() == Ok("1") {
        let dir = app_data_dir().join("debug_ocr");
        let _ = fs::create_dir_all(&dir);
        Some(dir)
    } else {
        None
    };

    let mut debug_lines: Vec<String> = Vec::new();

    // Pass 1A: Tight center header inside centered 16:9 viewport (x: 24.0% -> 41.5%, y: 15.0% -> 19.6%)
    let center_img = crop_hud_16_9_region(full_image, 0.240, 0.415, 0.150, 0.196);
    if let Some(ref dir) = debug_dir {
        let _ = center_img.save(dir.join("pass1a_center.png"));
    }
    let (raw_1a, ctc_1a) = ocr_engine.recognize(&center_img)?;
    let dec_1a = recognize_weapon_with_decision(&raw_1a, ctc_1a, None, candidates);
    if debug_dir.is_some() {
        debug_lines.push(format!("pass1a raw='{}' {}", raw_1a.trim(), dec_1a.debug_summary()));
    }

    if dec_1a.can_auto_copy && dec_1a.score >= 0.85 {
        if let Some(ref dir) = debug_dir {
            let _ = fs::write(dir.join("decision.log"), debug_lines.join("\n"));
        }
        return Ok(CapturedScanOutcome {
            weapon: dec_1a.weapon,
            score: dec_1a.score,
            can_auto_copy: true,
            raw_ocr: raw_1a,
            cleaned: dec_1a.cleaned,
        });
    }

    let mut best_dec = dec_1a.clone();
    let mut best_raw = raw_1a.clone();

    // Pass 1B: Local contrast stretch + gamma 0.55 on the left 30% shadowed by 18+ box
    let boosted_img = boost_left_region_gamma(&center_img, 0.30, 0.55);
    if let Some(ref dir) = debug_dir {
        let _ = boosted_img.save(dir.join("pass1b_boosted.png"));
    }
    if let Ok((raw_1b, ctc_1b)) = ocr_engine.recognize(&boosted_img) {
        let dec_1b = recognize_weapon_with_decision(&raw_1b, ctc_1b, None, candidates);
        if debug_dir.is_some() {
            debug_lines.push(format!("pass1b raw='{}' {}", raw_1b.trim(), dec_1b.debug_summary()));
        }
        let replace_1b = (dec_1b.can_auto_copy && !best_dec.can_auto_copy)
            || (dec_1b.can_auto_copy == best_dec.can_auto_copy && dec_1b.score > best_dec.score);
        if replace_1b {
            best_dec = dec_1b;
            best_raw = raw_1b;
        } else if best_dec.category.is_none() && dec_1b.category.is_some() {
            best_dec.category = dec_1b.category;
        }
    }

    if best_dec.can_auto_copy {
        if let Some(ref dir) = debug_dir {
            let _ = fs::write(dir.join("decision.log"), debug_lines.join("\n"));
        }
        return Ok(CapturedScanOutcome {
            weapon: best_dec.weapon,
            score: best_dec.score,
            can_auto_copy: true,
            raw_ocr: best_raw,
            cleaned: best_dec.cleaned,
        });
    }

    // Pass 2: Left selected card top-left code strip (immune to 18+ overlay & skin names),
    // paired with the weapon category locked during Pass 1.
    let locked_cat = best_dec.category;
    if let Some((lx, ly, lw, lh)) = find_selected_card_prefix_rect_16_9(full_image) {
        let left_crop = xcap::image::imageops::crop_imm(full_image, lx, ly, lw, lh).to_image();
        if let Some(ref dir) = debug_dir {
            let _ = left_crop.save(dir.join("pass2_leftcard.png"));
        }
        if let Ok((raw_2, ctc_2)) = ocr_engine.recognize(&left_crop) {
            let clean_2 = clean_left_card_ocr_text(&raw_2);
            if !clean_2.is_empty() {
                let dec_2 = recognize_weapon_with_decision(&clean_2, ctc_2, locked_cat, candidates);
                if debug_dir.is_some() {
                    debug_lines.push(format!("pass2 raw='{}' clean_2='{}' {}", raw_2.trim(), clean_2, dec_2.debug_summary()));
                }
                let replace_2 = (dec_2.can_auto_copy && !best_dec.can_auto_copy)
                    || (dec_2.weapon.is_some()
                        && (best_dec.weapon.is_none() || dec_2.score > best_dec.score));
                if replace_2 {
                    if let Some(ref dir) = debug_dir {
                        let _ = fs::write(dir.join("decision.log"), debug_lines.join("\n"));
                    }
                    let diag_raw = if best_raw.trim().is_empty() {
                        raw_2
                    } else {
                        format!("{} | Card: {}", best_raw.trim(), raw_2.trim())
                    };
                    return Ok(CapturedScanOutcome {
                        weapon: dec_2.weapon,
                        score: dec_2.score,
                        can_auto_copy: dec_2.can_auto_copy,
                        raw_ocr: diag_raw,
                        cleaned: dec_2.cleaned,
                    });
                }
            }
        }
    }

    if let Some(ref dir) = debug_dir {
        let _ = fs::write(dir.join("decision.log"), debug_lines.join("\n"));
    }

    Ok(CapturedScanOutcome {
        weapon: best_dec.weapon,
        score: best_dec.score,
        can_auto_copy: best_dec.can_auto_copy,
        raw_ocr: best_raw,
        cleaned: best_dec.cleaned,
    })
}

fn capture_and_recognize_weapon<'a>(
    ocr_engine: &mut PaddleOcrEngine,
    candidates: &'a [String],
    selected_monitor_id: Option<&str>,
) -> Result<CapturedScanOutcome<'a>, Box<dyn std::error::Error + Send + Sync>> {
    let target_monitor = select_target_monitor(selected_monitor_id)?;
    let full_image = target_monitor.capture_image()?;
    recognize_from_captured_image(ocr_engine, &full_image, candidates)
}

fn sort_builds_prioritized(builds: &[&BuildItem], favorite_codes: &HashSet<String>) -> Vec<BuildItem> {
    let mut sorted = builds.to_vec();
    if sorted.is_empty() {
        return Vec::new();
    }

    // 1. Calculate max copy count per source for the current slice of builds
    let mut max_copy_per_source: std::collections::HashMap<&str, u64> = std::collections::HashMap::new();
    for b in &sorted {
        let src = b.source_name();
        let entry = max_copy_per_source.entry(src).or_insert(0);
        if b.copy_count > *entry {
            *entry = b.copy_count;
        }
    }

    // 2. Stable sort:
    //    Tier:
    //      -1 = Pinned / Favorite / Custom build
    //       0 = Meta build
    //       1 = Recommended build
    //       2 = Normal build
    //    Normalized score (descending) -> Absolute copy_count (descending)
    sorted.sort_by(|a, b| {
        let is_fav_a = favorite_codes.contains(&a.loadout_code) || a.is_custom || a.is_favorite;
        let is_fav_b = favorite_codes.contains(&b.loadout_code) || b.is_custom || b.is_favorite;

        let tier_a = if is_fav_a {
            -1
        } else {
            let text_a = format!("{} {}", a.classification, a.tags.join(" ")).to_lowercase();
            let is_meta_a = text_a.replace("fpsmeta", "").contains("meta");
            let is_rec_a = text_a.contains("đề xuất") || text_a.contains("recommend");
            if is_meta_a { 0 } else if is_rec_a { 1 } else { 2 }
        };

        let tier_b = if is_fav_b {
            -1
        } else {
            let text_b = format!("{} {}", b.classification, b.tags.join(" ")).to_lowercase();
            let is_meta_b = text_b.replace("fpsmeta", "").contains("meta");
            let is_rec_b = text_b.contains("đề xuất") || text_b.contains("recommend");
            if is_meta_b { 0 } else if is_rec_b { 1 } else { 2 }
        };

        if tier_a != tier_b {
            return tier_a.cmp(&tier_b);
        }

        let src_a = a.source_name();
        let max_a = *max_copy_per_source.get(src_a).unwrap_or(&0);
        let score_a = if max_a > 0 {
            a.copy_count as f64 / max_a as f64
        } else {
            0.65 // Balanced baseline for sources without copy statistics (DFBuilds, FPSMeta)
        };

        let src_b = b.source_name();
        let max_b = *max_copy_per_source.get(src_b).unwrap_or(&0);
        let score_b = if max_b > 0 {
            b.copy_count as f64 / max_b as f64
        } else {
            0.65
        };

        // Descending by normalized score (scaled to integer for robust float comparison)
        let score_a_scaled = (score_a * 10000.0).round() as i64;
        let score_b_scaled = (score_b * 10000.0).round() as i64;
        if score_a_scaled != score_b_scaled {
            return score_b_scaled.cmp(&score_a_scaled);
        }

        // Secondary tie-breaker: absolute copy_count descending
        b.copy_count.cmp(&a.copy_count)
    });

    sorted.into_iter().cloned().collect()
}

fn start_hotkey_worker(
    tx: Sender<WorkerEvent>,
    ctx: egui::Context,
    shared_builds: Arc<RwLock<Vec<BuildItem>>>,
    shared_unique_weapons: Arc<RwLock<Vec<String>>>,
    active_filters: Arc<RwLock<ActiveFilters>>,
    shared_hotkey_vk: Arc<AtomicI32>,
    shared_is_rebinding: Arc<AtomicBool>,
    shared_favorites: Arc<RwLock<Vec<BuildItem>>>,
) {
    thread::spawn(move || {
        let mut ocr_engine = match PaddleOcrEngine::new() {
            Ok(e) => e,
            Err(err) => {
                let _ = tx.send(WorkerEvent::OcrUnavailable(err));
                ctx.request_repaint();
                return;
            }
        };

        let mut clipboard = init_clipboard_with_retry(3).ok();
        let mut last_scan: Option<Instant> = None;
        let mut was_pressed = false;

        loop {
            thread::sleep(Duration::from_millis(25));

            // Tạm dừng kích hoạt quét súng nếu đang ở chế độ gán lại phím (Rebinding)
            if shared_is_rebinding.load(Ordering::Relaxed) {
                was_pressed = false;
                continue;
            }

            let current_vk = shared_hotkey_vk.load(Ordering::Relaxed);
            let is_pressed = unsafe { (GetAsyncKeyState(current_vk) as u16 & 0x8000) != 0 };

            if is_pressed && !was_pressed {
                let now = Instant::now();
                let is_cooling_down = match last_scan {
                    Some(prev) => now.duration_since(prev) < Duration::from_millis(2500),
                    None => false,
                };

                if !is_cooling_down {
                    last_scan = Some(now);

                    let candidates = shared_unique_weapons.read().unwrap().clone();
                    let (cur_server, cur_mode, cur_source, cur_monitor_id) = {
                        let f = active_filters.read().unwrap();
                        (f.server, f.mode, f.source, f.selected_monitor_id.clone())
                    };

                    match capture_and_recognize_weapon(&mut ocr_engine, &candidates, cur_monitor_id.as_deref()) {
                        Ok(outcome) => {
                            let Some(matched_weapon) = outcome.weapon.filter(|_| outcome.score >= 0.75) else {
                                unsafe {
                                    let _ = MessageBeep(MB_ICONERROR);
                                }
                                let _ = tx.send(WorkerEvent::ScanFailed {
                                    raw_ocr: outcome.raw_ocr,
                                    cleaned: outcome.cleaned,
                                });
                                ctx.request_repaint();
                                was_pressed = is_pressed;
                                continue;
                            };
                            let score = outcome.score;
                            let can_auto_copy = outcome.can_auto_copy;
                            let raw_ocr = outcome.raw_ocr;

                            let all_builds = shared_builds.read().unwrap().clone();
                            let matching: Vec<&BuildItem> = all_builds
                                .iter()
                                .filter(|b| {
                                    if !b.weapon.eq_ignore_ascii_case(matched_weapon) {
                                        return false;
                                    }
                                    if !cur_source.matches(b) {
                                        return false;
                                    }
                                    if !b.server.is_empty() && !b.server.eq_ignore_ascii_case(cur_server.as_str()) {
                                        return false;
                                    }
                                    if cur_mode != ModeFilter::All && !b.mode.is_empty() && !b.mode.eq_ignore_ascii_case(cur_mode.as_str()) {
                                        return false;
                                    }
                                    true
                                })
                                .collect();

                            let (fav_builds, fav_codes) = {
                                let f_guard = shared_favorites.read().unwrap();
                                let codes: HashSet<String> = f_guard.iter().map(|f| f.loadout_code.clone()).collect();
                                let customs: Vec<BuildItem> = f_guard.iter().filter(|f| f.is_custom).cloned().collect();
                                (customs, codes)
                            };

                            let matching = if matching.is_empty() {
                                all_builds
                                    .iter()
                                    .filter(|b| {
                                        b.weapon.eq_ignore_ascii_case(matched_weapon)
                                            && (b.server.is_empty() || b.server.eq_ignore_ascii_case(cur_server.as_str()))
                                            && cur_source.matches(b)
                                    })
                                    .collect()
                            } else {
                                matching
                            };

                            let mut matching = if matching.is_empty() {
                                all_builds
                                    .iter()
                                    .filter(|b| {
                                        b.weapon.eq_ignore_ascii_case(matched_weapon)
                                            && (b.server.is_empty() || b.server.eq_ignore_ascii_case(cur_server.as_str()))
                                    })
                                    .collect()
                            } else {
                                matching
                            };

                            for c in &fav_builds {
                                if c.weapon.eq_ignore_ascii_case(matched_weapon) {
                                    matching.push(c);
                                }
                            }

                            let sorted = sort_builds_prioritized(&matching, &fav_codes);
                            let selected: Vec<BuildItem> = sorted.into_iter().take(5).collect();

                            if can_auto_copy {
                                // Đạt cả 3 rào chắn Auto-Copy >= 75%: Category Lock + Margin >= 12% + Code >= 2 ký tự & CTC >= 0.55
                                let mut clipboard_error: Option<String> = None;
                                let mut clipboard_report: Option<(usize, usize)> = None;
                                if !selected.is_empty() {
                                    let codes: Vec<String> =
                                        selected.iter().map(|b| b.loadout_code.clone()).collect();
                                    match copy_codes_sequence_to_clipboard(&mut clipboard, &codes, 150) {
                                        Ok(report) => {
                                            clipboard_report =
                                                Some((report.success_count, report.total_count));
                                            unsafe {
                                                let _ = if report.success_count == report.total_count {
                                                    MessageBeep(MB_OK)
                                                } else {
                                                    MessageBeep(MB_ICONWARNING)
                                                };
                                            }
                                        }
                                        Err(err) => {
                                            unsafe {
                                                let _ = MessageBeep(MB_ICONERROR);
                                            }
                                            clipboard_error = Some(err);
                                        }
                                    }
                                } else {
                                    unsafe {
                                        let _ = MessageBeep(MB_ICONWARNING);
                                    }
                                }

                                let _ = tx.send(WorkerEvent::WeaponScanned {
                                    weapon: matched_weapon.to_string(),
                                    score,
                                    raw_ocr: raw_ocr.clone(),
                                    builds: selected,
                                    is_uncertain: false,
                                    clipboard_error,
                                    clipboard_report,
                                });
                            } else {
                                // Thiếu một trong 3 rào chắn an toàn Auto-Copy: KHÔNG ghi đè Clipboard, yêu cầu người dùng xác nhận
                                unsafe {
                                    let _ = MessageBeep(MB_ICONWARNING);
                                }

                                let _ = tx.send(WorkerEvent::WeaponScanned {
                                    weapon: matched_weapon.to_string(),
                                    score,
                                    raw_ocr: raw_ocr.clone(),
                                    builds: selected,
                                    is_uncertain: true,
                                    clipboard_error: None,
                                    clipboard_report: None,
                                });
                            }
                            ctx.request_repaint();
                        }
                        Err(_) => {
                            unsafe {
                                let _ = MessageBeep(MB_ICONERROR);
                            }
                            let _ = tx.send(WorkerEvent::ScanFailed {
                                raw_ocr: "Lỗi chụp màn hình hoặc OCR".to_string(),
                                cleaned: String::new(),
                            });
                            ctx.request_repaint();
                        }
                    }
                }
            }
            was_pressed = is_pressed;
        }
    });
}

struct LoadoutTerminalApp {
    builds: Vec<BuildItem>,
    unique_weapons: Vec<String>,
    all_weapon_types: Vec<String>,
    history: Vec<HistoryItem>,

    // Tabs & Navigation
    current_tab: AppTab,

    // Favorites & Custom Codes (favorites.json)
    favorites: Vec<BuildItem>,
    shared_favorites: Arc<RwLock<Vec<BuildItem>>>,
    show_add_custom_modal: bool,
    custom_weapon: String,
    custom_mode: String,
    custom_desc: String,
    custom_code: String,
    custom_error: Option<String>,

    // OCR Confidence Confirmation
    pending_confirm_builds: Option<(String, Vec<BuildItem>)>,
    pending_confirm_score: Option<f64>,

    // Dynamic database update
    shared_builds: Arc<RwLock<Vec<BuildItem>>>,
    shared_unique_weapons: Arc<RwLock<Vec<String>>>,
    is_updating: bool,
    last_update_success: Option<Instant>,
    tx_db_update: Sender<DbUpdateResult>,
    rx_db_update: Receiver<DbUpdateResult>,

    // Filter states
    selected_source: SourceFilter,
    selected_server: ServerFilter,
    selected_mode: ModeFilter,
    selected_type: String, // "" for All
    selected_weapon: Option<String>,
    search_query: String,
    selected_monitor_id: Option<String>,
    available_monitors: Vec<(String, String)>,

    active_filters: Arc<RwLock<ActiveFilters>>,

    always_on_top: bool,
    show_about: bool,
    show_guide: bool,
    show_support: bool,
    lang: AppLang,
    ocr_available: bool,

    // Support / Donate (VietQR offline) state
    support_amount: u64,
    support_custom_amount_text: String,
    support_qr_texture: Option<egui::TextureHandle>,

    // Hotkey configuration & dynamic rebinding
    hotkey_vk: i32,
    hotkey_name: String,
    shared_hotkey_vk: Arc<AtomicI32>,
    shared_is_rebinding: Arc<AtomicBool>,
    is_rebinding_hotkey: bool,
    rebind_start_time: Option<Instant>,

    auto_check_update: bool,
    available_update: Option<ReleaseUpdateInfo>,
    is_checking_update: bool,
    is_downloading_update: bool,
    is_applying_update: bool,
    downloaded_update_path: Option<PathBuf>,
    update_banner_dismissed: bool,
    up_to_date_banner_until: Option<Instant>,
    just_updated_version: Option<String>,

    egui_ctx: egui::Context,
    tx_events: Sender<WorkerEvent>,
    rx: Receiver<WorkerEvent>,
    status_message: Option<(String, Instant, bool)>,
}

impl LoadoutTerminalApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut fonts = egui::FontDefinitions::default();
        let font_candidates = [
            r"C:\Windows\Fonts\segoeui.ttf",
            r"C:\Windows\Fonts\arial.ttf",
            r"C:\Windows\Fonts\tahoma.ttf",
        ];

        for path in font_candidates {
            if let Ok(data) = fs::read(path) {
                fonts.font_data.insert("windows_system_font".to_owned(), egui::FontData::from_owned(data));
                fonts
                    .families
                    .entry(egui::FontFamily::Proportional)
                    .or_default()
                    .insert(0, "windows_system_font".to_owned());
                fonts
                    .families
                    .entry(egui::FontFamily::Monospace)
                    .or_default()
                    .push("windows_system_font".to_owned());
                break;
            }
        }
        cc.egui_ctx.set_fonts(fonts);

        let mut visuals = egui::Visuals::dark();
        visuals.override_text_color = Some(COLOR_TEXT_MAIN);
        visuals.panel_fill = COLOR_BG;
        visuals.window_fill = COLOR_BG;

        visuals.widgets.noninteractive.bg_fill = COLOR_CARD;
        visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, COLOR_BORDER);
        visuals.widgets.noninteractive.rounding = Rounding::same(3.0);

        visuals.widgets.inactive.bg_fill = COLOR_CARD;
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, COLOR_BORDER);
        visuals.widgets.inactive.rounding = Rounding::same(3.0);

        visuals.widgets.hovered.bg_fill = COLOR_CARD_HOVER;
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, COLOR_ACCENT);
        visuals.widgets.hovered.rounding = Rounding::same(3.0);

        visuals.widgets.active.bg_fill = COLOR_ACCENT_DIM;
        visuals.widgets.active.bg_stroke = Stroke::new(1.0_f32, COLOR_ACCENT);
        visuals.widgets.active.rounding = Rounding::same(3.0);

        cc.egui_ctx.set_visuals(visuals);

        let (builds, is_from_local, was_corrupted) = load_builds_data();
        let unique_weapons: Vec<String> = {
            let mut set = HashSet::new();
            for b in &builds {
                set.insert(b.weapon.clone());
            }
            let mut list: Vec<String> = set.into_iter().collect();
            list.sort();
            list
        };

        let all_weapon_types = vec![
            "Assault Rifle".to_string(),
            "SMG".to_string(),
            "LMG".to_string(),
            "Marksman Rifle".to_string(),
            "Sniper Rifle".to_string(),
            "Shotgun".to_string(),
            "Pistol".to_string(),
            "Special".to_string(),
        ];

        let history = load_history();
        let config = load_user_config();

        let selected_server = match config.server.to_lowercase().as_str() {
            "infinity" => ServerFilter::Infinity,
            _ => ServerFilter::Garena,
        };

        let selected_mode = match config.mode.to_lowercase().as_str() {
            "all" => ModeFilter::All,
            "operations" => ModeFilter::Operations,
            _ => ModeFilter::Warfare,
        };

        let selected_source = SourceFilter::from_str(&config.source);

        let lang = match config.lang.to_lowercase().as_str() {
            "en" => AppLang::En,
            _ => AppLang::Vi,
        };

        let ocr_init_res = check_ocr_available();
        let ocr_available = ocr_init_res.is_ok();
        let available_monitors = enumerate_monitors(lang);
        let selected_monitor_id = config.selected_monitor_id.clone();

        let active_filters = Arc::new(RwLock::new(ActiveFilters {
            server: selected_server,
            mode: selected_mode,
            source: selected_source,
            selected_monitor_id: selected_monitor_id.clone(),
        }));

        let shared_builds = Arc::new(RwLock::new(builds.clone()));
        let shared_unique_weapons = Arc::new(RwLock::new(unique_weapons.clone()));

        let hotkey_vk = config.hotkey_vk;
        let hotkey_name = config.hotkey_name;
        let shared_hotkey_vk = Arc::new(AtomicI32::new(hotkey_vk));
        let shared_is_rebinding = Arc::new(AtomicBool::new(false));

        let favorites = load_favorites();
        let shared_favorites = Arc::new(RwLock::new(favorites.clone()));

        let (tx, rx) = channel();
        start_hotkey_worker(
            tx.clone(),
            cc.egui_ctx.clone(),
            shared_builds.clone(),
            shared_unique_weapons.clone(),
            active_filters.clone(),
            shared_hotkey_vk.clone(),
            shared_is_rebinding.clone(),
            shared_favorites.clone(),
        );

        let auto_check_update = config.auto_check_update;
        if auto_check_update {
            spawn_update_check(tx.clone(), cc.egui_ctx.clone(), false);
        }

        let (tx_db_update, rx_db_update) = channel();

        let default_weapon = determine_startup_weapon(&unique_weapons, &builds, selected_server, selected_mode);

        let just_updated_version = consume_just_updated_flag_at(&just_updated_flag_path());
        if just_updated_version.is_some() {
            unsafe {
                let _ = MessageBeep(MB_OK);
            }
        }

        let (initial_status, is_err) = if let Some(ref updated_ver) = just_updated_version {
            let msg = match lang {
                AppLang::Vi => format!("✔ Đã cập nhật thành công lên phiên bản {}! Sẵn sàng với {} cấu hình.", updated_ver, builds.len()),
                AppLang::En => format!("✔ Successfully updated to version {}! Ready with {} builds.", updated_ver, builds.len()),
            };
            (msg, false)
        } else if let Err(ref ocr_err) = ocr_init_res {
            let msg = match lang {
                AppLang::Vi => format!(
                    "CẢNH BÁO: Chưa nạp được lõi PaddleOCR v4 ({})! Phím {} bị tắt. Hãy đảm bảo tệp onnxruntime.dll nằm cạnh .exe. Bạn vẫn có thể tra cứu thủ công.",
                    ocr_err, hotkey_name
                ),
                AppLang::En => format!(
                    "WARNING: Failed to initialize PaddleOCR v4 ({})! {} is disabled. Ensure onnxruntime.dll sits next to the .exe. Manual lookup is still available.",
                    ocr_err, hotkey_name
                ),
            };
            (msg, true)
        } else if was_corrupted {
            let msg = match lang {
                AppLang::Vi => format!("File builds_local.json bị lỗi/rỗng nên đã tự xóa. Đã tự khôi phục {} cấu hình gốc. Nhấn [{}] để quét.", builds.len(), hotkey_name),
                AppLang::En => format!("Corrupted builds_local.json was removed and recovered {} embedded builds. Press [{}] to scan.", builds.len(), hotkey_name),
            };
            (msg, false)
        } else if is_from_local {
            let msg = match lang {
                AppLang::Vi => format!("Đã nạp {} cấu hình từ file builds_local.json. Nhấn [{}] để quét.", builds.len(), hotkey_name),
                AppLang::En => format!("Loaded {} builds from builds_local.json. Press [{}] to scan.", builds.len(), hotkey_name),
            };
            (msg, false)
        } else {
            let msg = match lang {
                AppLang::Vi => format!("Sẵn sàng với {} cấu hình nhúng sẵn. Nhấn [{}] để quét.", builds.len(), hotkey_name),
                AppLang::En => format!("Ready with {} embedded builds. Press [{}] to scan.", builds.len(), hotkey_name),
            };
            (msg, false)
        };

        let initial_weapon = default_weapon.clone().unwrap_or_else(|| "M4A1".to_string());

        Self {
            builds,
            unique_weapons,
            all_weapon_types,
            history,
            favorites,
            shared_favorites,
            current_tab: AppTab::Scanner,
            show_add_custom_modal: false,
            custom_weapon: initial_weapon,
            custom_mode: "Warfare".to_string(),
            custom_desc: String::new(),
            custom_code: String::new(),
            custom_error: None,
            pending_confirm_builds: None,
            pending_confirm_score: None,
            shared_builds,
            shared_unique_weapons,
            is_updating: false,
            last_update_success: None,
            tx_db_update,
            rx_db_update,
            selected_source,
            selected_server,
            selected_mode,
            selected_type: String::new(),
            selected_weapon: default_weapon,
            search_query: String::new(),
            selected_monitor_id,
            available_monitors,
            active_filters,
            always_on_top: false,
            show_about: false,
            show_guide: true,
            show_support: false,
            lang,
            ocr_available,
            support_amount: 10000,
            support_custom_amount_text: "10000".to_string(),
            support_qr_texture: None,
            hotkey_vk,
            hotkey_name,
            shared_hotkey_vk,
            shared_is_rebinding,
            is_rebinding_hotkey: false,
            rebind_start_time: None,
            auto_check_update,
            available_update: None,
            is_checking_update: auto_check_update,
            is_downloading_update: false,
            is_applying_update: false,
            downloaded_update_path: None,
            update_banner_dismissed: false,
            up_to_date_banner_until: None,
            just_updated_version,
            egui_ctx: cc.egui_ctx.clone(),
            tx_events: tx,
            rx,
            status_message: Some((initial_status, Instant::now(), is_err)),
        }
    }

    fn favorite_codes_set(&self) -> HashSet<String> {
        self.favorites.iter().map(|f| f.loadout_code.clone()).collect()
    }

    fn is_favorite(&self, code: &str) -> bool {
        self.favorites.iter().any(|f| f.loadout_code == code)
    }

    fn sync_shared_favorites(&self) {
        if let Ok(mut set) = self.shared_favorites.write() {
            *set = self.favorites.clone();
        }
    }

    fn toggle_favorite(&mut self, build: &BuildItem) {
        let success_msg = if let Some(pos) = self.favorites.iter().position(|f| f.loadout_code == build.loadout_code) {
            self.favorites.remove(pos);
            match self.lang {
                AppLang::Vi => format!("Đã bỏ ghim cấu hình [{}]", build.weapon),
                AppLang::En => format!("Unpinned loadout for [{}]", build.weapon),
            }
        } else {
            let mut fav = build.clone();
            fav.is_favorite = true;
            self.favorites.insert(0, fav);
            match self.lang {
                AppLang::Vi => format!("Đã ghim cấu hình [{}] vào Yêu thích!", build.weapon),
                AppLang::En => format!("Pinned [{}] loadout to Favorites!", build.weapon),
            }
        };
        match save_favorites(&self.favorites) {
            Ok(()) => {
                self.status_message = Some((success_msg, Instant::now(), false));
            }
            Err(err) => {
                let msg = match self.lang {
                    AppLang::Vi => format!("Lỗi lưu Yêu thích (favorites.json): {}", err),
                    AppLang::En => format!("Failed to save Favorites (favorites.json): {}", err),
                };
                self.status_message = Some((msg, Instant::now(), true));
            }
        }
        self.sync_shared_favorites();
    }

    fn delete_custom_build(&mut self, code: &str) {
        let mut removed_weapon = String::new();
        if let Some(pos) = self.favorites.iter().position(|f| f.loadout_code == code) {
            removed_weapon = self.favorites[pos].weapon.clone();
            self.favorites.remove(pos);
        }
        match save_favorites(&self.favorites) {
            Ok(()) => {
                let msg = match self.lang {
                    AppLang::Vi => format!("Đã xóa mã cá nhân cho [{}]", removed_weapon),
                    AppLang::En => format!("Deleted custom loadout for [{}]", removed_weapon),
                };
                self.status_message = Some((msg, Instant::now(), false));
            }
            Err(err) => {
                let msg = match self.lang {
                    AppLang::Vi => format!("Lỗi cập nhật favorites.json: {}", err),
                    AppLang::En => format!("Failed to update favorites.json: {}", err),
                };
                self.status_message = Some((msg, Instant::now(), true));
            }
        }
        self.sync_shared_favorites();
    }

    fn save_current_config(&mut self) {
        let cfg = UserConfig {
            server: self.selected_server.as_str().to_string(),
            mode: self.selected_mode.as_str().to_string(),
            source: self.selected_source.as_str().to_string(),
            lang: match self.lang {
                AppLang::Vi => "vi".to_string(),
                AppLang::En => "en".to_string(),
            },
            hotkey_vk: self.hotkey_vk,
            hotkey_name: self.hotkey_name.clone(),
            selected_monitor_id: self.selected_monitor_id.clone(),
            auto_check_update: self.auto_check_update,
        };
        if let Err(err) = save_user_config(&cfg) {
            let msg = match self.lang {
                AppLang::Vi => format!("Lỗi lưu cấu hình (config.json): {}", err),
                AppLang::En => format!("Failed to save config (config.json): {}", err),
            };
            self.status_message = Some((msg, Instant::now(), true));
        }
    }

    fn sync_active_filters(&self) {
        if let Ok(mut f) = self.active_filters.write() {
            f.server = self.selected_server;
            f.mode = self.selected_mode;
            f.source = self.selected_source;
            f.selected_monitor_id = self.selected_monitor_id.clone();
        }
    }

    fn record_history_entry(&mut self, weapon: &str, classification: &str, copy_count: u64, code: &str) {
        let item = HistoryItem {
            timestamp: get_current_time_str(),
            weapon: weapon.to_string(),
            classification: classification.to_string(),
            copy_count,
            loadout_code: code.to_string(),
        };
        self.history.retain(|h| h.loadout_code != code);
        self.history.insert(0, item);
        if self.history.len() > 50 {
            self.history.truncate(50);
        }
        if let Err(err) = save_history(&self.history) {
            let msg = match self.lang {
                AppLang::Vi => format!("Lỗi lưu lịch sử (df_history.json): {}", err),
                AppLang::En => format!("Failed to save history (df_history.json): {}", err),
            };
            self.status_message = Some((msg, Instant::now(), true));
        }
    }

    fn get_available_weapons(&self) -> Vec<String> {
        let mut set = HashSet::new();

        let mut check_weapon = |b: &BuildItem| {
            if !self.selected_source.matches(b) {
                return;
            }
            if !b.server.is_empty() && !b.server.eq_ignore_ascii_case(self.selected_server.as_str()) {
                return;
            }
            if self.selected_mode != ModeFilter::All && !b.mode.is_empty() && !b.mode.eq_ignore_ascii_case(self.selected_mode.as_str()) {
                return;
            }
            if !self.selected_type.is_empty() && !b.weapon_type.eq_ignore_ascii_case(&self.selected_type) {
                return;
            }
            set.insert(b.weapon.clone());
        };

        for b in &self.builds {
            check_weapon(b);
        }
        for f in &self.favorites {
            if f.is_custom {
                check_weapon(f);
            }
        }

        if set.is_empty() {
            for b in &self.builds {
                if !b.server.is_empty() && !b.server.eq_ignore_ascii_case(self.selected_server.as_str()) {
                    continue;
                }
                if self.selected_mode != ModeFilter::All && !b.mode.is_empty() && !b.mode.eq_ignore_ascii_case(self.selected_mode.as_str()) {
                    continue;
                }
                if !self.selected_type.is_empty() && !b.weapon_type.eq_ignore_ascii_case(&self.selected_type) {
                    continue;
                }
                set.insert(b.weapon.clone());
            }
        }
        let mut list: Vec<String> = set.into_iter().collect();
        list.sort();
        list
    }

    fn get_filtered_builds(&self, weapon: &str) -> Vec<BuildItem> {
        let fav_codes = self.favorite_codes_set();

        let mut all_candidates: Vec<&BuildItem> = self.builds.iter().collect();
        for f in &self.favorites {
            if f.is_custom && !all_candidates.iter().any(|b| b.loadout_code == f.loadout_code) {
                all_candidates.push(f);
            }
        }

        let matching: Vec<&BuildItem> = all_candidates
            .iter()
            .copied()
            .filter(|b| {
                if !b.weapon.eq_ignore_ascii_case(weapon) {
                    return false;
                }
                if !self.selected_source.matches(b) {
                    return false;
                }
                if !b.server.is_empty() && !b.server.eq_ignore_ascii_case(self.selected_server.as_str()) {
                    return false;
                }
                if self.selected_mode != ModeFilter::All && !b.mode.is_empty() && !b.mode.eq_ignore_ascii_case(self.selected_mode.as_str()) {
                    return false;
                }
                true
            })
            .collect();

        if matching.is_empty() {
            let fallback: Vec<&BuildItem> = self
                .builds
                .iter()
                .filter(|b| {
                    b.weapon.eq_ignore_ascii_case(weapon)
                        && (b.server.is_empty() || b.server.eq_ignore_ascii_case(self.selected_server.as_str()))
                        && self.selected_source.matches(b)
                })
                .collect();
            if fallback.is_empty() {
                let global_fallback: Vec<&BuildItem> = self
                    .builds
                    .iter()
                    .filter(|b| {
                        b.weapon.eq_ignore_ascii_case(weapon)
                            && (b.server.is_empty() || b.server.eq_ignore_ascii_case(self.selected_server.as_str()))
                    })
                    .collect();
                sort_builds_prioritized(&global_fallback, &fav_codes)
            } else {
                sort_builds_prioritized(&fallback, &fav_codes)
            }
        } else {
            sort_builds_prioritized(&matching, &fav_codes)
        }
    }

    fn copy_build_code(&mut self, weapon: &str, classification: &str, copy_count: u64, code: &str) {
        let copy_res = match init_clipboard_with_retry(3) {
            Ok(mut cb) => copy_to_clipboard_with_retry(&mut cb, code, 3),
            Err(e) => Err(e),
        };

        match copy_res {
            Ok(()) => {
                unsafe {
                    let _ = MessageBeep(MB_OK);
                }
                let display_class = format_display_classification(classification, self.lang);
                let msg = match self.lang {
                    AppLang::Vi => format!("Đã sao chép mã {} ({}) vào Clipboard!", weapon, display_class),
                    AppLang::En => format!("Copied loadout code for {} ({}) to Clipboard!", weapon, display_class),
                };
                self.status_message = Some((msg, Instant::now(), false));
                self.record_history_entry(weapon, classification, copy_count, code);
            }
            Err(err) => {
                unsafe {
                    let _ = MessageBeep(MB_ICONERROR);
                }
                let msg = match self.lang {
                    AppLang::Vi => format!("Lỗi ghi Clipboard khi sao chép [{}]: {}", weapon, err),
                    AppLang::En => format!("Failed to write [{}] loadout to Clipboard: {}", weapon, err),
                };
                self.status_message = Some((msg, Instant::now(), true));
            }
        }
    }

    fn spawn_win_v_copy_worker(
        &mut self,
        selected: Vec<BuildItem>,
        subject_label: String,
        err_prefix: String,
    ) {
        let tx = self.tx_events.clone();
        let ctx = self.egui_ctx.clone();
        let lang = self.lang;

        thread::spawn(move || {
            let codes: Vec<String> = selected.iter().map(|b| b.loadout_code.clone()).collect();
            let mut cb_opt = init_clipboard_with_retry(3).ok();
            match copy_codes_sequence_to_clipboard(&mut cb_opt, &codes, 150) {
                Ok(report) => {
                    let all_ok = report.success_count == report.total_count;
                    unsafe {
                        let _ = if all_ok {
                            MessageBeep(MB_OK)
                        } else {
                            MessageBeep(MB_ICONWARNING)
                        };
                    }
                    let msg = if all_ok {
                        match lang {
                            AppLang::Vi => format!(
                                "Đã đẩy {}/{} cấu hình {} vào Win + V (Meta ở đầu)!",
                                report.success_count, report.total_count, subject_label
                            ),
                            AppLang::En => format!(
                                "Pushed {}/{} {} builds to Win + V (Meta at top)!",
                                report.success_count, report.total_count, subject_label
                            ),
                        }
                    } else {
                        match lang {
                            AppLang::Vi => format!(
                                "Cảnh báo: Chỉ nạp được {}/{} cấu hình {} vào Win + V (mã đầu đã sẵn sàng Ctrl + V)!",
                                report.success_count, report.total_count, subject_label
                            ),
                            AppLang::En => format!(
                                "Warning: Only pushed {}/{} {} builds to Win + V (top build ready on Ctrl + V)!",
                                report.success_count, report.total_count, subject_label
                            ),
                        }
                    };
                    let _ = tx.send(WorkerEvent::WinVCopyCompleted {
                        top_build: selected.into_iter().next(),
                        msg,
                        is_error: !all_ok,
                    });
                }
                Err(err) => {
                    unsafe {
                        let _ = MessageBeep(MB_ICONERROR);
                    }
                    let _ = tx.send(WorkerEvent::WinVCopyCompleted {
                        top_build: None,
                        msg: format!("{}: {}", err_prefix, err),
                        is_error: true,
                    });
                }
            }
            ctx.request_repaint();
        });
    }

    fn copy_all_builds_to_win_v(&mut self, weapon: &str, sorted_builds: &[BuildItem]) {
        let selected: Vec<BuildItem> = sorted_builds.iter().take(5).cloned().collect();
        if selected.is_empty() {
            return;
        }
        let weapon_name = weapon.to_string();
        let subject_label = match self.lang {
            AppLang::Vi => format!("của {}", weapon_name),
            AppLang::En => format!("for {}", weapon_name),
        };
        let err_prefix = match self.lang {
            AppLang::Vi => format!("Lỗi ghi Clipboard cho {}", weapon_name),
            AppLang::En => format!("Clipboard write error for {}", weapon_name),
        };
        self.spawn_win_v_copy_worker(selected, subject_label, err_prefix);
    }

    fn copy_all_favorites_to_win_v(&mut self) {
        let selected: Vec<BuildItem> = self.favorites.iter().take(5).cloned().collect();
        if selected.is_empty() {
            return;
        }
        let subject_label = self.lang.t("yêu thích", "favorite").to_string();
        let err_prefix = self
            .lang
            .t("Lỗi ghi Clipboard cho Yêu thích", "Clipboard write error for Favorites")
            .to_string();
        self.spawn_win_v_copy_worker(selected, subject_label, err_prefix);
    }

    fn trigger_db_update(&mut self, ctx: egui::Context) {
        if self.is_updating {
            return;
        }
        self.is_updating = true;

        let msg = match self.lang {
            AppLang::Vi => "Đang tải dữ liệu cấu hình mùa mới từ máy chủ...",
            AppLang::En => "Downloading latest season database from server...",
        };
        self.status_message = Some((msg.to_string(), Instant::now(), false));

        let tx = self.tx_db_update.clone();

        thread::spawn(move || {
            match ureq::get(OFFICIAL_UPDATE_URL).timeout(Duration::from_secs(15)).call() {
                Ok(response) => {
                    match response.into_string() {
                        Ok(body) => {
                            match serde_json::from_str::<Vec<BuildItem>>(&body) {
                                Ok(raw_items) => match validate_downloaded_builds(raw_items) {
                                    Ok(valid_items) => {
                                        let count = valid_items.len();
                                        let local_path = local_builds_file_path();
                                        let serialized = match serde_json::to_string(&valid_items) {
                                            Ok(s) => s,
                                            Err(_) => body,
                                        };
                                        match atomic_write_verified(&local_path, &serialized) {
                                            Ok(()) => {
                                                let _ = tx.send(DbUpdateResult::Success {
                                                    builds: valid_items,
                                                    count,
                                                });
                                            }
                                            Err(err) => {
                                                let _ = tx.send(DbUpdateResult::Error(format!(
                                                    "Lỗi lưu database xuống ổ đĩa (giữ nguyên dữ liệu cũ): {}",
                                                    err
                                                )));
                                            }
                                        }
                                    }
                                    Err(reason) => {
                                        let _ = tx.send(DbUpdateResult::Error(reason));
                                    }
                                },
                                Err(e) => {
                                    let _ = tx.send(DbUpdateResult::Error(format!("Lỗi phân tích JSON: {}", e)));
                                }
                            }
                        }
                        Err(e) => {
                            let _ = tx.send(DbUpdateResult::Error(format!("Lỗi đọc phản hồi: {}", e)));
                        }
                    }
                }
                Err(e) => {
                    let _ = tx.send(DbUpdateResult::Error(format!("Lỗi kết nối mạng: {}", e)));
                }
            }
            ctx.request_repaint();
        });
    }

    fn update_support_qr(&mut self, ctx: &egui::Context) {
        if let Some(color_img) = generate_vietqr_image(self.support_amount, "Ung ho DF Loadout Scanner") {
            self.support_qr_texture = Some(ctx.load_texture(
                "vietqr_texture",
                color_img,
                egui::TextureOptions::NEAREST,
            ));
        }
    }
}

impl eframe::App for LoadoutTerminalApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Check window active state (focused and not minimized)
        let is_focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));
        let is_minimized = ctx.input(|i| i.viewport().minimized.unwrap_or(false));
        let is_window_active = is_focused && !is_minimized;

        // Handle incoming F9 scan events and version update notifications
        while let Ok(event) = self.rx.try_recv() {
            match event {
                WorkerEvent::WeaponScanned {
                    weapon,
                    score,
                    raw_ocr,
                    builds,
                    is_uncertain,
                    clipboard_error,
                    clipboard_report,
                } => {
                    self.selected_weapon = Some(weapon.clone());
                    self.current_tab = AppTab::Scanner;
                    let s = self.selected_server.as_str();
                    let m = self.selected_mode.as_str();
                    let src = self.selected_source.as_str();
                    let is_empty = builds.is_empty();
                    let ocr_preview = truncate_ocr_preview(&raw_ocr, 24);

                    if is_uncertain {
                        self.pending_confirm_builds = Some((weapon.clone(), builds));
                        self.pending_confirm_score = Some(score);
                        let msg = match self.lang {
                            AppLang::Vi => format!(
                                "Nghi ngờ nhận diện: [{}] ({:.1}%) — Bấm [✔ XÁC NHẬN & COPY] (OCR: '{}')",
                                weapon,
                                score * 100.0,
                                ocr_preview
                            ),
                            AppLang::En => format!(
                                "Confirm match: [{}] ({:.1}%) — Click [✔ CONFIRM & COPY] (OCR: '{}')",
                                weapon,
                                score * 100.0,
                                ocr_preview
                            ),
                        };
                        self.status_message = Some((msg, Instant::now(), true));
                    } else if let Some(err) = clipboard_error {
                        self.pending_confirm_builds = None;
                        self.pending_confirm_score = None;
                        let msg = match self.lang {
                            AppLang::Vi => format!("Nhận diện [{}] nhưng ghi Clipboard thất bại: {}", weapon, err),
                            AppLang::En => format!("Recognized [{}] but Clipboard write failed: {}", weapon, err),
                        };
                        self.status_message = Some((msg, Instant::now(), true));
                    } else {
                        self.pending_confirm_builds = None;
                        self.pending_confirm_score = None;
                        if let Some(top_build) = builds.first() {
                            self.record_history_entry(
                                &weapon,
                                &top_build.classification,
                                top_build.copy_count,
                                &top_build.loadout_code,
                            );
                        }
                        let partial_clip = matches!(clipboard_report, Some((ok, tot)) if ok < tot);
                        let ratio_suffix = match clipboard_report {
                            Some((ok, tot)) if tot > 0 => format!(" [Win+V: {}/{}]", ok, tot),
                            _ => String::new(),
                        };
                        let msg = match self.lang {
                            AppLang::Vi => {
                                if is_empty {
                                    format!("Đã nhận diện súng [{}] ({:.1}%) nhưng chưa có cấu hình cho bộ lọc hiện tại [{}/{}/{}]", weapon, score * 100.0, src, s, m)
                                } else if partial_clip {
                                    format!("Nhận diện: {} ({:.1}%) — Cảnh báo chỉ nạp được{} - OCR: '{}'", weapon, score * 100.0, ratio_suffix, ocr_preview)
                                } else {
                                    format!("Nhận diện: {} ({:.1}%) [{}/{}/{}]{} - OCR: '{}'", weapon, score * 100.0, src, s, m, ratio_suffix, ocr_preview)
                                }
                            }
                            AppLang::En => {
                                if is_empty {
                                    format!("Recognized gun [{}] ({:.1}%) but no builds found for current filter [{}/{}/{}]", weapon, score * 100.0, src, s, m)
                                } else if partial_clip {
                                    format!("Recognized: {} ({:.1}%) — Warning partial clipboard{} - OCR: '{}'", weapon, score * 100.0, ratio_suffix, ocr_preview)
                                } else {
                                    format!("Recognized: {} ({:.1}%) [{}/{}/{}]{} - OCR: '{}'", weapon, score * 100.0, src, s, m, ratio_suffix, ocr_preview)
                                }
                            }
                        };
                        self.status_message = Some((msg, Instant::now(), is_empty || partial_clip));
                    }
                }
                WorkerEvent::ScanFailed { raw_ocr, cleaned } => {
                    self.pending_confirm_builds = None;
                    self.pending_confirm_score = None;
                    let ocr_preview = truncate_ocr_preview(&raw_ocr, 28);
                    let cleaned_preview = truncate_ocr_preview(&cleaned, 24);
                    let msg = match self.lang {
                        AppLang::Vi => format!("Không nhận diện được súng! (OCR: '{}' | Xử lý: '{}')", ocr_preview, cleaned_preview),
                        AppLang::En => format!("Weapon not recognized! (OCR: '{}' | Cleaned: '{}')", ocr_preview, cleaned_preview),
                    };
                    self.status_message = Some((msg, Instant::now(), true));
                }
                WorkerEvent::WinVCopyCompleted {
                    top_build,
                    msg,
                    is_error,
                } => {
                    if let Some(top) = top_build {
                        self.record_history_entry(
                            &top.weapon,
                            &top.classification,
                            top.copy_count,
                            &top.loadout_code,
                        );
                    }
                    self.status_message = Some((msg, Instant::now(), is_error));
                }
                WorkerEvent::OcrUnavailable(err) => {
                    self.ocr_available = false;
                    let msg = match self.lang {
                        AppLang::Vi => format!(
                            "CẢNH BÁO: Lỗi khởi tạo PaddleOCR v4 ({})! Phím {} bị tắt. Hãy đặt tệp onnxruntime.dll cạnh tệp .exe. Bạn vẫn có thể tra cứu thủ công.",
                            err, self.hotkey_name
                        ),
                        AppLang::En => format!(
                            "WARNING: Failed to initialize PaddleOCR v4 ({})! {} is disabled. Place onnxruntime.dll beside the .exe. Manual lookup is still available.",
                            err, self.hotkey_name
                        ),
                    };
                    self.status_message = Some((msg, Instant::now(), true));
                }
                WorkerEvent::UpdateCheckFinished { manual, result } => {
                    self.is_checking_update = false;
                    match result {
                        Ok(Some(info)) => {
                            let changed = self
                                .available_update
                                .as_ref()
                                .map_or(true, |prev| prev.tag_name != info.tag_name);
                            if changed {
                                self.downloaded_update_path = None;
                            }
                            self.up_to_date_banner_until = None;
                            self.update_banner_dismissed = false;
                            if manual {
                                unsafe {
                                    let _ = MessageBeep(MB_OK);
                                }
                            }
                            let msg = match self.lang {
                                AppLang::Vi => format!(
                                    "🚀 Có bản cập nhật mới {} (hiện tại: {}). Xem banner phía trên để mở hoặc tải gói .zip.",
                                    info.tag_name, APP_VERSION
                                ),
                                AppLang::En => format!(
                                    "🚀 New version {} available (current: {}). Use the top banner to open or download .zip.",
                                    info.tag_name, APP_VERSION
                                ),
                            };
                            self.available_update = Some(info);
                            self.status_message = Some((msg, Instant::now(), false));
                        }
                        Ok(None) => {
                            self.available_update = None;
                            if manual {
                                unsafe {
                                    let _ = MessageBeep(MB_OK);
                                }
                                self.up_to_date_banner_until = Some(Instant::now() + Duration::from_secs(8));
                                let msg = match self.lang {
                                    AppLang::Vi => format!("✔ Bạn đang sử dụng phiên bản mới nhất ({}).", APP_VERSION),
                                    AppLang::En => format!("✔ You are already on the latest version ({}).", APP_VERSION),
                                };
                                self.status_message = Some((msg, Instant::now(), false));
                            }
                        }
                        Err(err) => {
                            if manual {
                                unsafe {
                                    let _ = MessageBeep(MB_ICONERROR);
                                }
                            }
                            let msg = match self.lang {
                                AppLang::Vi => format!("Kiểm tra cập nhật thất bại: {}", err),
                                AppLang::En => format!("Update check failed: {}", err),
                            };
                            self.status_message = Some((msg, Instant::now(), true));
                        }
                    }
                }
                WorkerEvent::UpdateDownloadFinished { target_tag, result } => {
                    self.is_downloading_update = false;
                    match result {
                        Ok(path) => {
                            self.downloaded_update_path = Some(path.clone());
                            self.is_applying_update = true;
                            match launch_update_installer_script(&path, &target_tag) {
                                Ok(()) => {
                                    let msg = match self.lang {
                                        AppLang::Vi => format!(
                                            "⚡ Đã xác thực gói .zip! Đang đóng ứng dụng để cập nhật lên {}...",
                                            target_tag
                                        ),
                                        AppLang::En => format!(
                                            "⚡ Validated .zip package! Closing app to update to {}...",
                                            target_tag
                                        ),
                                    };
                                    self.status_message = Some((msg, Instant::now(), false));
                                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                                }
                                Err(err) => {
                                    self.is_applying_update = false;
                                    unsafe {
                                        let _ = MessageBeep(MB_ICONERROR);
                                    }
                                    let msg = match self.lang {
                                        AppLang::Vi => format!("Lỗi khởi chạy trình cập nhật: {}", err),
                                        AppLang::En => format!("Failed to launch updater: {}", err),
                                    };
                                    self.status_message = Some((msg, Instant::now(), true));
                                }
                            }
                        }
                        Err(err) => {
                            self.is_applying_update = false;
                            unsafe {
                                let _ = MessageBeep(MB_ICONERROR);
                            }
                            let msg = match self.lang {
                                AppLang::Vi => format!("Tải/kiểm tra gói cập nhật thất bại: {}", err),
                                AppLang::En => format!("Update download/validation failed: {}", err),
                            };
                            self.status_message = Some((msg, Instant::now(), true));
                        }
                    }
                }
            }
        }

        // If window is minimized to taskbar, skip UI rendering entirely to save CPU/GPU
        if is_minimized {
            return;
        }

        // Key Capture Handler for Rebinding Hotkey (only request repaint when window is active)
        if self.is_rebinding_hotkey {
            if is_window_active {
                ctx.request_repaint_after(Duration::from_millis(30));
            }
            let should_check = self.rebind_start_time.map_or(false, |t| t.elapsed() >= Duration::from_millis(150));
            if should_check {
                if let Some(res) = detect_pressed_hotkey() {
                    match res {
                        Some((vk, name)) => {
                            self.hotkey_vk = vk;
                            self.hotkey_name = name.to_string();
                            self.shared_hotkey_vk.store(vk, Ordering::Relaxed);
                            self.is_rebinding_hotkey = false;
                            self.shared_is_rebinding.store(false, Ordering::Relaxed);
                            self.save_current_config();

                            unsafe {
                                let _ = MessageBeep(MB_OK);
                            }

                            let msg = match self.lang {
                                AppLang::Vi => format!("Đã đổi phím tắt quét súng sang [{}]!", name),
                                AppLang::En => format!("Scan hotkey changed to [{}]!", name),
                            };
                            self.status_message = Some((msg, Instant::now(), false));
                        }
                        None => {
                            // Esc pressed -> cancel rebinding
                            self.is_rebinding_hotkey = false;
                            self.shared_is_rebinding.store(false, Ordering::Relaxed);
                            let msg = match self.lang {
                                AppLang::Vi => "Đã hủy thao tác đổi phím tắt.".to_string(),
                                AppLang::En => "Hotkey change cancelled.".to_string(),
                            };
                            self.status_message = Some((msg, Instant::now(), false));
                        }
                    }
                }
            }
        }

        // Handle incoming DB Update events
        while let Ok(res) = self.rx_db_update.try_recv() {
            self.is_updating = false;
            match res {
                DbUpdateResult::Success { builds, count } => {
                    self.last_update_success = Some(Instant::now());
                    let msg = match self.lang {
                        AppLang::Vi => format!("Cập nhật thành công! Đã nạp {} cấu hình mùa mới vào builds_local.json.", count),
                        AppLang::En => format!("Update successful! Loaded {} latest configurations into builds_local.json.", count),
                    };
                    self.status_message = Some((msg, Instant::now(), false));

                    self.unique_weapons = {
                        let mut set = HashSet::new();
                        for b in &builds {
                            set.insert(b.weapon.clone());
                        }
                        let mut list: Vec<String> = set.into_iter().collect();
                        list.sort();
                        list
                    };
                    self.builds = builds.clone();

                    if let Ok(mut sb) = self.shared_builds.write() {
                        *sb = builds;
                    }
                    if let Ok(mut su) = self.shared_unique_weapons.write() {
                        *su = self.unique_weapons.clone();
                    }
                }
                DbUpdateResult::Error(err) => {
                    let msg = match self.lang {
                        AppLang::Vi => format!("Cập nhật thất bại: {}", err),
                        AppLang::En => format!("Update failed: {}", err),
                    };
                    self.status_message = Some((msg, Instant::now(), true));
                }
            }
        }

        // Global ESC shortcut: closes modals first, clears search, or exits Favorites tab back to Scanner
        if !self.is_rebinding_hotkey && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            if self.show_add_custom_modal {
                self.show_add_custom_modal = false;
                self.custom_error = None;
            } else if self.show_guide {
                self.show_guide = false;
            } else if self.show_support {
                self.show_support = false;
            } else if self.show_about {
                self.show_about = false;
            } else if !self.search_query.is_empty() {
                self.search_query.clear();
            } else if self.current_tab == AppTab::Favorites {
                self.current_tab = AppTab::Scanner;
            }
        }

        // Top Header Panel
        egui::TopBottomPanel::top("header_panel")
            .frame(
                Frame::default()
                    .fill(COLOR_CARD)
                    .stroke(Stroke::new(1.0_f32, COLOR_BORDER))
                    .inner_margin(Margin {
                        left: 14.0,
                        right: 12.0,
                        top: 7.0,
                        bottom: 7.0,
                    }),
            )
            .show(ctx, |ui| {
                // Row 1: Logo + Primary Navigation Tabs (Left) | Utility Controls (Right)
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Label::new(
                            RichText::new(APP_NAME)
                                .color(COLOR_ACCENT)
                                .size(15.0)
                                .strong()
                                .monospace(),
                        )
                        .selectable(false),
                    );

                    ui.add_space(6.0);
                    ui.add(egui::Label::new(RichText::new("|").color(COLOR_BORDER).size(13.0)).selectable(false));
                    ui.add_space(6.0);

                    // Navigation Tabs: [🔍 QUÉT & TRA CỨU] and [★ YÊU THÍCH (N)]
                    let tab1_active = self.current_tab == AppTab::Scanner;
                    let tab1_label = self.lang.t("🔍 QUÉT & TRA CỨU", "🔍 SCAN & SEARCH");
                    let tab1_btn = egui::Button::new(
                        RichText::new(tab1_label)
                            .color(if tab1_active { COLOR_ACCENT } else { COLOR_TEXT_MUTED })
                            .size(11.0)
                            .strong(),
                    )
                    .fill(if tab1_active { COLOR_ACCENT_DIM } else { COLOR_CARD })
                    .stroke(Stroke::new(1.0_f32, if tab1_active { COLOR_ACCENT } else { COLOR_BORDER }))
                    .rounding(Rounding::same(3.0));

                    if ui.add(tab1_btn).on_hover_text(self.lang.t("Màn hình chính: Quét súng và tra cứu theo danh mục", "Main screen: Weapon scanner & catalog search")).clicked() {
                        self.current_tab = AppTab::Scanner;
                    }

                    ui.add_space(4.0);

                    let tab2_active = self.current_tab == AppTab::Favorites;
                    let fav_count = self.favorites.len();
                    let tab2_label = match self.lang {
                        AppLang::Vi => format!("★ YÊU THÍCH ({})", fav_count),
                        AppLang::En => format!("★ FAVORITES ({})", fav_count),
                    };
                    let tab2_btn = egui::Button::new(
                        RichText::new(tab2_label)
                            .color(if tab2_active { Color32::from_rgb(255, 215, 0) } else { if fav_count > 0 { Color32::from_rgb(220, 190, 80) } else { COLOR_TEXT_MUTED } })
                            .size(11.0)
                            .strong(),
                    )
                    .fill(if tab2_active { Color32::from_rgb(50, 42, 10) } else { COLOR_CARD })
                    .stroke(Stroke::new(1.0_f32, if tab2_active { Color32::from_rgb(255, 215, 0) } else { COLOR_BORDER }))
                    .rounding(Rounding::same(3.0));

                    if ui.add(tab2_btn).on_hover_text(self.lang.t("Kho lưu trữ: Toàn bộ cấu hình đã ghim và mã cá nhân", "Repository: All pinned builds and custom loadouts")).clicked() {
                        self.current_tab = AppTab::Favorites;
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 5.0;

                        let about_color = if self.show_about { COLOR_ACCENT } else { COLOR_TEXT_MUTED };
                        let about_btn = egui::Button::new(
                            RichText::new("[ABOUT]")
                                .color(about_color)
                                .size(11.0)
                                .strong(),
                        )
                        .stroke(Stroke::new(1.0_f32, if self.show_about { COLOR_ACCENT } else { COLOR_BORDER }))
                        .rounding(Rounding::same(3.0));

                        if ui.add(about_btn).on_hover_text(self.lang.t("Thông tin tác giả", "Author information")).clicked() {
                            self.show_about = !self.show_about;
                        }

                        let support_color = if self.show_support {
                            Color32::from_rgb(255, 105, 180)
                        } else {
                            Color32::from_rgb(255, 140, 180)
                        };
                        let support_btn = egui::Button::new(
                            RichText::new(self.lang.t("[❤ ỦNG HỘ]", "[❤ SUPPORT]"))
                                .color(support_color)
                                .size(11.0)
                                .strong(),
                        )
                        .stroke(Stroke::new(1.0_f32, if self.show_support { Color32::from_rgb(255, 105, 180) } else { Color32::from_rgb(180, 60, 100) }))
                        .rounding(Rounding::same(3.0));

                        if ui.add(support_btn).on_hover_text(self.lang.t("Ủng hộ tác giả (VietQR)", "Support developer (VietQR)")).clicked() {
                            self.show_support = !self.show_support;
                            if self.show_support && self.support_qr_texture.is_none() {
                                self.update_support_qr(ctx);
                            }
                        }

                        let top_icon = if self.always_on_top {
                            self.lang.t("[GHIM: BẬT]", "[PIN: ON]")
                        } else {
                            self.lang.t("[GHIM: TẮT]", "[PIN: OFF]")
                        };
                        let top_color = if self.always_on_top { COLOR_ACCENT } else { COLOR_TEXT_MUTED };
                        let top_btn = egui::Button::new(
                            RichText::new(top_icon)
                                .color(top_color)
                                .size(11.0)
                                .strong(),
                        )
                        .stroke(Stroke::new(1.0_f32, if self.always_on_top { COLOR_ACCENT } else { COLOR_BORDER }))
                        .rounding(Rounding::same(3.0));

                        if ui.add(top_btn).on_hover_text(self.lang.t("Ghim trên cùng", "Always on Top")).clicked() {
                            self.always_on_top = !self.always_on_top;
                            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(if self.always_on_top {
                                egui::WindowLevel::AlwaysOnTop
                            } else {
                                egui::WindowLevel::Normal
                            }));
                        }

                        let lang_label = match self.lang {
                            AppLang::Vi => "[LANG: VI]",
                            AppLang::En => "[LANG: EN]",
                        };
                        let lang_btn = egui::Button::new(
                            RichText::new(lang_label)
                                .color(COLOR_ACCENT)
                                .size(11.0)
                                .strong(),
                        )
                        .stroke(Stroke::new(1.0_f32, COLOR_BORDER))
                        .rounding(Rounding::same(3.0));

                        if ui.add(lang_btn).on_hover_text(self.lang.t("Chuyển sang English", "Switch to Tiếng Việt")).clicked() {
                            self.lang = match self.lang {
                                AppLang::Vi => AppLang::En,
                                AppLang::En => AppLang::Vi,
                            };
                            self.save_current_config();
                        }

                        let guide_label = self.lang.t("[HƯỚNG DẪN]", "[GUIDE]");
                        let guide_color = if self.show_guide { COLOR_ACCENT } else { COLOR_TEXT_MUTED };
                        let guide_btn = egui::Button::new(
                            RichText::new(guide_label)
                                .color(guide_color)
                                .size(11.0)
                                .strong(),
                        )
                        .stroke(Stroke::new(1.0_f32, if self.show_guide { COLOR_ACCENT } else { COLOR_BORDER }))
                        .rounding(Rounding::same(3.0));

                        if ui.add(guide_btn).on_hover_text(self.lang.t("Mở bảng hướng dẫn sử dụng", "Open User Guide")).clicked() {
                            self.show_guide = !self.show_guide;
                        }
                    });
                });

                ui.add_space(5.0);

                // Row 2: Scan Hotkey Cluster (Left) | DB Update & App Update Cluster (Right, generously separated)
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(6.0, 4.0);

                    let (f9_bg, f9_stroke, f9_color, f9_text) = if self.ocr_available {
                        (
                            COLOR_ACCENT_DIM,
                            COLOR_ACCENT,
                            COLOR_ACCENT,
                            match self.lang {
                                AppLang::Vi => format!("{}: QUÉT SÚNG", self.hotkey_name),
                                AppLang::En => format!("{}: SCAN WEAPON", self.hotkey_name),
                            },
                        )
                    } else {
                        (
                            Color32::from_rgb(40, 25, 20),
                            COLOR_AMBER,
                            COLOR_AMBER,
                            match self.lang {
                                AppLang::Vi => format!("{}: TẮT (THIẾU OCR)", self.hotkey_name),
                                AppLang::En => format!("{}: OFF (NO OCR)", self.hotkey_name),
                            },
                        )
                    };

                    Frame::default()
                        .fill(f9_bg)
                        .stroke(Stroke::new(1.0_f32, f9_stroke))
                        .rounding(Rounding::same(3.0))
                        .inner_margin(Margin::symmetric(8.0, 3.0))
                        .show(ui, |ui| {
                            let lbl = ui.add(
                                egui::Label::new(
                                    RichText::new(&f9_text)
                                        .color(f9_color)
                                        .size(11.0)
                                        .strong(),
                                )
                                .selectable(false),
                            );
                            if !self.ocr_available {
                                lbl.on_hover_text(match self.lang {
                                    AppLang::Vi => format!("Không nạp được lõi PaddleOCR v4 (thiếu onnxruntime.dll cạnh .exe). Phím {} bị tắt.", self.hotkey_name),
                                    AppLang::En => format!("PaddleOCR v4 engine failed to initialize (missing onnxruntime.dll beside .exe). {} is disabled.", self.hotkey_name),
                                });
                            }
                        });

                    // Rebind Hotkey Button [⌨] or [NHẤN PHÍM...]
                    let (rebind_label, rebind_text_color, rebind_bg, rebind_stroke) = if self.is_rebinding_hotkey {
                        (
                            self.lang.t("[NHẤN PHÍM...]", "[PRESS KEY...]"),
                            COLOR_AMBER,
                            Color32::from_rgb(50, 35, 10),
                            Stroke::new(1.0_f32, COLOR_AMBER),
                        )
                    } else {
                        (
                            "[⌨]",
                            COLOR_TEXT_MAIN,
                            COLOR_CARD,
                            Stroke::new(1.0_f32, COLOR_BORDER),
                        )
                    };

                    let rebind_btn = egui::Button::new(
                        RichText::new(rebind_label)
                            .color(rebind_text_color)
                            .size(11.0)
                            .strong(),
                    )
                    .fill(rebind_bg)
                    .stroke(rebind_stroke)
                    .rounding(Rounding::same(3.0));

                    let rebind_tooltip = if self.is_rebinding_hotkey {
                        self.lang.t(
                            "Đang chờ nhấn phím mới (F1-F12, Insert, Delete, Home, End, Numpad, Mouse 4/5...)... Bấm Esc để hủy.",
                            "Waiting for key press (F1-F12, Insert, Delete, Home, End, Numpad, Mouse 4/5...)... Press Esc to cancel.",
                        )
                    } else {
                        self.lang.t("Đổi phím tắt quét súng", "Change scan hotkey")
                    };

                    if ui.add(rebind_btn).on_hover_text(rebind_tooltip).clicked() {
                        if self.is_rebinding_hotkey {
                            self.is_rebinding_hotkey = false;
                            self.shared_is_rebinding.store(false, Ordering::Relaxed);
                        } else {
                            self.is_rebinding_hotkey = true;
                            self.shared_is_rebinding.store(true, Ordering::Relaxed);
                            self.rebind_start_time = Some(Instant::now());
                        }
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;

                        // [UPDATE DB] button with 30-min cooldown
                        let cooldown_secs: u64 = 1800; // 30 minutes
                        let remaining_cooldown_secs: Option<u64> = self.last_update_success.and_then(|t| {
                            let elapsed = t.elapsed().as_secs();
                            if elapsed < cooldown_secs {
                                Some(cooldown_secs - elapsed)
                            } else {
                                None
                            }
                        });

                        // Only request repaint for cooldown timer when window is active to save background CPU/GPU
                        if is_window_active && remaining_cooldown_secs.is_some() {
                            ctx.request_repaint_after(Duration::from_secs(1));
                        }

                        let (db_text, can_update, db_color) = if self.is_updating {
                            (
                                self.lang.t("[ĐANG TẢI DB...]", "[UPDATING DB...]").to_string(),
                                false,
                                COLOR_AMBER,
                            )
                        } else if let Some(rem_secs) = remaining_cooldown_secs {
                            let rem_mins = (rem_secs + 59) / 60;
                            let label = match self.lang {
                                AppLang::Vi => format!("[CHỜ DB {}M]", rem_mins),
                                AppLang::En => format!("[WAIT DB {}M]", rem_mins),
                            };
                            (label, false, COLOR_TEXT_MUTED)
                        } else {
                            (
                                self.lang.t("[CẬP NHẬT DB]", "[UPDATE DB]").to_string(),
                                true,
                                COLOR_ACCENT,
                            )
                        };

                        let db_btn = egui::Button::new(
                            RichText::new(&db_text)
                                .color(db_color)
                                .size(11.0)
                                .strong(),
                        )
                        .stroke(Stroke::new(1.0_f32, if self.is_updating { COLOR_AMBER } else { COLOR_BORDER }))
                        .rounding(Rounding::same(3.0));

                        let tooltip = if self.is_updating {
                            self.lang.t("Đang tải dữ liệu cấu hình mùa mới...", "Downloading latest database...").to_string()
                        } else if let Some(rem_secs) = remaining_cooldown_secs {
                            let rem_mins = rem_secs / 60;
                            let rem_s = rem_secs % 60;
                            match self.lang {
                                AppLang::Vi => format!("Dữ liệu đã được cập nhật thành công! Vui lòng chờ {} phút {} giây nữa trước khi cập nhật tiếp.", rem_mins, rem_s),
                                AppLang::En => format!("Database was recently updated! Please wait {}m {}s before updating again.", rem_mins, rem_s),
                            }
                        } else {
                            self.lang.t(
                                "Tải về dữ liệu cấu hình mùa mới từ GitHub vào file builds_local.json",
                                "Download latest season loadout database from GitHub into builds_local.json"
                            ).to_string()
                        };

                        if ui
                            .add_enabled(can_update, db_btn)
                            .on_hover_text(tooltip)
                            .clicked()
                        {
                            self.trigger_db_update(ctx.clone());
                        }

                        // Manual Check App Update button (grouped neatly with DB Update on the right)
                        let (upd_text, upd_enabled, upd_color) = if self.is_checking_update {
                            (
                                self.lang.t("[ĐANG KIỂM TRA...]", "[CHECKING...]"),
                                false,
                                COLOR_AMBER,
                            )
                        } else {
                            (
                                self.lang.t("[KIỂM TRA CẬP NHẬT]", "[CHECK UPDATE]"),
                                true,
                                COLOR_TEXT_MUTED,
                            )
                        };
                        let upd_btn = egui::Button::new(
                            RichText::new(upd_text)
                                .color(upd_color)
                                .size(11.0)
                                .strong(),
                        )
                        .stroke(Stroke::new(1.0_f32, if self.is_checking_update { COLOR_AMBER } else { COLOR_BORDER }))
                        .rounding(Rounding::same(3.0));

                        if ui
                            .add_enabled(upd_enabled, upd_btn)
                            .on_hover_text(self.lang.t(
                                "Kiểm tra phiên bản mới trên GitHub Releases",
                                "Check for newer releases on GitHub Releases",
                            ))
                            .clicked()
                        {
                            self.is_checking_update = true;
                            self.up_to_date_banner_until = None;
                            spawn_update_check(self.tx_events.clone(), ctx.clone(), true);
                        }

                        // Prominent New Version Badge Button if an update is available on GitHub
                        if let Some(new_ver) = &self.available_update {
                            let ver_label = match self.lang {
                                AppLang::Vi => format!("[BẢN MỚI {}]", new_ver.tag_name),
                                AppLang::En => format!("[NEW {}]", new_ver.tag_name),
                            };
                            let ver_btn = egui::Button::new(
                                RichText::new(&ver_label)
                                    .color(COLOR_AMBER)
                                    .size(11.0)
                                    .strong(),
                            )
                            .fill(Color32::from_rgb(50, 35, 10))
                            .stroke(Stroke::new(1.0_f32, COLOR_AMBER))
                            .rounding(Rounding::same(3.0));

                            let ver_tooltip = match self.lang {
                                AppLang::Vi => format!("Đã có phiên bản mới {} (hiện tại: {})! Bấm để hiện banner tải về.", new_ver.tag_name, APP_VERSION),
                                AppLang::En => format!("New release {} available (current: {})! Click to show update banner.", new_ver.tag_name, APP_VERSION),
                            };

                            if ui.add(ver_btn).on_hover_text(ver_tooltip).clicked() {
                                self.update_banner_dismissed = false;
                            }
                        }
                    });
                });
            });

        // Prominent Update / Download Completion Banner
        if let Some(update_info) = self.available_update.clone().filter(|_| !self.update_banner_dismissed) {
            let is_downloaded = self.downloaded_update_path.is_some();
            let (banner_bg, banner_stroke, text_color) = if is_downloaded {
                (Color32::from_rgb(16, 44, 30), COLOR_ACCENT, COLOR_ACCENT)
            } else {
                (Color32::from_rgb(42, 30, 12), COLOR_AMBER, COLOR_AMBER)
            };

            egui::TopBottomPanel::top("update_banner")
                .frame(
                    Frame::default()
                        .fill(banner_bg)
                        .stroke(Stroke::new(1.0_f32, banner_stroke))
                        .inner_margin(Margin::symmetric(14.0, 6.0)),
                )
                .show(ctx, |ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;

                        let close_btn = egui::Button::new(
                            RichText::new("✕").color(COLOR_TEXT_MUTED).size(11.0),
                        )
                        .fill(Color32::TRANSPARENT);
                        if ui.add(close_btn).clicked() {
                            self.update_banner_dismissed = true;
                        }

                        if let Some(zip_url) = update_info.zip_download_url.clone() {
                            let busy = self.is_downloading_update || self.is_applying_update;
                            let zip_label = if self.is_applying_update {
                                self.lang.t("[⚡ ĐANG CHUẨN BỊ CẬP NHẬT...]", "[⚡ PREPARING UPDATE...]")
                            } else if self.is_downloading_update {
                                self.lang.t("[⏳ ĐANG TẢI & KIỂM TRA .ZIP...]", "[⏳ DOWNLOADING & VERIFYING .ZIP...]")
                            } else {
                                self.lang.t("[⚡ TỰ CẬP NHẬT NGAY]", "[⚡ AUTO-UPDATE NOW]")
                            };
                            let zip_btn = egui::Button::new(
                                RichText::new(zip_label)
                                    .color(if busy { COLOR_TEXT_MUTED } else { COLOR_BG })
                                    .size(11.0)
                                    .strong(),
                            )
                            .fill(if busy { Color32::from_rgb(60, 50, 30) } else { COLOR_AMBER })
                            .rounding(Rounding::same(2.0));
                            if ui
                                .add_enabled(!busy, zip_btn)
                                .on_hover_text(self.lang.t(
                                    "Tải gói .zip vào DF-Loadout-Scanner-Data/downloads/, tự đóng app, cài đè bản mới (giữ nguyên dữ liệu người dùng) và tự mở lại",
                                    "Download .zip into DF-Loadout-Scanner-Data/downloads/, close app, apply update (preserving user data) and relaunch",
                                ))
                                .clicked()
                            {
                                self.is_downloading_update = true;
                                let file_name = update_info
                                    .zip_file_name
                                    .clone()
                                    .unwrap_or_else(|| format!("DF-Loadout-Scanner-{}-Windows-x64.zip", update_info.tag_name));
                                spawn_update_zip_download(
                                    self.tx_events.clone(),
                                    ctx.clone(),
                                    zip_url,
                                    file_name,
                                    update_info.tag_name.clone(),
                                );
                            }
                        }

                        let open_rel_btn = egui::Button::new(
                            RichText::new(self.lang.t("[🌐 MỞ RELEASE]", "[🌐 OPEN RELEASE]"))
                                .color(text_color)
                                .size(11.0)
                                .strong(),
                        )
                        .stroke(Stroke::new(1.0_f32, text_color))
                        .rounding(Rounding::same(2.0));
                        if ui.add(open_rel_btn).clicked() {
                            ctx.open_url(egui::OpenUrl::new_tab(&update_info.html_url));
                        }

                        let mut auto_chk = self.auto_check_update;
                        if ui
                            .checkbox(
                                &mut auto_chk,
                                RichText::new(self.lang.t("Tự kiểm tra", "Auto-check"))
                                    .color(COLOR_TEXT_MUTED)
                                    .size(11.0),
                            )
                            .changed()
                        {
                            self.auto_check_update = auto_chk;
                            self.save_current_config();
                        }

                        let rem_w = ui.available_width().max(60.0);
                        ui.allocate_ui_with_layout(
                            egui::vec2(rem_w, 20.0),
                            Layout::left_to_right(Align::Center),
                            |ui| {
                                let banner_text = if let Some(dl_path) = &self.downloaded_update_path {
                                    match self.lang {
                                        AppLang::Vi => format!(
                                            "✔ Đã tải xong bản {} về: {}",
                                            update_info.tag_name,
                                            dl_path.display()
                                        ),
                                        AppLang::En => format!(
                                            "✔ Downloaded {} to: {}",
                                            update_info.tag_name,
                                            dl_path.display()
                                        ),
                                    }
                                } else if update_info.summary.is_empty() {
                                    match self.lang {
                                        AppLang::Vi => format!(
                                            "🚀 Có bản mới {} (hiện tại: {}) — {}",
                                            update_info.tag_name, APP_VERSION, update_info.name
                                        ),
                                        AppLang::En => format!(
                                            "🚀 Update available {} (current: {}) — {}",
                                            update_info.tag_name, APP_VERSION, update_info.name
                                        ),
                                    }
                                } else {
                                    match self.lang {
                                        AppLang::Vi => format!(
                                            "🚀 Có bản mới {} (hiện tại: {}) — {}",
                                            update_info.tag_name, APP_VERSION, update_info.summary
                                        ),
                                        AppLang::En => format!(
                                            "🚀 Update available {} (current: {}) — {}",
                                            update_info.tag_name, APP_VERSION, update_info.summary
                                        ),
                                    }
                                };
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(banner_text)
                                            .color(text_color)
                                            .size(12.0)
                                            .strong(),
                                    )
                                    .truncate()
                                    .selectable(false),
                                );
                            },
                        );
                    });
                });
        } else if let Some(updated_ver) = self.just_updated_version.clone() {
            egui::TopBottomPanel::top("post_update_banner")
                .frame(
                    Frame::default()
                        .fill(Color32::from_rgb(16, 44, 30))
                        .stroke(Stroke::new(1.0_f32, COLOR_ACCENT))
                        .inner_margin(Margin::symmetric(14.0, 6.0)),
                )
                .show(ctx, |ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let close_btn = egui::Button::new(
                            RichText::new("✕").color(COLOR_TEXT_MUTED).size(11.0),
                        )
                        .fill(Color32::TRANSPARENT);
                        if ui.add(close_btn).clicked() {
                            self.just_updated_version = None;
                        }
                        let rem_w = ui.available_width().max(60.0);
                        ui.allocate_ui_with_layout(
                            egui::vec2(rem_w, 20.0),
                            Layout::left_to_right(Align::Center),
                            |ui| {
                                let msg = match self.lang {
                                    AppLang::Vi => format!(
                                        "🎉 Đã cập nhật thành công lên phiên bản mới {}! Toàn bộ cấu hình và dữ liệu cá nhân được giữ nguyên.",
                                        updated_ver
                                    ),
                                    AppLang::En => format!(
                                        "🎉 Successfully updated to version {}! All your settings and personal data were preserved.",
                                        updated_ver
                                    ),
                                };
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(msg).color(COLOR_ACCENT).size(12.0).strong(),
                                    )
                                    .truncate()
                                    .selectable(false),
                                );
                            },
                        );
                    });
                });
        } else if let Some(until) = self.up_to_date_banner_until {
            if Instant::now() < until {
                if is_window_active {
                    ctx.request_repaint_after(Duration::from_secs(1));
                }
                egui::TopBottomPanel::top("up_to_date_banner")
                    .frame(
                        Frame::default()
                            .fill(Color32::from_rgb(16, 44, 30))
                            .stroke(Stroke::new(1.0_f32, COLOR_ACCENT))
                            .inner_margin(Margin::symmetric(14.0, 5.0)),
                    )
                    .show(ctx, |ui| {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let close_btn = egui::Button::new(
                                RichText::new("✕").color(COLOR_TEXT_MUTED).size(11.0),
                            )
                            .fill(Color32::TRANSPARENT);
                            if ui.add(close_btn).clicked() {
                                self.up_to_date_banner_until = None;
                            }
                            let rem_w = ui.available_width().max(60.0);
                            ui.allocate_ui_with_layout(
                                egui::vec2(rem_w, 18.0),
                                Layout::left_to_right(Align::Center),
                                |ui| {
                                    let msg = match self.lang {
                                        AppLang::Vi => format!("✔ Bạn đang sử dụng phiên bản mới nhất ({}).", APP_VERSION),
                                        AppLang::En => format!("✔ You are already on the latest version ({}).", APP_VERSION),
                                    };
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(msg).color(COLOR_ACCENT).size(12.0).strong(),
                                        )
                                        .truncate()
                                        .selectable(false),
                                    );
                                },
                            );
                        });
                    });
            } else {
                self.up_to_date_banner_until = None;
            }
        }

        // 5-Level Filter Bar (Directly below Header) or Favorites Banner
        if self.current_tab == AppTab::Scanner {
            egui::TopBottomPanel::top("filter_panel")
            .frame(
                Frame::default()
                    .fill(Color32::from_rgb(15, 22, 29))
                    .stroke(Stroke::new(1.0_f32, COLOR_BORDER))
                    .inner_margin(Margin {
                        left: 14.0,
                        right: 14.0,
                        top: 6.0,
                        bottom: 6.0,
                    }),
            )
            .show(ctx, |ui| {
                let available_weapons = self.get_available_weapons();

                // Compute display weapons filtered by search_query (with alias support)
                let display_weapons: Vec<String> = if self.search_query.trim().is_empty() {
                    available_weapons.clone()
                } else {
                    let filtered: Vec<String> = available_weapons
                        .iter()
                        .filter(|w| matches_weapon_search(w, &self.search_query))
                        .cloned()
                        .collect();
                    if filtered.is_empty() {
                        let global: Vec<String> = self.unique_weapons
                            .iter()
                            .filter(|w| matches_weapon_search(w, &self.search_query))
                            .cloned()
                            .collect();
                        if global.is_empty() {
                            available_weapons.clone()
                        } else {
                            global
                        }
                    } else {
                        filtered
                    }
                };

                // Auto-reset selected_weapon if it has no builds under current filters
                if let Some(ref cur) = self.selected_weapon.clone() {
                    if !available_weapons.iter().any(|w| w == cur) && !self.unique_weapons.iter().any(|w| w == cur) {
                        self.selected_weapon = if available_weapons.iter().any(|w| w == "M4A1") {
                            Some("M4A1".to_string())
                        } else {
                            available_weapons.first().cloned()
                        };
                    }
                } else {
                    self.selected_weapon = if available_weapons.iter().any(|w| w == "M4A1") {
                        Some("M4A1".to_string())
                    } else {
                        available_weapons.first().cloned()
                    };
                }

                // Row 1: BỘ LỌC: | Nguồn: [..] | Server: [..] | Chế độ: [..] | Loại: [..]
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Label::new(
                            RichText::new(self.lang.t("BỘ LỌC:", "FILTER:"))
                                .color(COLOR_ACCENT)
                                .size(12.0)
                                .strong()
                                .monospace(),
                        )
                        .selectable(false),
                    );

                    ui.add_space(5.0);

                    // 1. Web Source ComboBox
                    ui.add(egui::Label::new(RichText::new(self.lang.t("Nguồn:", "Source:")).color(COLOR_TEXT_MUTED).size(12.0)).selectable(false));
                    let prev_source = self.selected_source;
                    egui::ComboBox::from_id_salt("filter_source_cb")
                        .selected_text(RichText::new(self.selected_source.label(self.lang)).size(12.0))
                        .width(105.0)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.selected_source, SourceFilter::All, self.lang.t("Tất cả nguồn", "All Sources"));
                            ui.selectable_value(&mut self.selected_source, SourceFilter::DFTools, "DFTools");
                            ui.selectable_value(&mut self.selected_source, SourceFilter::DFBuilds, "DFBuilds");
                            ui.selectable_value(&mut self.selected_source, SourceFilter::FPSMeta, "FPSMeta");
                        });
                    if prev_source != self.selected_source {
                        self.sync_active_filters();
                        self.save_current_config();
                    }

                    ui.add_space(6.0);
                    ui.add(egui::Label::new(RichText::new("|").color(COLOR_BORDER).size(12.0)).selectable(false));
                    ui.add_space(6.0);

                    // 2. Server ComboBox
                    ui.add(egui::Label::new(RichText::new("Server:").color(COLOR_TEXT_MUTED).size(12.0)).selectable(false));
                    let prev_server = self.selected_server;
                    egui::ComboBox::from_id_salt("filter_server_cb")
                        .selected_text(RichText::new(self.selected_server.label()).size(12.0))
                        .width(105.0)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.selected_server, ServerFilter::Garena, "Garena");
                            ui.selectable_value(&mut self.selected_server, ServerFilter::Infinity, "Infinity (Global)");
                        });
                    if prev_server != self.selected_server {
                        self.sync_active_filters();
                        self.save_current_config();
                    }

                    ui.add_space(6.0);
                    ui.add(egui::Label::new(RichText::new("|").color(COLOR_BORDER).size(12.0)).selectable(false));
                    ui.add_space(6.0);

                    // 3. Mode ComboBox
                    ui.add(egui::Label::new(RichText::new(self.lang.t("Chế độ:", "Mode:")).color(COLOR_TEXT_MUTED).size(12.0)).selectable(false));
                    let prev_mode = self.selected_mode;
                    egui::ComboBox::from_id_salt("filter_mode_cb")
                        .selected_text(RichText::new(self.selected_mode.label(self.lang)).size(12.0))
                        .width(125.0)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.selected_mode, ModeFilter::All, self.lang.t("Tất cả chế độ", "All Modes"));
                            ui.selectable_value(&mut self.selected_mode, ModeFilter::Warfare, self.lang.t("Chiến Trường (Warfare)", "Warfare"));
                            ui.selectable_value(&mut self.selected_mode, ModeFilter::Operations, self.lang.t("Sinh Tồn (Operations)", "Operations"));
                        });
                    if prev_mode != self.selected_mode {
                        self.sync_active_filters();
                        self.save_current_config();
                    }

                    ui.add_space(6.0);
                    ui.add(egui::Label::new(RichText::new("|").color(COLOR_BORDER).size(12.0)).selectable(false));
                    ui.add_space(6.0);

                    // 4. Weapon Type ComboBox
                    ui.add(egui::Label::new(RichText::new(self.lang.t("Loại:", "Type:")).color(COLOR_TEXT_MUTED).size(12.0)).selectable(false));
                    let type_label = if self.selected_type.is_empty() {
                        self.lang.t("Tất cả", "All")
                    } else {
                        &self.selected_type
                    };
                    egui::ComboBox::from_id_salt("filter_type_cb")
                        .selected_text(RichText::new(type_label).size(12.0))
                        .width(115.0)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.selected_type, String::new(), self.lang.t("Tất cả", "All"));
                            for wt in &self.all_weapon_types {
                                ui.selectable_value(&mut self.selected_type, wt.clone(), wt);
                            }
                        });
                });

                ui.add_space(4.0);

                // Row 2: Tìm kiếm: [TextEdit ~220px + [×]] | Chọn vũ khí: [ComboBox ~240px]
                ui.horizontal(|ui| {
                    ui.add(egui::Label::new(RichText::new(self.lang.t("Tìm kiếm:", "Search:")).color(COLOR_TEXT_MUTED).size(12.0)).selectable(false));
                    let search_res = ui.add(
                        egui::TextEdit::singleline(&mut self.search_query)
                            .hint_text(self.lang.t("Tên súng, bí danh (vd: hai nong, cung, ar57...)", "Weapon name, alias (e.g. hai nong, cung, ar57...)"))
                            .desired_width(220.0),
                    );

                    if !self.search_query.is_empty() {
                        let clear_btn = egui::Button::new(
                            RichText::new("×")
                                .color(COLOR_TEXT_MUTED)
                                .size(11.0)
                                .strong(),
                        )
                        .fill(COLOR_CARD)
                        .stroke(Stroke::new(1.0_f32, COLOR_BORDER))
                        .rounding(Rounding::same(2.0));

                        if ui.add(clear_btn).on_hover_text(self.lang.t("Xóa từ khóa tìm kiếm", "Clear search query")).clicked() {
                            self.search_query.clear();
                        }
                    }

                    if search_res.changed() && !self.search_query.trim().is_empty() {
                        if let Some(matched) = find_best_matching_weapon(&available_weapons, &self.search_query) {
                            self.selected_weapon = Some(matched.clone());
                        } else if let Some(matched) = find_best_matching_weapon(&self.unique_weapons, &self.search_query) {
                            self.selected_weapon = Some(matched.clone());
                            self.selected_type.clear();
                        }
                    }

                    ui.add_space(8.0);
                    ui.add(egui::Label::new(RichText::new("|").color(COLOR_BORDER).size(12.0)).selectable(false));
                    ui.add_space(8.0);

                    // 5. Weapon ComboBox
                    ui.add(egui::Label::new(RichText::new(self.lang.t("Chọn vũ khí:", "Select Weapon:")).color(COLOR_TEXT_MUTED).size(12.0)).selectable(false));
                    let weapon_label = self.selected_weapon.as_deref().unwrap_or(self.lang.t("Chọn súng...", "Select Weapon..."));
                    egui::ComboBox::from_id_salt("filter_weapon_cb")
                        .selected_text(RichText::new(weapon_label).color(COLOR_ACCENT).size(12.0).strong())
                        .width(240.0)
                        .show_ui(ui, |ui| {
                            for w in &display_weapons {
                                let is_selected = self.selected_weapon.as_ref() == Some(w);
                                if ui.selectable_label(is_selected, w).clicked() {
                                    self.selected_weapon = Some(w.clone());
                                }
                            }
                        });

                    if !self.search_query.trim().is_empty() {
                        ui.add_space(4.0);
                        ui.add(
                            egui::Label::new(
                                RichText::new(format!("({} {})", display_weapons.len(), self.lang.t("khớp", "matched")))
                                    .color(COLOR_TEXT_MUTED)
                                    .size(11.0),
                            )
                            .selectable(false),
                        );
                    }

                    ui.add_space(8.0);
                    ui.add(egui::Label::new(RichText::new("|").color(COLOR_BORDER).size(12.0)).selectable(false));
                    ui.add_space(8.0);

                    // 6. Monitor Selector ComboBox (None = Auto Foreground Game Window)
                    ui.add(egui::Label::new(RichText::new(self.lang.t("Màn hình:", "Monitor:")).color(COLOR_TEXT_MUTED).size(12.0)).selectable(false));
                    let monitor_label = match &self.selected_monitor_id {
                        None => self.lang.t("Tự động (Game)", "Auto (Game)").to_string(),
                        Some(id) => self
                            .available_monitors
                            .iter()
                            .find(|(m_id, _)| m_id == id)
                            .map(|(_, name)| name.clone())
                            .unwrap_or_else(|| format!("{} ({})", self.lang.t("Đã ngắt", "Offline"), id)),
                    };
                    let prev_mon = self.selected_monitor_id.clone();
                    let cb_resp = egui::ComboBox::from_id_salt("filter_monitor_cb")
                        .selected_text(RichText::new(monitor_label).size(12.0))
                        .width(155.0)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.selected_monitor_id,
                                None,
                                self.lang.t("Tự động (Theo cửa sổ game)", "Auto (Active Window)"),
                            );
                            for (mon_id, name) in &self.available_monitors {
                                ui.selectable_value(&mut self.selected_monitor_id, Some(mon_id.clone()), name);
                            }
                        });
                    if cb_resp.response.clicked() {
                        self.available_monitors = enumerate_monitors(self.lang);
                    }
                    if prev_mon != self.selected_monitor_id {
                        self.sync_active_filters();
                        self.save_current_config();
                    }
                });
            });

        let mut confirm_pending = false;
        let mut cancel_pending = false;

        // Bottom Status Bar (right-aligned info & confirm buttons reserved first so status text truncates cleanly without overlap)
        egui::TopBottomPanel::bottom("status_bar")
            .frame(
                Frame::default()
                    .fill(COLOR_CARD)
                    .stroke(Stroke::new(1.0_f32, COLOR_BORDER))
                    .inner_margin(Margin::symmetric(14.0, 6.0)),
            )
            .show(ctx, |ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let status_info = format!(
                        "Source: {} | Server: {} | Mode: {} | Total: {} Builds",
                        self.selected_source.as_str(),
                        self.selected_server.as_str().to_uppercase(),
                        self.selected_mode.as_str().to_uppercase(),
                        self.builds.len()
                    );
                    ui.add(
                        egui::Label::new(
                            RichText::new(status_info)
                                .color(COLOR_TEXT_MUTED)
                                .size(11.0)
                                .monospace(),
                        )
                        .selectable(false),
                    );

                    ui.add_space(10.0);

                    if self.pending_confirm_builds.is_some() {
                        let cancel_btn = egui::Button::new(
                            RichText::new("✕")
                                .color(COLOR_TEXT_MUTED)
                                .size(11.0),
                        )
                        .fill(Color32::TRANSPARENT);

                        if ui.add(cancel_btn).clicked() {
                            cancel_pending = true;
                        }

                        let confirm_btn = egui::Button::new(
                            RichText::new(self.lang.t("✔ XÁC NHẬN & COPY", "✔ CONFIRM & COPY"))
                                .color(COLOR_BG)
                                .size(11.0)
                                .strong(),
                        )
                        .fill(COLOR_AMBER)
                        .rounding(Rounding::same(2.0));

                        if ui
                            .add(confirm_btn)
                            .on_hover_text(self.lang.t(
                                "Nhận diện chưa đủ rào chắn an toàn Auto-Copy. Bấm để xác nhận súng này và sao chép vào Clipboard",
                                "Match requires manual confirmation (safety guard triggered). Click to confirm weapon and copy loadouts to Clipboard"
                            ))
                            .clicked()
                        {
                            confirm_pending = true;
                        }

                        ui.add_space(6.0);
                    }

                    let remaining_w = ui.available_width().max(40.0);
                    ui.allocate_ui_with_layout(
                        egui::vec2(remaining_w, 18.0),
                        Layout::left_to_right(Align::Center),
                        |ui| {
                            if let Some((msg, _time, is_error)) = &self.status_message {
                                let icon = if *is_error { "[!] " } else { "[i] " };
                                let color = if self.pending_confirm_builds.is_some() {
                                    COLOR_AMBER
                                } else if *is_error {
                                    COLOR_RED
                                } else {
                                    COLOR_ACCENT
                                };
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(format!("{}{}", icon, msg))
                                            .color(color)
                                            .size(12.0),
                                    )
                                    .truncate()
                                    .selectable(false),
                                );
                            }
                        },
                    );
                });
            });

        if confirm_pending {
            if let Some((weapon, builds)) = self.pending_confirm_builds.take() {
                if !builds.is_empty() {
                    let codes: Vec<String> = builds.iter().map(|b| b.loadout_code.clone()).collect();
                    let mut cb_opt = init_clipboard_with_retry(3).ok();
                    match copy_codes_sequence_to_clipboard(&mut cb_opt, &codes, 150) {
                        Ok(report) => {
                            let all_ok = report.success_count == report.total_count;
                            unsafe {
                                let _ = if all_ok {
                                    MessageBeep(MB_OK)
                                } else {
                                    MessageBeep(MB_ICONWARNING)
                                };
                            }
                            if let Some(top_build) = builds.first() {
                                self.record_history_entry(
                                    &weapon,
                                    &top_build.classification,
                                    top_build.copy_count,
                                    &top_build.loadout_code,
                                );
                            }
                            let msg = if all_ok {
                                match self.lang {
                                    AppLang::Vi => format!(
                                        "Đã xác nhận súng [{}] và sao chép {}/{} cấu hình vào Clipboard!",
                                        weapon, report.success_count, report.total_count
                                    ),
                                    AppLang::En => format!(
                                        "Confirmed weapon [{}] and copied {}/{} loadouts to Clipboard!",
                                        weapon, report.success_count, report.total_count
                                    ),
                                }
                            } else {
                                match self.lang {
                                    AppLang::Vi => format!(
                                        "Đã xác nhận [{}] nhưng chỉ nạp được {}/{} cấu hình vào Win + V!",
                                        weapon, report.success_count, report.total_count
                                    ),
                                    AppLang::En => format!(
                                        "Confirmed [{}] but only pushed {}/{} loadouts to Win + V!",
                                        weapon, report.success_count, report.total_count
                                    ),
                                }
                            };
                            self.status_message = Some((msg, Instant::now(), !all_ok));
                        }
                        Err(err) => {
                            unsafe {
                                let _ = MessageBeep(MB_ICONERROR);
                            }
                            let msg = match self.lang {
                                AppLang::Vi => format!("Đã xác nhận [{}] nhưng lỗi ghi Clipboard: {}", weapon, err),
                                AppLang::En => format!("Confirmed [{}] but Clipboard write failed: {}", weapon, err),
                            };
                            self.status_message = Some((msg, Instant::now(), true));
                        }
                    }
                } else {
                    let msg = match self.lang {
                        AppLang::Vi => format!("Đã xác nhận súng [{}] (chưa có cấu hình phù hợp)", weapon),
                        AppLang::En => format!("Confirmed weapon [{}] (no matching builds)", weapon),
                    };
                    self.status_message = Some((msg, Instant::now(), false));
                }
            }
            self.pending_confirm_score = None;
        }

        if cancel_pending {
            self.pending_confirm_builds = None;
            self.pending_confirm_score = None;
        }

        // Right Column: History Panel (35% approx width)
        egui::SidePanel::right("history_panel")
            .exact_width(290.0)
            .frame(
                Frame::default()
                    .fill(COLOR_BG)
                    .stroke(Stroke::new(1.0_f32, COLOR_BORDER))
                    .inner_margin(Margin::same(10.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Label::new(
                            RichText::new(self.lang.t("LỊCH SỬ ĐÃ QUÉT", "SCAN HISTORY"))
                                .color(COLOR_ACCENT)
                                .size(13.0)
                                .strong(),
                        )
                        .selectable(false),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .button(RichText::new(self.lang.t("[XÓA]", "[CLEAR]")).color(COLOR_TEXT_MUTED).size(11.0))
                            .clicked()
                        {
                            self.history.clear();
                            if let Err(err) = save_history(&self.history) {
                                let msg = match self.lang {
                                    AppLang::Vi => format!("Lỗi lưu lịch sử (df_history.json): {}", err),
                                    AppLang::En => format!("Failed to save history (df_history.json): {}", err),
                                };
                                self.status_message = Some((msg, Instant::now(), true));
                            }
                        }
                    });
                });

                ui.add_space(6.0);
                ui.separator();
                ui.add_space(6.0);

                if self.history.is_empty() {
                    ui.vertical_centered(|ui| {
                        ui.add_space(40.0);
                        ui.label(RichText::new(self.lang.t("Chưa có lịch sử.", "No scan history.")).color(COLOR_TEXT_MUTED).size(13.0));
                        let no_hist_msg = match self.lang {
                            AppLang::Vi => format!("Nhấn [{}] trong game hoặc sao chép build để lưu vào đây.", self.hotkey_name),
                            AppLang::En => format!("Press [{}] in-game or copy a build to save here.", self.hotkey_name),
                        };
                        ui.label(
                            RichText::new(no_hist_msg)
                                .color(COLOR_TEXT_MUTED)
                                .size(11.0),
                        );
                    });
                } else {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            let mut item_to_select = None;
                            let mut code_to_copy = None;

                            for item in &self.history {
                                Frame::default()
                                    .fill(COLOR_CARD)
                                    .stroke(Stroke::new(1.0_f32, COLOR_BORDER))
                                    .rounding(Rounding::same(3.0))
                                    .inner_margin(Margin::same(8.0))
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            ui.label(
                                                RichText::new(&item.timestamp)
                                                    .color(COLOR_TEXT_MUTED)
                                                    .size(11.0)
                                                    .monospace(),
                                            );
                                            ui.label(
                                                RichText::new(&item.weapon)
                                                    .color(COLOR_ACCENT)
                                                    .size(13.0)
                                                    .strong(),
                                            );

                                            if item.copy_count > 0 {
                                                ui.label(
                                                    RichText::new(format!("📋 {}", format_number_with_commas(item.copy_count)))
                                                        .color(Color32::from_rgb(100, 200, 255))
                                                        .size(10.0),
                                                );
                                            }

                                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                                if ui
                                                    .button(RichText::new("[COPY]").size(10.0))
                                                    .on_hover_text(self.lang.t("Sao chép lại mã build này", "Copy this loadout code"))
                                                    .clicked()
                                                {
                                                    code_to_copy = Some((
                                                        item.weapon.clone(),
                                                        item.classification.clone(),
                                                        item.copy_count,
                                                        item.loadout_code.clone(),
                                                    ));
                                                }
                                            });
                                        });

                                        ui.add_space(2.0);
                                        let display_class = format_display_classification(&item.classification, self.lang);
                                        let class_btn = ui.add(
                                            egui::Label::new(
                                                RichText::new(display_class)
                                                    .color(COLOR_TEXT_MAIN)
                                                    .size(11.0),
                                            )
                                            .sense(egui::Sense::click()),
                                        );

                                        if class_btn.on_hover_text(self.lang.t("Bấm để xem các cấu hình của súng này", "Click to view builds for this weapon")).clicked() {
                                            item_to_select = Some(item.weapon.clone());
                                        }
                                    });
                                ui.add_space(4.0);
                            }

                            if let Some(w) = item_to_select {
                                self.selected_weapon = Some(w);
                            }
                            if let Some((w, c, cnt, code)) = code_to_copy {
                                self.copy_build_code(&w, &c, cnt, &code);
                            }
                        });
                }
            });
        } else {
            egui::TopBottomPanel::top("fav_banner_panel")
                .frame(
                    Frame::default()
                        .fill(Color32::from_rgb(15, 22, 29))
                        .stroke(Stroke::new(1.0_f32, COLOR_BORDER))
                        .inner_margin(Margin {
                            left: 14.0,
                            right: 14.0,
                            top: 8.0,
                            bottom: 8.0,
                        }),
                )
                .show(ctx, |ui| {
                    let mut copy_all_favs = false;
                    ui.horizontal(|ui| {
                        let back_btn = egui::Button::new(
                            RichText::new(self.lang.t("← QUAY LẠI (ESC)", "← BACK (ESC)"))
                                .color(COLOR_TEXT_MAIN)
                                .size(11.0)
                                .strong(),
                        )
                        .fill(COLOR_CARD)
                        .stroke(Stroke::new(1.0_f32, COLOR_ACCENT))
                        .rounding(Rounding::same(3.0))
                        .min_size(Vec2::new(130.0, 26.0));

                        if ui.add(back_btn).on_hover_text(self.lang.t("Quay về màn hình Quét & Tra cứu (hoặc bấm phím ESC)", "Return to Scanner screen (or press ESC)")).clicked() {
                            self.current_tab = AppTab::Scanner;
                        }

                        ui.add_space(8.0);
                        ui.add(egui::Label::new(RichText::new("|").color(COLOR_BORDER).size(13.0)).selectable(false));
                        ui.add_space(8.0);

                        ui.add(
                            egui::Label::new(
                                RichText::new(self.lang.t("★ KHO CẤU HÌNH YÊU THÍCH & MÃ CÁ NHÂN", "★ FAVORITES & CUSTOM LOADOUTS"))
                                    .color(Color32::from_rgb(255, 215, 0))
                                    .size(13.0)
                                    .strong()
                                    .monospace(),
                            )
                            .selectable(false),
                        );

                        ui.add_space(6.0);

                        Frame::default()
                            .fill(Color32::from_rgb(45, 40, 10))
                            .stroke(Stroke::new(1.0_f32, Color32::from_rgb(255, 215, 0)))
                            .rounding(Rounding::same(2.0))
                            .inner_margin(Margin::symmetric(6.0, 2.0))
                            .show(ui, |ui| {
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(format!("{} {}", self.favorites.len(), self.lang.t("CẤU HÌNH", "BUILDS")))
                                            .color(Color32::from_rgb(255, 215, 0))
                                            .size(11.0)
                                            .strong()
                                            .monospace(),
                                    )
                                    .selectable(false),
                                );
                            });

                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let copy_all_btn = egui::Button::new(
                                RichText::new(self.lang.t("[COPY TẤT CẢ VÀO WIN+V]", "[COPY ALL TO WIN+V]"))
                                    .color(COLOR_ACCENT)
                                    .size(11.0)
                                    .strong(),
                            )
                            .fill(COLOR_CARD_HOVER)
                            .stroke(Stroke::new(1.0_f32, COLOR_ACCENT))
                            .rounding(Rounding::same(3.0))
                            .min_size(Vec2::new(175.0, 26.0));

                            if ui
                                .add_enabled(!self.favorites.is_empty(), copy_all_btn)
                                .on_hover_text(self.lang.t(
                                    "Đẩy tối đa 5 cấu hình yêu thích vào Windows Clipboard History (Win + V)",
                                    "Push up to 5 favorite builds into Windows Clipboard History (Win + V)"
                                ))
                                .clicked()
                            {
                                copy_all_favs = true;
                            }

                            ui.add_space(4.0);

                            let add_custom_btn = egui::Button::new(
                                RichText::new(self.lang.t("+ THÊM MÃ RIÊNG", "+ ADD CUSTOM"))
                                    .color(COLOR_TEXT_MAIN)
                                    .size(11.0)
                                    .strong(),
                            )
                            .fill(COLOR_CARD)
                            .stroke(Stroke::new(1.0_f32, COLOR_BORDER))
                            .rounding(Rounding::same(3.0))
                            .min_size(Vec2::new(115.0, 26.0));

                            if ui
                                .add(add_custom_btn)
                                .on_hover_text(self.lang.t(
                                    "Tự thêm mã build cá nhân mới vào kho Yêu thích",
                                    "Add a new custom loadout code to Favorites"
                                ))
                                .clicked()
                            {
                                if self.custom_weapon.is_empty() {
                                    self.custom_weapon = self.selected_weapon.clone().unwrap_or_else(|| "M4A1".to_string());
                                }
                                self.custom_code.clear();
                                self.custom_desc.clear();
                                self.custom_error = None;
                                self.show_add_custom_modal = true;
                            }
                        });
                    });

                    if copy_all_favs {
                        self.copy_all_favorites_to_win_v();
                    }
                });
        }

        // Left Column: Central Panel (Builds list or Favorites)
        egui::CentralPanel::default()
            .frame(Frame::default().fill(COLOR_BG).inner_margin(Margin::same(14.0)))
            .show(ctx, |ui| {
                if self.current_tab == AppTab::Scanner {
                    if let Some(weapon_name) = self.selected_weapon.clone() {
                    let sorted = self.get_filtered_builds(&weapon_name);
                    let mut copy_all_requested = false;

                    // Weapon Header Banner with 4px vertical accent bar on left edge
                    Frame::default()
                        .fill(COLOR_CARD)
                        .stroke(Stroke::new(1.0_f32, COLOR_BORDER))
                        .rounding(Rounding::same(4.0))
                        .inner_margin(Margin::same(12.0))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let (bar_rect, _) = ui.allocate_exact_size(Vec2::new(4.0, 38.0), egui::Sense::hover());
                                ui.painter().rect_filled(bar_rect, Rounding::same(2.0), COLOR_ACCENT);

                                ui.add_space(10.0);

                                ui.vertical(|ui| {
                                    ui.horizontal(|ui| {
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(&weapon_name)
                                                    .color(COLOR_TEXT_MAIN)
                                                    .size(22.0)
                                                    .strong()
                                                    .monospace(),
                                            )
                                            .selectable(false),
                                        );

                                        ui.add_space(8.0);

                                        Frame::default()
                                            .fill(COLOR_ACCENT_DIM)
                                            .stroke(Stroke::new(1.0_f32, COLOR_ACCENT))
                                            .rounding(Rounding::same(2.0))
                                            .inner_margin(Margin::symmetric(6.0, 2.0))
                                            .show(ui, |ui| {
                                                ui.add(
                                                    egui::Label::new(
                                                        RichText::new(format!("{} {}", sorted.len(), self.lang.t("CẤU HÌNH", "BUILDS")))
                                                            .color(COLOR_ACCENT)
                                                            .size(11.0)
                                                            .strong()
                                                            .monospace(),
                                                    )
                                                    .selectable(false),
                                                );
                                            });
                                    });

                                    ui.add_space(2.0);
                                    let mode_info = format!(
                                        "{} // Server: {} // {} // {}",
                                        self.selected_source.label(self.lang),
                                        self.selected_server.as_str(),
                                        self.selected_mode.label(self.lang),
                                        if self.selected_type.is_empty() { self.lang.t("Tất cả", "All") } else { &self.selected_type }
                                    );
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(mode_info)
                                                .color(COLOR_TEXT_MUTED)
                                                .size(11.0),
                                        )
                                        .selectable(false),
                                    );
                                });

                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    let copy_all_btn = egui::Button::new(
                                        RichText::new(self.lang.t("[COPY TẤT CẢ VÀO WIN+V]", "[COPY ALL TO WIN+V]"))
                                            .color(COLOR_ACCENT)
                                            .size(11.0)
                                            .strong(),
                                    )
                                    .fill(COLOR_CARD_HOVER)
                                    .stroke(Stroke::new(1.0_f32, COLOR_ACCENT))
                                    .rounding(Rounding::same(3.0))
                                    .min_size(Vec2::new(175.0, 28.0));

                                    if ui
                                        .add(copy_all_btn)
                                        .on_hover_text(self.lang.t(
                                            "Đẩy toàn bộ tối đa 5 cấu hình vào Windows Clipboard History (Win + V), bản Meta nằm ở đầu",
                                            "Push up to 5 builds into Windows Clipboard History (Win + V), Meta at top"
                                        ))
                                        .clicked()
                                    {
                                        copy_all_requested = true;
                                    }

                                    ui.add_space(4.0);

                                    let add_custom_btn = egui::Button::new(
                                        RichText::new(self.lang.t("+ THÊM MÃ RIÊNG", "+ ADD CUSTOM"))
                                            .color(COLOR_TEXT_MAIN)
                                            .size(11.0)
                                            .strong(),
                                    )
                                    .fill(COLOR_CARD)
                                    .stroke(Stroke::new(1.0_f32, COLOR_BORDER))
                                    .rounding(Rounding::same(3.0))
                                    .min_size(Vec2::new(115.0, 28.0));

                                    if ui
                                        .add(add_custom_btn)
                                        .on_hover_text(self.lang.t(
                                            "Tự thêm mã build cá nhân cho khẩu súng này",
                                            "Add your custom loadout code for this weapon"
                                        ))
                                        .clicked()
                                    {
                                        self.custom_weapon = weapon_name.clone();
                                        self.custom_code.clear();
                                        self.custom_desc.clear();
                                        self.custom_error = None;
                                        self.show_add_custom_modal = true;
                                    }
                                });
                            });
                        });

                    if copy_all_requested {
                        self.copy_all_builds_to_win_v(&weapon_name, &sorted);
                    }

                    ui.add_space(10.0);

                    if sorted.is_empty() {
                        ui.vertical_centered(|ui| {
                            ui.add_space(30.0);
                            ui.label(RichText::new(self.lang.t("Chưa có bản build nào khớp với bộ lọc.", "No builds match the current filter.")).color(COLOR_TEXT_MUTED));
                        });
                    } else {
                        let mut build_to_copy: Option<(String, String, u64, String)> = None;
                        let mut build_to_toggle_fav: Option<BuildItem> = None;
                        let mut code_to_delete: Option<String> = None;

                        egui::ScrollArea::vertical()
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                for (idx, b) in sorted.iter().enumerate() {
                                    let is_fav = self.is_favorite(&b.loadout_code);
                                    let is_meta = b.classification.to_lowercase().contains("meta")
                                        || b.tags.iter().any(|t| t.to_lowercase().contains("meta"));
                                    let is_recommended = b.classification.to_lowercase().contains("đề xuất")
                                        || b.tags.iter().any(|t| t.to_lowercase().contains("đề xuất"));

                                    let card_border = if is_fav {
                                        Stroke::new(1.2_f32, Color32::from_rgb(255, 215, 0))
                                    } else if is_meta {
                                        Stroke::new(1.0_f32, COLOR_ACCENT)
                                    } else if is_recommended {
                                        Stroke::new(1.0_f32, COLOR_AMBER)
                                    } else {
                                        Stroke::new(1.0_f32, COLOR_BORDER)
                                    };

                                    Frame::default()
                                        .fill(COLOR_CARD)
                                        .stroke(card_border)
                                        .rounding(Rounding::same(4.0))
                                        .inner_margin(Margin::same(12.0))
                                        .show(ui, |ui| {
                                            ui.horizontal(|ui| {
                                                ui.add(
                                                    egui::Label::new(
                                                        RichText::new(format!("#{}.", idx + 1))
                                                            .color(COLOR_TEXT_MUTED)
                                                            .size(13.0)
                                                            .monospace(),
                                                    )
                                                    .selectable(false),
                                                );

                                                if is_fav {
                                                    Frame::default()
                                                        .fill(Color32::from_rgb(50, 45, 10))
                                                        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(255, 215, 0)))
                                                        .rounding(Rounding::same(2.0))
                                                        .inner_margin(Margin::symmetric(6.0, 2.0))
                                                        .show(ui, |ui| {
                                                            ui.add(
                                                                egui::Label::new(
                                                                    RichText::new(if b.is_custom {
                                                                        self.lang.t("★ MÃ RIÊNG", "★ CUSTOM")
                                                                    } else {
                                                                        self.lang.t("★ YÊU THÍCH", "★ FAVORITE")
                                                                    })
                                                                    .color(Color32::from_rgb(255, 215, 0))
                                                                    .size(11.0)
                                                                    .strong(),
                                                                )
                                                                .selectable(false),
                                                            );
                                                        });
                                                }

                                                if is_meta {
                                                    Frame::default()
                                                        .fill(COLOR_ACCENT_DIM)
                                                        .stroke(Stroke::new(1.0_f32, COLOR_ACCENT))
                                                        .rounding(Rounding::same(2.0))
                                                        .inner_margin(Margin::symmetric(6.0, 2.0))
                                                        .show(ui, |ui| {
                                                            ui.add(
                                                                egui::Label::new(
                                                                    RichText::new(self.lang.t("CẤU HÌNH META", "META BUILD"))
                                                                        .color(COLOR_ACCENT)
                                                                        .size(11.0)
                                                                        .strong(),
                                                                )
                                                                .selectable(false),
                                                            );
                                                        });
                                                }

                                                if is_recommended {
                                                    Frame::default()
                                                        .fill(Color32::from_rgb(60, 45, 0))
                                                        .stroke(Stroke::new(1.0_f32, COLOR_AMBER))
                                                        .rounding(Rounding::same(2.0))
                                                        .inner_margin(Margin::symmetric(6.0, 2.0))
                                                        .show(ui, |ui| {
                                                            ui.add(
                                                                egui::Label::new(
                                                                    RichText::new(self.lang.t("ĐỀ XUẤT", "RECOMMENDED"))
                                                                        .color(COLOR_AMBER)
                                                                        .size(11.0)
                                                                        .strong(),
                                                                )
                                                                .selectable(false),
                                                            );
                                                        });
                                                }

                                                let display_class = format_display_classification(&b.classification, self.lang);
                                                ui.add(
                                                    egui::Label::new(
                                                        RichText::new(display_class)
                                                            .color(COLOR_TEXT_MAIN)
                                                            .size(13.0)
                                                            .strong(),
                                                    )
                                                    .selectable(false),
                                                );

                                                if b.copy_count > 0 {
                                                    let count_formatted = format_number_with_commas(b.copy_count);
                                                    let copy_text = match self.lang {
                                                        AppLang::Vi => format!("📋 {} lượt copy", count_formatted),
                                                        AppLang::En => format!("📋 {} copies", count_formatted),
                                                    };
                                                    Frame::default()
                                                        .fill(Color32::from_rgb(18, 30, 42))
                                                        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(35, 75, 110)))
                                                        .rounding(Rounding::same(2.0))
                                                        .inner_margin(Margin::symmetric(6.0, 2.0))
                                                        .show(ui, |ui| {
                                                            ui.add(
                                                                egui::Label::new(
                                                                    RichText::new(copy_text)
                                                                        .color(Color32::from_rgb(100, 200, 255))
                                                                        .size(11.0)
                                                                        .strong(),
                                                                )
                                                                .selectable(false),
                                                            );
                                                        });
                                                }

                                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                                    let copy_btn = egui::Button::new(
                                                        RichText::new(self.lang.t("SAO CHÉP", "COPY"))
                                                            .color(COLOR_BG)
                                                            .size(12.0)
                                                            .strong(),
                                                    )
                                                    .fill(COLOR_ACCENT)
                                                    .rounding(Rounding::same(3.0))
                                                    .min_size(Vec2::new(88.0, 26.0));

                                                    if ui.add(copy_btn).clicked() {
                                                        build_to_copy = Some((b.weapon.clone(), b.classification.clone(), b.copy_count, b.loadout_code.clone()));
                                                    }

                                                    ui.add_space(4.0);

                                                    let (fav_label, fav_color, fav_fill) = if is_fav {
                                                        (self.lang.t("★ Đã ghim", "★ Pinned"), Color32::from_rgb(255, 215, 0), Color32::from_rgb(45, 40, 10))
                                                    } else {
                                                        (self.lang.t("☆ Lưu", "☆ Pin"), COLOR_TEXT_MUTED, COLOR_CARD)
                                                    };

                                                    let fav_btn = egui::Button::new(
                                                        RichText::new(fav_label)
                                                            .color(fav_color)
                                                            .size(11.0)
                                                            .strong(),
                                                    )
                                                    .fill(fav_fill)
                                                    .stroke(Stroke::new(1.0_f32, if is_fav { Color32::from_rgb(255, 215, 0) } else { COLOR_BORDER }))
                                                    .rounding(Rounding::same(3.0))
                                                    .min_size(Vec2::new(76.0, 26.0));

                                                    if ui
                                                        .add(fav_btn)
                                                        .on_hover_text(self.lang.t(
                                                            "Ghim bản build này lên đầu và lưu vào favorites.json",
                                                            "Pin this build to top and save to favorites.json"
                                                        ))
                                                        .clicked()
                                                    {
                                                        build_to_toggle_fav = Some(b.clone());
                                                    }

                                                    if b.is_custom {
                                                        ui.add_space(4.0);
                                                        let del_btn = egui::Button::new(
                                                            RichText::new(self.lang.t("🗑 Xóa", "🗑 Delete"))
                                                                .color(Color32::from_rgb(255, 100, 100))
                                                                .size(11.0),
                                                        )
                                                        .fill(Color32::from_rgb(40, 15, 15))
                                                        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(180, 50, 50)))
                                                        .rounding(Rounding::same(3.0))
                                                        .min_size(Vec2::new(60.0, 26.0));

                                                        if ui
                                                            .add(del_btn)
                                                            .on_hover_text(self.lang.t(
                                                                "Xóa mã build cá nhân này",
                                                                "Delete this custom loadout"
                                                            ))
                                                            .clicked()
                                                        {
                                                            code_to_delete = Some(b.loadout_code.clone());
                                                        }
                                                    }
                                                });
                                            });

                                            ui.add_space(8.0);

                                            Frame::default()
                                                .fill(Color32::from_rgb(13, 19, 25))
                                                .stroke(Stroke::new(1.0_f32, Color32::from_rgb(26, 36, 47)))
                                                .rounding(Rounding::same(2.0))
                                                .inner_margin(Margin::symmetric(8.0, 5.0))
                                                .show(ui, |ui| {
                                                    ui.horizontal(|ui| {
                                                        ui.add(
                                                            egui::Label::new(
                                                                RichText::new("CODE:")
                                                                    .color(COLOR_TEXT_MUTED)
                                                                    .size(11.0)
                                                                    .monospace(),
                                                            )
                                                            .selectable(false),
                                                        );
                                                        ui.add(
                                                            egui::Label::new(
                                                                RichText::new(&b.loadout_code)
                                                                    .color(COLOR_TEXT_MAIN)
                                                                    .size(11.0)
                                                                    .monospace(),
                                                            )
                                                            .selectable(true),
                                                        );
                                                    });
                                                });
                                        });

                                    ui.add_space(8.0);
                                }
                            });

                        if let Some((w, c, cnt, code)) = build_to_copy {
                            self.copy_build_code(&w, &c, cnt, &code);
                        }
                        if let Some(b) = build_to_toggle_fav {
                            self.toggle_favorite(&b);
                        }
                        if let Some(code) = code_to_delete {
                            self.delete_custom_build(&code);
                        }
                    }
                } else {
                    ui.vertical_centered(|ui| {
                        ui.add_space(40.0);
                        ui.add(
                            egui::Label::new(
                                RichText::new(self.lang.t("DF LOADOUT SCANNER // SẴN SÀNG", "DF LOADOUT SCANNER // READY"))
                                    .color(COLOR_ACCENT)
                                    .size(20.0)
                                    .strong(),
                            )
                            .selectable(false),
                        );
                        ui.add_space(8.0);
                        let ready_hint = match self.lang {
                            AppLang::Vi => format!("Nhấn phím [{}] khi đang ở giao diện xem chỉ số súng trong game.", self.hotkey_name),
                            AppLang::En => format!("Press [{}] in-game while inspecting weapon stats.", self.hotkey_name),
                        };
                        ui.add(
                            egui::Label::new(
                                RichText::new(ready_hint)
                                    .color(COLOR_TEXT_MAIN)
                                    .size(14.0),
                            )
                            .selectable(false),
                        );
                        ui.add_space(20.0);
                        ui.add(egui::Label::new(RichText::new(self.lang.t("Hoặc chọn nhanh vũ khí từ danh sách:", "Or pick a weapon directly:")).color(COLOR_TEXT_MUTED).size(12.0)).selectable(false));
                        ui.add_space(8.0);

                        ui.horizontal_wrapped(|ui| {
                            for w in self.unique_weapons.iter().take(20) {
                                if ui
                                    .button(RichText::new(w).color(COLOR_TEXT_MAIN).size(12.0))
                                    .clicked()
                                {
                                    self.selected_weapon = Some(w.clone());
                                }
                            }
                        });
                    });
                }
            } else {
                // Favorites & Custom Codes View (All weapons)
                if self.favorites.is_empty() {
                    ui.vertical_centered(|ui| {
                        ui.add_space(60.0);
                        ui.add(
                            egui::Label::new(
                                RichText::new("★")
                                    .color(Color32::from_rgb(120, 105, 40))
                                    .size(48.0),
                            )
                            .selectable(false),
                        );
                        ui.add_space(12.0);
                        ui.add(
                            egui::Label::new(
                                RichText::new(self.lang.t("KHO CẤU HÌNH YÊU THÍCH ĐANG TRỐNG", "FAVORITE LOADOUTS IS EMPTY"))
                                    .color(COLOR_TEXT_MAIN)
                                    .size(16.0)
                                    .strong(),
                            )
                            .selectable(false),
                        );
                        ui.add_space(8.0);
                        ui.add(
                            egui::Label::new(
                                RichText::new(self.lang.t(
                                    "Chưa có cấu hình yêu thích nào. Hãy bấm [☆ Lưu] ở màn hình chính hoặc [+ THÊM MÃ RIÊNG] để lưu tại đây.",
                                    "No favorite builds saved yet. Click [☆ Pin] on the main screen or [+ ADD CUSTOM] to save here."
                                ))
                                .color(COLOR_TEXT_MUTED)
                                .size(13.0),
                            )
                            .selectable(false),
                        );
                        ui.add_space(6.0);
                        ui.add(
                            egui::Label::new(
                                RichText::new(self.lang.t(
                                    "(Bấm phím ESC hoặc nút [← QUAY LẠI] ở trên để quay về màn hình quét)",
                                    "(Press ESC key or [← BACK] button above to return to scanner screen)"
                                ))
                                .color(COLOR_ACCENT)
                                .size(12.0)
                                .monospace(),
                            )
                            .selectable(false),
                        );
                        ui.add_space(20.0);
                        let add_first_btn = egui::Button::new(
                            RichText::new(self.lang.t("+ THÊM MÃ RIÊNG ĐẦU TIÊN", "+ ADD FIRST CUSTOM LOADOUT"))
                                .color(COLOR_BG)
                                .size(12.0)
                                .strong(),
                        )
                        .fill(COLOR_ACCENT)
                        .rounding(Rounding::same(3.0))
                        .min_size(Vec2::new(190.0, 32.0));

                        if ui.add(add_first_btn).clicked() {
                            if self.custom_weapon.is_empty() {
                                self.custom_weapon = self.selected_weapon.clone().unwrap_or_else(|| "M4A1".to_string());
                            }
                            self.custom_code.clear();
                            self.custom_desc.clear();
                            self.custom_error = None;
                            self.show_add_custom_modal = true;
                        }
                    });
                } else {
                    let mut build_to_copy: Option<(String, String, u64, String)> = None;
                    let mut build_to_toggle_fav: Option<BuildItem> = None;
                    let mut code_to_delete: Option<String> = None;
                    let mut jump_to_weapon: Option<String> = None;

                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            for (idx, b) in self.favorites.iter().enumerate() {
                                let is_custom = b.is_custom;

                                Frame::default()
                                    .fill(COLOR_CARD)
                                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(255, 215, 0)))
                                    .rounding(Rounding::same(4.0))
                                    .inner_margin(Margin::same(12.0))
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            ui.add(
                                                egui::Label::new(
                                                    RichText::new(format!("#{}.", idx + 1))
                                                        .color(COLOR_TEXT_MUTED)
                                                        .size(13.0)
                                                        .monospace(),
                                                )
                                                .selectable(false),
                                            );

                                            // Weapon Badge: [M4A1]
                                            Frame::default()
                                                .fill(COLOR_ACCENT_DIM)
                                                .stroke(Stroke::new(1.0_f32, COLOR_ACCENT))
                                                .rounding(Rounding::same(2.0))
                                                .inner_margin(Margin::symmetric(6.0, 2.0))
                                                .show(ui, |ui| {
                                                    ui.add(
                                                        egui::Label::new(
                                                            RichText::new(format!("[{}]", b.weapon))
                                                                .color(COLOR_ACCENT)
                                                                .size(12.0)
                                                                .strong()
                                                                .monospace(),
                                                        )
                                                        .selectable(false),
                                                    );
                                                });

                                            // Type badge: [★ MÃ RIÊNG] or [★ YÊU THÍCH]
                                            Frame::default()
                                                .fill(Color32::from_rgb(50, 45, 10))
                                                .stroke(Stroke::new(1.0_f32, Color32::from_rgb(255, 215, 0)))
                                                .rounding(Rounding::same(2.0))
                                                .inner_margin(Margin::symmetric(6.0, 2.0))
                                                .show(ui, |ui| {
                                                    ui.add(
                                                        egui::Label::new(
                                                            RichText::new(if is_custom {
                                                                self.lang.t("★ MÃ RIÊNG", "★ CUSTOM")
                                                            } else {
                                                                self.lang.t("★ YÊU THÍCH", "★ FAVORITE")
                                                            })
                                                            .color(Color32::from_rgb(255, 215, 0))
                                                            .size(11.0)
                                                            .strong(),
                                                        )
                                                        .selectable(false),
                                                    );
                                                });

                                            // Mode badge if present
                                            if !b.mode.is_empty() {
                                                Frame::default()
                                                    .fill(Color32::from_rgb(20, 30, 40))
                                                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(40, 70, 100)))
                                                    .rounding(Rounding::same(2.0))
                                                    .inner_margin(Margin::symmetric(6.0, 2.0))
                                                    .show(ui, |ui| {
                                                        ui.add(
                                                            egui::Label::new(
                                                                RichText::new(&b.mode)
                                                                    .color(Color32::from_rgb(120, 190, 255))
                                                                    .size(11.0)
                                                                    .strong(),
                                                            )
                                                            .selectable(false),
                                                        );
                                                    });
                                            }

                                            // Classification / Description
                                            let display_class = format_display_classification(&b.classification, self.lang);
                                            ui.add(
                                                egui::Label::new(
                                                    RichText::new(display_class)
                                                        .color(COLOR_TEXT_MAIN)
                                                        .size(13.0)
                                                        .strong(),
                                                )
                                                .selectable(false),
                                            );

                                            if b.copy_count > 0 && !is_custom {
                                                let count_formatted = format_number_with_commas(b.copy_count);
                                                let copy_text = match self.lang {
                                                    AppLang::Vi => format!("📋 {} lượt copy", count_formatted),
                                                    AppLang::En => format!("📋 {} copies", count_formatted),
                                                };
                                                Frame::default()
                                                    .fill(Color32::from_rgb(18, 30, 42))
                                                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(35, 75, 110)))
                                                    .rounding(Rounding::same(2.0))
                                                    .inner_margin(Margin::symmetric(6.0, 2.0))
                                                    .show(ui, |ui| {
                                                        ui.add(
                                                            egui::Label::new(
                                                                RichText::new(copy_text)
                                                                    .color(Color32::from_rgb(100, 200, 255))
                                                                    .size(11.0)
                                                                    .strong(),
                                                            )
                                                            .selectable(false),
                                                        );
                                                    });
                                            }

                                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                                let copy_btn = egui::Button::new(
                                                    RichText::new(self.lang.t("SAO CHÉP", "COPY"))
                                                        .color(COLOR_BG)
                                                        .size(12.0)
                                                        .strong(),
                                                )
                                                .fill(COLOR_ACCENT)
                                                .rounding(Rounding::same(3.0))
                                                .min_size(Vec2::new(88.0, 26.0));

                                                if ui.add(copy_btn).clicked() {
                                                    build_to_copy = Some((b.weapon.clone(), b.classification.clone(), b.copy_count, b.loadout_code.clone()));
                                                }

                                                ui.add_space(4.0);

                                                // "Xem súng" button: jumps to weapon and switches to Scanner tab
                                                let view_btn = egui::Button::new(
                                                    RichText::new(self.lang.t("🔍 Xem súng", "🔍 View Gun"))
                                                        .color(COLOR_TEXT_MAIN)
                                                        .size(11.0),
                                                )
                                                .fill(COLOR_CARD_HOVER)
                                                .stroke(Stroke::new(1.0_f32, COLOR_BORDER))
                                                .rounding(Rounding::same(3.0))
                                                .min_size(Vec2::new(82.0, 26.0));

                                                if ui
                                                    .add(view_btn)
                                                    .on_hover_text(self.lang.t(
                                                        "Chuyển sang màn hình tra cứu của khẩu súng này",
                                                        "Jump to this weapon on the main scanner screen"
                                                    ))
                                                    .clicked()
                                                {
                                                    jump_to_weapon = Some(b.weapon.clone());
                                                }

                                                ui.add_space(4.0);

                                                if is_custom {
                                                    let del_btn = egui::Button::new(
                                                        RichText::new(self.lang.t("🗑 Xóa", "🗑 Delete"))
                                                            .color(Color32::from_rgb(255, 100, 100))
                                                            .size(11.0),
                                                    )
                                                    .fill(Color32::from_rgb(40, 15, 15))
                                                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(180, 50, 50)))
                                                    .rounding(Rounding::same(3.0))
                                                    .min_size(Vec2::new(60.0, 26.0));

                                                    if ui
                                                        .add(del_btn)
                                                        .on_hover_text(self.lang.t(
                                                            "Xóa mã build cá nhân này khỏi danh sách",
                                                            "Delete this custom loadout"
                                                        ))
                                                        .clicked()
                                                    {
                                                        code_to_delete = Some(b.loadout_code.clone());
                                                    }
                                                } else {
                                                    let unpin_btn = egui::Button::new(
                                                        RichText::new(self.lang.t("★ Bỏ lưu", "★ Unpin"))
                                                            .color(Color32::from_rgb(255, 215, 0))
                                                            .size(11.0)
                                                            .strong(),
                                                    )
                                                    .fill(Color32::from_rgb(45, 40, 10))
                                                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(255, 215, 0)))
                                                    .rounding(Rounding::same(3.0))
                                                    .min_size(Vec2::new(76.0, 26.0));

                                                    if ui
                                                        .add(unpin_btn)
                                                        .on_hover_text(self.lang.t(
                                                            "Bỏ ghim bản build này khỏi kho Yêu thích",
                                                            "Unpin this build from Favorites"
                                                        ))
                                                        .clicked()
                                                    {
                                                        build_to_toggle_fav = Some(b.clone());
                                                    }
                                                }
                                            });
                                        });

                                        ui.add_space(8.0);

                                        Frame::default()
                                            .fill(Color32::from_rgb(13, 19, 25))
                                            .stroke(Stroke::new(1.0_f32, Color32::from_rgb(26, 36, 47)))
                                            .rounding(Rounding::same(2.0))
                                            .inner_margin(Margin::symmetric(8.0, 5.0))
                                            .show(ui, |ui| {
                                                ui.horizontal(|ui| {
                                                    ui.add(
                                                        egui::Label::new(
                                                            RichText::new("CODE:")
                                                                .color(COLOR_TEXT_MUTED)
                                                                .size(11.0)
                                                                .monospace(),
                                                        )
                                                        .selectable(false),
                                                    );
                                                    ui.add(
                                                        egui::Label::new(
                                                            RichText::new(&b.loadout_code)
                                                                .color(COLOR_TEXT_MAIN)
                                                                .size(11.0)
                                                                .monospace(),
                                                        )
                                                        .selectable(true),
                                                    );
                                                });
                                            });
                                    });

                                ui.add_space(8.0);
                            }
                        });

                    if let Some((w, c, cnt, code)) = build_to_copy {
                        self.copy_build_code(&w, &c, cnt, &code);
                    }
                    if let Some(b) = build_to_toggle_fav {
                        self.toggle_favorite(&b);
                    }
                    if let Some(code) = code_to_delete {
                        self.delete_custom_build(&code);
                    }
                    if let Some(weapon) = jump_to_weapon {
                        self.selected_weapon = Some(weapon);
                        self.current_tab = AppTab::Scanner;
                    }
                }
            }
            });

        // User Guide Modal Dialog (Bilingual 2-column manual)
        if self.show_guide {
            let mut close_guide = false;
            egui::Window::new(self.lang.t("HƯỚNG DẪN SỬ DỤNG // USER GUIDE", "USER GUIDE // HƯỚNG DẪN"))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .frame(
                    Frame::default()
                        .fill(COLOR_CARD)
                        .stroke(Stroke::new(1.5_f32, COLOR_ACCENT))
                        .rounding(Rounding::same(4.0))
                        .inner_margin(Margin::same(18.0)),
                )
                .open(&mut self.show_guide)
                .show(ctx, |ui| {
                    ui.set_width(620.0);

                    ui.vertical_centered(|ui| {
                        ui.add(
                            egui::Label::new(
                                RichText::new("DF LOADOUT SCANNER // USER MANUAL")
                                    .color(COLOR_ACCENT)
                                    .size(17.0)
                                    .strong()
                                    .monospace(),
                            )
                            .selectable(false),
                        );
                        ui.add_space(2.0);
                        ui.add(
                            egui::Label::new(
                                RichText::new(self.lang.t(
                                    "Hướng dẫn cài đặt và sử dụng công cụ đồng bộ cấu hình súng Delta Force",
                                    "Setup and operating manual for Delta Force Loadout Scanner",
                                ))
                                .color(COLOR_TEXT_MUTED)
                                .size(11.0),
                            )
                            .selectable(false),
                        );
                    });

                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(10.0);

                    ui.columns(2, |cols| {
                        // Left Column: TIẾNG VIỆT
                        cols[0].vertical(|ui| {
                            Frame::default()
                                .fill(COLOR_ACCENT_DIM)
                                .rounding(Rounding::same(2.0))
                                .inner_margin(Margin::symmetric(6.0, 2.0))
                                .show(ui, |ui| {
                                    ui.label(RichText::new("TIẾNG VIỆT").color(COLOR_ACCENT).size(11.0).strong());
                                });
                            ui.add_space(6.0);

                            ui.label(RichText::new("1. Cài đặt hệ thống & game:").color(COLOR_AMBER).size(12.0).strong());
                            ui.label(RichText::new(format!("• Quyền Admin: Ứng dụng tự yêu cầu quyền Administrator để Windows nhận diện phím {} khi cửa sổ game đang mở.", self.hotkey_name)).color(COLOR_TEXT_MAIN).size(11.0));
                            ui.label(RichText::new("• Game: Chuyển màn hình sang Không viền (Borderless Windowed).").color(COLOR_TEXT_MAIN).size(11.0));
                            ui.label(RichText::new("• Windows: Bấm Win + V và chọn BẬT (Turn on) để lưu lịch sử sao chép.").color(COLOR_TEXT_MAIN).size(11.0));

                            ui.add_space(6.0);
                            ui.label(RichText::new("2. Hai cách sử dụng:").color(COLOR_AMBER).size(12.0).strong());
                            ui.label(RichText::new(format!("• Quét tự động ({}): Vào xem bảng chỉ số súng trong game và nhấn {}. Lõi AI PaddleOCR v4 chạy offline (ONNX Runtime CPU) sẽ đọc tiêu đề súng và đẩy cấu hình vào Win + V.", self.hotkey_name, self.hotkey_name)).color(COLOR_TEXT_MAIN).size(11.0));
                            ui.label(RichText::new("• Tra cứu thủ công: Chọn Server, Chế độ và Loại súng trên thanh Bộ Lọc để xem và sao chép bất kỳ súng nào.").color(COLOR_TEXT_MAIN).size(11.0));

                            ui.add_space(6.0);
                            ui.label(RichText::new("3. Tính tương thích & An toàn kỹ thuật:").color(COLOR_AMBER).size(12.0).strong());
                            ui.label(RichText::new("• Được thiết kế để giảm thiểu tối đa can thiệp vào trò chơi: không đọc bộ nhớ (RAM), không tiêm tệp (DLL Injection), không chỉnh sửa tiến trình và không tự động hóa thao tác phím/chuột. Tuy nhiên, tính tương thích với các hệ thống chống gian lận (ACE) không được bảo đảm chính thức từ nhà phát hành.").color(COLOR_TEXT_MUTED).size(11.0));
                        });

                        // Right Column: ENGLISH
                        cols[1].vertical(|ui| {
                            Frame::default()
                                .fill(Color32::from_rgb(26, 37, 50))
                                .rounding(Rounding::same(2.0))
                                .inner_margin(Margin::symmetric(6.0, 2.0))
                                .show(ui, |ui| {
                                    ui.label(RichText::new("ENGLISH").color(COLOR_TEXT_MAIN).size(11.0).strong());
                                });
                            ui.add_space(6.0);

                            ui.label(RichText::new("1. System & Game Setup:").color(COLOR_AMBER).size(12.0).strong());
                            ui.label(RichText::new(format!("• Admin Rights: App requests Administrator privileges so Windows can capture hotkey {} while game is focused.", self.hotkey_name)).color(COLOR_TEXT_MAIN).size(11.0));
                            ui.label(RichText::new("• Game: Set Display Mode to Borderless Windowed.").color(COLOR_TEXT_MAIN).size(11.0));
                            ui.label(RichText::new("• Windows: Press Win + V and click Turn on to enable Clipboard History.").color(COLOR_TEXT_MAIN).size(11.0));

                            ui.add_space(6.0);
                            ui.label(RichText::new("2. Two Operating Modes:").color(COLOR_AMBER).size(12.0).strong());
                            ui.label(RichText::new(format!("• Auto Scan ({}): Inspect weapon stats in-game and press {}. Offline PaddleOCR v4 (ONNX Runtime CPU) reads weapon title and feeds builds (Top copies first) into Win + V.", self.hotkey_name, self.hotkey_name)).color(COLOR_TEXT_MAIN).size(11.0));
                            ui.label(RichText::new("• Manual Lookup: Select Server, Mode, and Weapon Type on the Filter Bar to browse and copy any build.").color(COLOR_TEXT_MAIN).size(11.0));

                            ui.add_space(6.0);
                            ui.label(RichText::new("3. Compatibility & Technical Safety:").color(COLOR_AMBER).size(12.0).strong());
                            ui.label(RichText::new("• Designed to minimize game interference: no game memory access, DLL injection, process modification, or automated gameplay input. Compatibility with anti-cheat systems is not officially guaranteed.").color(COLOR_TEXT_MUTED).size(11.0));
                        });
                    });

                    ui.add_space(14.0);
                    ui.separator();
                    ui.add_space(10.0);

                    ui.vertical_centered(|ui| {
                        let got_it_btn = egui::Button::new(
                            RichText::new(self.lang.t("[ĐÃ HIỂU / GOT IT]", "[GOT IT / ĐÃ HIỂU]"))
                                .color(COLOR_BG)
                                .size(13.0)
                                .strong(),
                        )
                        .fill(COLOR_ACCENT)
                        .rounding(Rounding::same(3.0))
                        .min_size(Vec2::new(160.0, 32.0));

                        if ui.add(got_it_btn).clicked() {
                            close_guide = true;
                        }
                    });
                });

            if close_guide {
                self.show_guide = false;
            }
        }

        // About Modal Dialog
        if self.show_about {
            let mut close_clicked = false;
            egui::Window::new(self.lang.t("THÔNG TIN // ABOUT", "ABOUT // THÔNG TIN"))
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .frame(
                    Frame::default()
                        .fill(COLOR_CARD)
                        .stroke(Stroke::new(1.5_f32, COLOR_ACCENT))
                        .rounding(Rounding::same(4.0))
                        .inner_margin(Margin::same(18.0)),
                )
                .open(&mut self.show_about)
                .show(ctx, |ui| {
                    ui.set_width(520.0);

                    egui::ScrollArea::vertical()
                        .max_height(520.0)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.vertical_centered(|ui| {
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(APP_NAME)
                                            .color(COLOR_ACCENT)
                                            .size(18.0)
                                            .strong()
                                            .monospace(),
                                    )
                                    .selectable(false),
                                );
                                ui.add_space(2.0);
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(format!("{} - Open Source Edition (MIT License)", APP_VERSION))
                                            .color(COLOR_TEXT_MUTED)
                                            .size(12.0)
                                            .monospace(),
                                    )
                                    .selectable(false),
                                );
                            });

                            ui.add_space(10.0);
                            ui.separator();
                            ui.add_space(10.0);

                            egui::Grid::new("about_grid")
                                .num_columns(2)
                                .spacing([14.0, 8.0])
                                .show(ui, |ui| {
                                    ui.add(egui::Label::new(RichText::new(self.lang.t("Phát triển bởi:", "Developed by:")).color(COLOR_TEXT_MUTED).size(12.0)).selectable(false));
                                    ui.add(egui::Label::new(RichText::new(AUTHOR_NAME).color(COLOR_TEXT_MAIN).size(12.0).strong()).selectable(false));
                                    ui.end_row();

                                    ui.add(egui::Label::new(RichText::new(self.lang.t("Chế độ chơi:", "Game Modes:")).color(COLOR_TEXT_MUTED).size(12.0)).selectable(false));
                                    ui.add(egui::Label::new(RichText::new("Warfare & Operations").color(COLOR_ACCENT).size(12.0)).selectable(false));
                                    ui.end_row();

                                    ui.add(egui::Label::new(RichText::new(self.lang.t("Khu vực máy chủ:", "Server Regions:")).color(COLOR_TEXT_MUTED).size(12.0)).selectable(false));
                                    ui.add(egui::Label::new(RichText::new("Garena & Infinity (Global)").color(COLOR_TEXT_MAIN).size(12.0)).selectable(false));
                                    ui.end_row();

                                    ui.add(egui::Label::new(RichText::new(self.lang.t("Cơ sở dữ liệu:", "Database:")).color(COLOR_TEXT_MUTED).size(12.0)).selectable(false));
                                    ui.add(egui::Label::new(RichText::new(format!("{} builds / {} weapons", self.builds.len(), self.unique_weapons.len())).color(COLOR_TEXT_MAIN).size(12.0)).selectable(false));
                                    ui.end_row();
                                });

                            ui.add_space(8.0);
                            ui.separator();
                            ui.add_space(8.0);

                            // Section: Nguồn dữ liệu tham khảo cộng đồng (Data Sources Attribution)
                            ui.add(
                                egui::Label::new(
                                    RichText::new(self.lang.t("Nguồn dữ liệu tham khảo cộng đồng (Community Data Sources):", "Community Data Sources & Attribution:"))
                                        .color(COLOR_TEXT_MUTED)
                                        .size(11.0)
                                        .strong(),
                                )
                                .selectable(false),
                            );
                            ui.add_space(3.0);
                            ui.label(RichText::new(self.lang.t(
                                "Dữ liệu cấu hình súng được tổng hợp từ 3 cổng cộng đồng trực tuyến nhằm mục đích tra cứu:",
                                "Weapon loadout configurations are aggregated from 3 community portals for reference:"
                            )).color(COLOR_TEXT_MUTED).size(11.0));
                            ui.add_space(3.0);
                            Frame::default()
                                .fill(Color32::from_rgb(13, 19, 25))
                                .stroke(Stroke::new(1.0_f32, Color32::from_rgb(26, 36, 47)))
                                .rounding(Rounding::same(2.0))
                                .inner_margin(Margin::symmetric(10.0, 6.0))
                                .show(ui, |ui| {
                                    ui.vertical(|ui| {
                                        ui.horizontal(|ui| {
                                            ui.label(RichText::new("• DeltaForceTools:").color(COLOR_ACCENT).size(11.0).strong());
                                            ui.label(RichText::new("https://deltaforcetools.gg").color(COLOR_TEXT_MAIN).size(11.0).monospace());
                                        });
                                        ui.add_space(2.0);
                                        ui.horizontal(|ui| {
                                            ui.label(RichText::new("• DeltaForceBuilds:").color(COLOR_ACCENT).size(11.0).strong());
                                            ui.label(RichText::new("https://deltaforcebuilds.com").color(COLOR_TEXT_MAIN).size(11.0).monospace());
                                        });
                                        ui.add_space(2.0);
                                        ui.horizontal(|ui| {
                                            ui.label(RichText::new("• FPSMeta:").color(COLOR_ACCENT).size(11.0).strong());
                                            ui.label(RichText::new("https://fpsmeta.com (fpsmeta.gg)").color(COLOR_TEXT_MAIN).size(11.0).monospace());
                                        });
                                    });
                                });

                            ui.add_space(8.0);
                            ui.separator();
                            ui.add_space(8.0);

                            // Section: Kênh cập nhật chính thức
                            ui.add(
                                egui::Label::new(
                                    RichText::new(self.lang.t("Kênh cập nhật dữ liệu mùa mới (Khóa an toàn):", "Official Seasonal Update Channel (Locked):"))
                                        .color(COLOR_TEXT_MUTED)
                                        .size(11.0)
                                        .strong(),
                                )
                                .selectable(false),
                            );
                            ui.add_space(3.0);
                            Frame::default()
                                .fill(Color32::from_rgb(13, 19, 25))
                                .stroke(Stroke::new(1.0_f32, Color32::from_rgb(26, 36, 47)))
                                .rounding(Rounding::same(2.0))
                                .inner_margin(Margin::symmetric(8.0, 5.0))
                                .show(ui, |ui| {
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(OFFICIAL_UPDATE_URL)
                                                .color(COLOR_ACCENT)
                                                .size(10.5)
                                                .monospace(),
                                        )
                                        .selectable(true),
                                    );
                                });

                            ui.add_space(8.0);
                            ui.separator();
                            ui.add_space(8.0);

                            // Section: Tuyên bố pháp lý & Miễn trừ trách nhiệm
                            ui.add(
                                egui::Label::new(
                                    RichText::new(self.lang.t("Tuyên bố pháp lý & Bản quyền thương hiệu:", "Legal Disclaimer & Trademarks:"))
                                        .color(COLOR_TEXT_MUTED)
                                        .size(11.0)
                                        .strong(),
                                )
                                .selectable(false),
                            );
                            ui.add_space(3.0);
                            ui.add(
                                egui::Label::new(
                                    RichText::new(self.lang.t(
                                        "Phần mềm được phát hành mã nguồn mở hoàn toàn (MIT License) và miễn phí 100% cho cộng đồng — mọi khoản ủng hộ (trà tắc/cà phê) từ người dùng đều hoàn toàn tự nguyện và không khóa bất kỳ tính năng nào. Hoạt động hoàn toàn độc lập với Team Jade / TiMi Studio Group / Garena và các trang web nguồn. Tiện ích hoạt động thuần túy bằng chụp màn hình, nhận diện chữ offline qua PaddleOCR v4 (ONNX Runtime CPU) và Clipboard hệ thống, không chứa mã nguồn hay tài sản game.",
                                        "The software is 100% free and Open Source (MIT License) for the community — any support/donations (tea/coffee) from users are entirely voluntary and do not lock any features. Completely independent from Team Jade / TiMi Studio Group / Garena and source websites. Operates strictly via screen capture, offline PaddleOCR v4 (ONNX Runtime CPU) recognition, and system Clipboard, containing no game code or proprietary assets."
                                    ))
                                    .color(COLOR_TEXT_MUTED)
                                    .size(11.0),
                                )
                                .selectable(false),
                            );

                            ui.add_space(8.0);
                            ui.separator();
                            ui.add_space(8.0);

                            // Section: Kênh tiếp nhận khiếu nại & Gỡ bỏ dữ liệu (Takedown Policy)
                            ui.add(
                                egui::Label::new(
                                    RichText::new(self.lang.t("Tiếp nhận khiếu nại & Gỡ bỏ dữ liệu (Takedown / Opt-out):", "Notice & Takedown Policy (Opt-out):"))
                                        .color(COLOR_AMBER)
                                        .size(11.0)
                                        .strong(),
                                )
                                .selectable(false),
                            );
                            ui.add_space(3.0);
                            ui.add(
                                egui::Label::new(
                                    RichText::new(self.lang.t(
                                        "Chủ sở hữu các trang web nguồn có thể yêu cầu gỡ bỏ nguồn tương ứng bất cứ lúc nào qua GitHub Issues (https://github.com/TaiGiap/df-loadout-scanner/issues) hoặc gửi email tới tai.gd361@gmail.com. Yêu cầu hợp lệ sẽ được rà soát và gỡ bỏ hoàn toàn trong vòng 24–48 giờ.",
                                        "Source website owners may request content removal at any time via GitHub Issues (https://github.com/TaiGiap/df-loadout-scanner/issues) or by contacting tai.gd361@gmail.com. Verified requests will be reviewed and removed within 24-48 hours."
                                    ))
                                    .color(COLOR_TEXT_MUTED)
                                    .size(11.0),
                                )
                                .selectable(false),
                            );

                            ui.add_space(14.0);

                            ui.vertical_centered(|ui| {
                                let close_btn = egui::Button::new(
                                    RichText::new(self.lang.t("[ĐÓNG]", "[CLOSE]"))
                                        .color(COLOR_BG)
                                        .size(12.0)
                                        .strong(),
                                )
                                .fill(COLOR_ACCENT)
                                .rounding(Rounding::same(3.0))
                                .min_size(Vec2::new(110.0, 28.0));

                                if ui.add(close_btn).clicked() {
                                    close_clicked = true;
                                }
                            });
                        });
                });

            if close_clicked {
                self.show_about = false;
            }
        }

        // Support / Donate Modal Dialog
        if self.show_support {
            if self.support_qr_texture.is_none() {
                self.update_support_qr(ctx);
            }

            let mut is_open = true;
            let mut close_clicked = false;
            let mut trigger_amount: Option<u64> = None;
            let mut copied_stk = false;

            let support_title = self.lang.t("ỦNG HỘ TÁC GIẢ // SUPPORT ME", "SUPPORT ME // ỦNG HỘ TÁC GIẢ");
            egui::Window::new(support_title)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .frame(
                    Frame::default()
                        .fill(COLOR_CARD)
                        .stroke(Stroke::new(1.5_f32, Color32::from_rgb(255, 105, 180)))
                        .rounding(Rounding::same(6.0))
                        .inner_margin(Margin::same(16.0)),
                )
                .open(&mut is_open)
                .show(ctx, |ui| {
                    ui.set_width(450.0);

                    egui::ScrollArea::vertical()
                        .max_height(540.0)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.vertical_centered(|ui| {
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(self.lang.t("❤ HỖ TRỢ TÁC GIẢ ❤", "❤ SUPPORT THE DEVELOPER ❤"))
                                            .color(Color32::from_rgb(255, 105, 180))
                                            .size(17.0)
                                            .strong(),
                                    )
                                    .selectable(false),
                                );
                                ui.add_space(3.0);
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(self.lang.t(
                                            "\"Nếu thấy phần mềm hữu ích hãy hỗ trợ tôiiiiii\"",
                                            "\"If you find this app helpful, please support meeeeee!\""
                                        ))
                                        .color(COLOR_AMBER)
                                        .size(13.0)
                                        .italics(),
                                    )
                                    .selectable(false),
                                );
                            });

                            ui.add_space(8.0);
                            ui.separator();
                            ui.add_space(8.0);

                            // Account Information Frame
                            Frame::default()
                                .fill(Color32::from_rgb(13, 19, 25))
                                .stroke(Stroke::new(1.0_f32, Color32::from_rgb(30, 45, 60)))
                                .rounding(Rounding::same(4.0))
                                .inner_margin(Margin::symmetric(14.0, 8.0))
                                .show(ui, |ui| {
                                    egui::Grid::new("donate_info_grid")
                                        .num_columns(2)
                                        .spacing([12.0, 6.0])
                                        .show(ui, |ui| {
                                            ui.label(RichText::new(self.lang.t("Ngân hàng:", "Bank:")).color(COLOR_TEXT_MUTED).size(12.0));
                                            ui.label(RichText::new(DONATE_BANK_NAME).color(COLOR_TEXT_MAIN).size(12.0).strong());
                                            ui.end_row();

                                            ui.label(RichText::new(self.lang.t("Chủ tài khoản:", "Account Name:")).color(COLOR_TEXT_MUTED).size(12.0));
                                            ui.label(RichText::new(DONATE_ACCOUNT_NAME_DISPLAY).color(COLOR_ACCENT).size(12.0).strong());
                                            ui.end_row();

                                            ui.label(RichText::new(self.lang.t("Số tài khoản:", "Account No:")).color(COLOR_TEXT_MUTED).size(12.0));
                                            ui.horizontal(|ui| {
                                                ui.label(RichText::new(DONATE_ACCOUNT_NO_DISPLAY).color(Color32::from_rgb(255, 215, 0)).size(13.0).monospace().strong());
                                                ui.add_space(4.0);
                                                let copy_btn = egui::Button::new(
                                                    RichText::new(self.lang.t("[Copy STK]", "[Copy Account]"))
                                                        .size(10.5)
                                                        .color(COLOR_BG)
                                                        .strong()
                                                )
                                                .fill(COLOR_ACCENT)
                                                .rounding(Rounding::same(2.0));
                                                if ui.add(copy_btn).clicked() {
                                                    copied_stk = true;
                                                }
                                            });
                                            ui.end_row();
                                        });
                                });

                            ui.add_space(10.0);

                            // Presets Row
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(self.lang.t("Mốc nhanh:", "Quick presets:")).color(COLOR_TEXT_MUTED).size(12.0));
                                let presets = [
                                    (1000, "1.000đ", "1,000 VND"),
                                    (5000, "5.000đ", "5,000 VND"),
                                    (10000, "10.000đ", "10,000 VND"),
                                ];
                                for (amt, label_vi, label_en) in presets {
                                    let label = match self.lang {
                                        AppLang::Vi => label_vi,
                                        AppLang::En => label_en,
                                    };
                                    let is_sel = self.support_amount == amt;
                                    let btn = egui::Button::new(
                                        RichText::new(label)
                                            .color(if is_sel { COLOR_BG } else { COLOR_TEXT_MAIN })
                                            .size(11.5)
                                            .strong(),
                                    )
                                    .fill(if is_sel { Color32::from_rgb(255, 105, 180) } else { Color32::from_rgb(25, 35, 45) })
                                    .stroke(Stroke::new(1.0_f32, if is_sel { Color32::from_rgb(255, 105, 180) } else { COLOR_BORDER }))
                                    .rounding(Rounding::same(3.0));

                                    if ui.add(btn).clicked() {
                                        self.support_amount = amt;
                                        self.support_custom_amount_text = amt.to_string();
                                        trigger_amount = Some(amt);
                                    }
                                }
                            });

                            ui.add_space(6.0);

                            // Custom amount input
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(self.lang.t("Tùy ý:", "Custom:")).color(COLOR_TEXT_MUTED).size(12.0));
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.support_custom_amount_text)
                                        .hint_text("VNĐ...")
                                        .desired_width(100.0)
                                );
                                self.support_custom_amount_text.retain(|c| c.is_ascii_digit());

                                let gen_btn = egui::Button::new(
                                    RichText::new(self.lang.t("[Tạo mã QR]", "[Generate QR]"))
                                        .color(COLOR_BG)
                                        .size(11.0)
                                        .strong(),
                                )
                                .fill(COLOR_ACCENT)
                                .rounding(Rounding::same(3.0));

                                if ui.add(gen_btn).clicked() {
                                    let parsed = self.support_custom_amount_text.parse::<u64>().unwrap_or(0);
                                    if parsed > 0 {
                                        self.support_amount = parsed;
                                        trigger_amount = Some(parsed);
                                    }
                                }
                            });

                            ui.add_space(8.0);

                            // Thank you message
                            let thank_msg = match self.support_amount {
                                1000 => self.lang.t("Cảm ơn bạn đã cho tôi 4 xu delta ❤", "Thank you for giving me 4 Delta Coins! ❤"),
                                5000 => self.lang.t("Cảm ơn bạn đã cho tôi 20 xu delta ❤", "Thank you for giving me 20 Delta Coins! ❤"),
                                10000 => self.lang.t("Cảm ơn bạn đã bao tôi ly trà tắc ❤", "Thank you for treating me to an iced kumquat tea! ❤"),
                                _ => self.lang.t("Cảm ơn bạn rất nhiều vì đã tiếp thêm động lực cho tôi! ❤", "Thank you so much for keeping me motivated! ❤"),
                            };

                            ui.vertical_centered(|ui| {
                                Frame::default()
                                    .fill(Color32::from_rgb(35, 18, 28))
                                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(180, 60, 100)))
                                    .rounding(Rounding::same(4.0))
                                    .inner_margin(Margin::symmetric(10.0, 5.0))
                                    .show(ui, |ui| {
                                        ui.add(
                                            egui::Label::new(
                                                RichText::new(thank_msg)
                                                    .color(Color32::from_rgb(255, 140, 180))
                                                    .size(12.5)
                                                    .strong(),
                                            )
                                            .selectable(false),
                                        );
                                    });
                            });

                            ui.add_space(10.0);

                            // VietQR Image Display (100% Offline, sharp pixels)
                            ui.vertical_centered(|ui| {
                                if let Some(texture) = &self.support_qr_texture {
                                    let img_size = Vec2::new(240.0, 240.0);
                                    ui.image((texture.id(), img_size));
                                } else {
                                    ui.add_space(40.0);
                                    ui.label(RichText::new("Chưa có mã QR").color(COLOR_TEXT_MUTED));
                                    ui.add_space(40.0);
                                }
                            });

                            ui.add_space(8.0);
                            ui.separator();
                            ui.add_space(8.0);

                            // Footer: Open in Browser + Close
                            let qr_url = format!(
                                "https://img.vietqr.io/image/{}-{}-compact2.png?amount={}&addInfo=Ung%20ho%20DF%20Loadout%20Scanner&accountName={}",
                                DONATE_BANK_ID,
                                DONATE_ACCOUNT_NO,
                                self.support_amount,
                                DONATE_ACCOUNT_NAME
                            );

                            ui.horizontal(|ui| {
                                let browser_btn = egui::Button::new(
                                    RichText::new(self.lang.t("🌐 Mở link VietQR dự phòng", "🌐 Open Backup VietQR Link"))
                                        .color(COLOR_TEXT_MAIN)
                                        .size(11.0),
                                )
                                .stroke(Stroke::new(1.0_f32, COLOR_BORDER))
                                .rounding(Rounding::same(3.0));

                                if ui.add(browser_btn).clicked() {
                                    ctx.open_url(egui::OpenUrl::new_tab(&qr_url));
                                }

                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    let close_btn = egui::Button::new(
                                        RichText::new(self.lang.t("[ĐÓNG]", "[CLOSE]"))
                                            .color(COLOR_BG)
                                            .size(11.5)
                                            .strong(),
                                    )
                                    .fill(Color32::from_rgb(255, 105, 180))
                                    .rounding(Rounding::same(3.0))
                                    .min_size(Vec2::new(80.0, 24.0));

                                    if ui.add(close_btn).clicked() {
                                        close_clicked = true;
                                    }
                                });
                            });
                        });
                });

            if copied_stk {
                let copy_res = match init_clipboard_with_retry(3) {
                    Ok(mut cb) => copy_to_clipboard_with_retry(&mut cb, DONATE_ACCOUNT_NO, 3),
                    Err(e) => Err(e),
                };
                match copy_res {
                    Ok(()) => {
                        unsafe { let _ = MessageBeep(MB_OK); }
                        self.status_message = Some((
                            self.lang.t("Đã copy số tài khoản Techcombank!", "Copied Techcombank account number!").to_string(),
                            Instant::now(),
                            false,
                        ));
                    }
                    Err(err) => {
                        unsafe { let _ = MessageBeep(MB_ICONERROR); }
                        self.status_message = Some((
                            format!("{}: {}", self.lang.t("Lỗi ghi Clipboard", "Clipboard error"), err),
                            Instant::now(),
                            true,
                        ));
                    }
                }
            }

            if !is_open || close_clicked {
                self.show_support = false;
            }

            if let Some(amt) = trigger_amount {
                self.support_amount = amt;
                self.update_support_qr(ctx);
            }
        }

        // Add Custom Loadout Code Modal Dialog
        if self.show_add_custom_modal {
            let mut is_open = true;
            let mut save_clicked = false;
            let mut close_clicked = false;

            let title = self.lang.t("THÊM MÃ CẤU HÌNH CÁ NHÂN", "ADD CUSTOM LOADOUT CODE");
            egui::Window::new(title)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .frame(
                    Frame::default()
                        .fill(COLOR_CARD)
                        .stroke(Stroke::new(1.5_f32, COLOR_ACCENT))
                        .rounding(Rounding::same(6.0))
                        .inner_margin(Margin::same(16.0)),
                )
                .open(&mut is_open)
                .show(ctx, |ui| {
                    ui.set_width(420.0);
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new(self.lang.t(
                                "Lưu mã súng cá nhân của bạn vào favorites.json. Mã này luôn được ghim lên đầu và không bao giờ bị ghi đè khi cập nhật DB.",
                                "Save your custom loadout code to favorites.json. It will always rank at top and never be overwritten by DB updates."
                            ))
                            .color(COLOR_TEXT_MUTED)
                            .size(11.0),
                        );
                        ui.add_space(10.0);

                        // Weapon input
                        ui.label(RichText::new(self.lang.t("Tên vũ khí:", "Weapon Name:")).color(COLOR_TEXT_MAIN).size(12.0).strong());
                        ui.add(egui::TextEdit::singleline(&mut self.custom_weapon).hint_text("ví dụ: M4A1, MP5, G3...").desired_width(f32::INFINITY));
                        ui.add_space(8.0);

                        // Mode input
                        ui.label(RichText::new(self.lang.t("Chế độ chơi:", "Game Mode:")).color(COLOR_TEXT_MAIN).size(12.0).strong());
                        ui.horizontal(|ui| {
                            ui.selectable_value(&mut self.custom_mode, "Warfare".to_string(), "Warfare");
                            ui.selectable_value(&mut self.custom_mode, "Operations".to_string(), "Operations");
                        });
                        ui.add_space(8.0);

                        // Description input
                        ui.label(RichText::new(self.lang.t("Mô tả / Phân loại:", "Description / Classification:")).color(COLOR_TEXT_MAIN).size(12.0).strong());
                        ui.add(egui::TextEdit::singleline(&mut self.custom_desc).hint_text("ví dụ: Laser beam / Giật thấp / Đấu rank...").desired_width(f32::INFINITY));
                        ui.add_space(8.0);

                        // Loadout code input
                        ui.label(RichText::new(self.lang.t("Mã cấu hình (Loadout Code):", "Loadout Code:")).color(COLOR_TEXT_MAIN).size(12.0).strong());
                        ui.add(egui::TextEdit::singleline(&mut self.custom_code).hint_text("Mã súng Delta Force trong game...").desired_width(f32::INFINITY));
                        ui.add_space(8.0);

                        if let Some(err) = &self.custom_error {
                            ui.label(RichText::new(err).color(COLOR_AMBER).size(11.0));
                            ui.add_space(6.0);
                        }

                        ui.horizontal(|ui| {
                            let save_btn = egui::Button::new(
                                RichText::new(self.lang.t("💾 LƯU MÃ", "💾 SAVE CODE"))
                                    .color(COLOR_BG)
                                    .size(12.0)
                                    .strong(),
                            )
                            .fill(COLOR_ACCENT)
                            .rounding(Rounding::same(3.0))
                            .min_size(Vec2::new(100.0, 28.0));

                            if ui.add(save_btn).clicked() {
                                save_clicked = true;
                            }

                            let cancel_btn = egui::Button::new(
                                RichText::new(self.lang.t("HỦY", "CANCEL"))
                                    .color(COLOR_TEXT_MUTED)
                                    .size(12.0),
                            )
                            .fill(COLOR_CARD)
                            .rounding(Rounding::same(3.0))
                            .min_size(Vec2::new(80.0, 28.0));

                            if ui.add(cancel_btn).clicked() {
                                close_clicked = true;
                            }
                        });
                    });
                });

            if !is_open || close_clicked {
                self.show_add_custom_modal = false;
                self.custom_error = None;
            }

            if save_clicked {
                let weapon_trimmed = self.custom_weapon.trim().to_string();
                let code_trimmed = self.custom_code.trim().to_string();
                let desc_trimmed = self.custom_desc.trim().to_string();

                if weapon_trimmed.is_empty() {
                    self.custom_error = Some(self.lang.t("Vui lòng nhập tên vũ khí!", "Please enter weapon name!").to_string());
                } else if code_trimmed.len() < 18 || code_trimmed.matches('-').count() < 2 {
                    self.custom_error = Some(self.lang.t(
                        "Mã súng không hợp lệ! Độ dài tối thiểu 18 ký tự và chứa ít nhất 2 dấu gạch ngang '-'",
                        "Invalid loadout code! Must be >= 18 characters and contain at least 2 '-' dashes"
                    ).to_string());
                } else {
                    let item = BuildItem {
                        server: self.selected_server.as_str().to_string(),
                        mode: self.custom_mode.clone(),
                        weapon_type: String::new(),
                        weapon: weapon_trimmed.clone(),
                        classification: if desc_trimmed.is_empty() {
                            self.lang.t("Cá nhân / Custom", "Custom Build").to_string()
                        } else {
                            desc_trimmed
                        },
                        tags: vec!["Custom".to_string(), "Favorite".to_string()],
                        copy_count: 999_999,
                        loadout_code: code_trimmed,
                        is_custom: true,
                        is_favorite: true,
                    };

                    self.favorites.retain(|f| f.loadout_code != item.loadout_code);
                    self.favorites.insert(0, item);
                    let save_res = save_favorites(&self.favorites);
                    self.sync_shared_favorites();

                    match save_res {
                        Ok(()) => {
                            self.selected_weapon = Some(weapon_trimmed.clone());
                            self.show_add_custom_modal = false;
                            self.custom_error = None;
                            self.custom_code.clear();
                            self.custom_desc.clear();

                            let msg = match self.lang {
                                AppLang::Vi => format!("Đã lưu mã cá nhân cho [{}] vào favorites.json!", weapon_trimmed),
                                AppLang::En => format!("Saved custom loadout for [{}] to favorites.json!", weapon_trimmed),
                            };
                            self.status_message = Some((msg, Instant::now(), false));
                        }
                        Err(err) => {
                            let err_msg = match self.lang {
                                AppLang::Vi => format!("Lỗi lưu favorites.json: {}", err),
                                AppLang::En => format!("Failed to save favorites.json: {}", err),
                            };
                            self.custom_error = Some(err_msg.clone());
                            self.status_message = Some((err_msg, Instant::now(), true));
                        }
                    }
                }
            }
        }
    }
}

fn load_app_icon() -> egui::IconData {
    let rgba = include_bytes!("../assets/icon_32.rgba").to_vec();
    egui::IconData {
        rgba,
        width: 32,
        height: 32,
    }
}

fn main() -> eframe::Result<()> {
    let icon = load_app_icon();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1140.0, 680.0])
            .with_resizable(false)
            .with_maximize_button(false)
            .with_title(APP_NAME)
            .with_icon(icon),
        ..Default::default()
    };

    eframe::run_native(
        "df_loadout_scanner",
        options,
        Box::new(|cc| Ok(Box::new(LoadoutTerminalApp::new(cc)))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get_test_candidates() -> Vec<String> {
        let items: Vec<BuildItem> = serde_json::from_str(BUILDS_JSON).expect("valid builds json");
        let mut set = std::collections::HashSet::new();
        for b in items {
            set.insert(b.weapon);
        }
        let mut list: Vec<String> = set.into_iter().collect();
        list.sort();
        list
    }

    #[test]
    fn test_target_weapon_recognition() {
        let candidates = get_test_candidates();

        let test_cases = [
            ("G3 Battle Rifle", "G3"),
            ("M249 Light Machine Gun", "M249"),
            ("PKM General Machine Gun", "PKM"),
            ("M14 Marksman Rifle", "M14"),
            ("R93 Sniper Rifle", "R93"),
            ("725 Double-Barrel Shotgun", "725 DOUBLE-BARREL"),
            ("Compound Bow", "COMPOUND BOW"),
        ];

        for (raw_ocr, expected) in test_cases {
            let (matched, cleaned) = recognize_weapon_from_title(raw_ocr, &candidates);
            assert!(
                matched.is_some(),
                "Failed to match '{}' (cleaned: '{}')",
                raw_ocr,
                cleaned
            );
            let (weapon, score) = matched.unwrap();
            assert_eq!(
                weapon, expected,
                "For raw OCR '{}', expected '{}' but got '{}' with score {:.2}",
                raw_ocr, expected, weapon, score
            );
        }
    }

    #[test]
    fn test_no_false_identification_mini14_m16a4() {
        let candidates = get_test_candidates();

        // M249 must NOT be identified as MINI-14
        let (matched_m249, _) = recognize_weapon_from_title("M249 Light Machine Gun", &candidates);
        let matched_m249 = matched_m249.unwrap();
        assert_eq!(matched_m249.0, "M249");
        assert_ne!(matched_m249.0, "MINI-14");

        // M14 must NOT be identified as M16A4
        let (matched_m14, _) = recognize_weapon_from_title("M14 Marksman Rifle", &candidates);
        let matched_m14 = matched_m14.unwrap();
        assert_eq!(matched_m14.0, "M14");
        assert_ne!(matched_m14.0, "M16A4");
    }

    #[test]
    fn test_ocr_rejects_vietnamese_aliases_and_category_only() {
        let candidates = get_test_candidates();

        // 1. General Vietnamese nicknames (`cung`, `no`, `sung truong`, etc.) MUST remain disabled in OCR flow,
        //    except for explicit HUD titles `Cung Phức Hợp` -> `COMPOUND BOW` and `Shotgun Hai Nòng` -> `725 DOUBLE-BARREL`.
        for vi_nick in [
            "cung",
            "Cung",
            "Nỏ",
            "Nỏ chiến thuật",
            "sung truong",
            "Súng trường",
            "tieu lien",
            "Bắn tỉa",
        ] {
            let (matched, _) = recognize_weapon_from_title(vi_nick, &candidates);
            assert!(
                matched.is_none(),
                "OCR must ignore Vietnamese aliases, but '{}' matched {:?}",
                vi_nick,
                matched
            );
        }

        for bow_title in ["Cung Phức Hợp", "Cung Phuc Hop", "Jng Phurc Hop"] {
            let d = recognize_weapon_with_decision(bow_title, 0.90, None, &candidates);
            assert_eq!(d.weapon, Some("COMPOUND BOW"), "Title '{}' should map to COMPOUND BOW", bow_title);
            assert!(d.can_auto_copy, "Title '{}' should satisfy auto-copy guards", bow_title);
        }

        for db_title in ["Shotgun Hai Nòng", "Shotgun Hai Nong U", "Hai Nòng", "Hai Nong...", "HAI N0NG U"] {
            let d = recognize_weapon_with_decision(db_title, 0.88, None, &candidates);
            assert_eq!(
                d.weapon,
                Some("725 DOUBLE-BARREL"),
                "Title '{}' should map to 725 DOUBLE-BARREL",
                db_title
            );
            assert!(d.can_auto_copy, "Title '{}' should satisfy auto-copy guards", db_title);
        }

        // 2. Category-only OCR text with no weapon code characters MUST be rejected
        for cat_only in [
            "Assault Rifle",
            "Battle Rifle",
            "Compact Assault Rifle",
            "Submachine Gun",
            "Marksman Rifle",
            "Sniper Rifle",
            "Light Machine Gun",
            "General Machine Gun",
            "Shotgun",
            "Pistol",
            "Revolver",
            "... Assault Rifle",
            "18+ Battle Rifle",
            ".. Submachine Gun",
        ] {
            let (matched, cleaned) = recognize_weapon_from_title(cat_only, &candidates);
            assert!(
                matched.is_none(),
                "Category-only text '{}' (cleaned: '{}') must be rejected, got {:?}",
                cat_only,
                cleaned,
                matched
            );
        }
    }

    #[test]
    fn test_category_abbreviations_and_homoglyphs() {
        let candidates = get_test_candidates();

        let cases = [
            ("M249 LMG", "M249"),
            ("G3 BR", "G3"),
            ("M14 DMR", "M14"),
            ("PKM LMG", "PKM"),
            ("R93 SR", "R93"),
            ("AK-I2", "AK-12"),
        ];

        for (raw_ocr, expected) in cases {
            let (matched, cleaned) = recognize_weapon_from_title(raw_ocr, &candidates);
            assert!(
                matched.is_some(),
                "Failed to match '{}' (cleaned: '{}')",
                raw_ocr,
                cleaned
            );
            let (weapon, _) = matched.unwrap();
            assert_eq!(
                weapon, expected,
                "For OCR '{}', expected '{}' but got '{}'",
                raw_ocr, expected, weapon
            );
        }
    }

    #[test]
    fn test_v136_error_cases_and_skin_stripping() {
        let candidates = get_test_candidates();

        let cases = [
            // 1. G3
            ("G3 Battle Rifle", "G3"),
            ("Chơi quá 180 phút G3 Battle Rifle", "G3"),
            ("qua G3 Battle Rifle", "G3"),
            // 2. P90
            ("P90 Submachine Gun", "P90"),
            ("18+ Chơi quá 180 phút một ngày P90", "P90"),
            ("P90", "P90"),
            // 3. AR-57
            ("AR-57 SMG", "AR-57"),
            ("AR57 SMG", "AR-57"),
            ("AR57", "AR-57"),
            // 4. PSG-1
            ("PSG-1 Marksman Rifle - Nghệ Thuật Đương Đại", "PSG-1"),
            ("PSG1 Marksman Rifle", "PSG-1"),
            ("PEG-1 Marksman Rifle", "PSG-1"),
            ("PEG1", "PSG-1"),
            // 5. M82
            ("M82 Sniper Rifle", "M82"),
            ("882 Sniper Rifle", "M82"),
            ("B82 Sniper Rifle", "M82"),
            ("M82A1 Sniper Rifle", "M82"),
            // 6. M7
            ("M7 Battle Rifle - Đỏ", "M7"),
            ("M7 Battle Rifle", "M7"),
            ("180 phút M7 Battle Rifle", "M7"),
            // 7. MK4
            ("MK4 SMG", "MK4"),
            ("MK4 Submachine Gun", "MK4"),
            ("MK4", "MK4"),
            // 8. R93
            ("R93 Sniper Rifle", "R93"),
            ("R93", "R93"),
            // 9. QBZ95-1
            ("TEZG5-1 Assault Rifle", "QBZ95-1"),
            ("QBZ951 Assault Rifle", "QBZ95-1"),
            ("QBZ-95-1 Assault Rifle", "QBZ95-1"),
            ("QBZ95-1", "QBZ95-1"),
            // 10. SG552
            ("ECS52 Assault Rifle", "SG552"),
            ("SG-552 Assault Rifle", "SG552"),
            ("SG552", "SG552"),
            // 11. CI-19
            ("CL1G Compact Assault Rifle", "CI-19"),
            ("C1-19 Compact Assault Rifle", "CI-19"),
            ("CL-19 Compact Assault Rifle", "CI-19"),
            ("CI19 Compact Assault Rifle", "CI-19"),
            ("CI-19", "CI-19"),
            // 12. SR-3M
            ("SR-3M Assault Rifle", "SR-3M"),
            ("SR3M Assault Rifle", "SR-3M"),
            ("SR-3M", "SR-3M"),
            // 13. VECTOR
            ("CVECTOR SMG", "VECTOR"),
            ("VECTOR SMG", "VECTOR"),
            ("VECTOR", "VECTOR"),
            // 14. 725 DOUBLE-BARREL
            ("725 Double-Barrel - Sa Mạc", "725 DOUBLE-BARREL"),
            ("Double-Barrel", "725 DOUBLE-BARREL"),
            ("725", "725 DOUBLE-BARREL"),
        ];

        for (raw_ocr, expected) in cases {
            let (matched, cleaned) = recognize_weapon_from_title(raw_ocr, &candidates);
            assert!(
                matched.is_some(),
                "Failed to match '{}' (cleaned: '{}')",
                raw_ocr,
                cleaned
            );
            let (weapon, _) = matched.unwrap();
            assert_eq!(
                weapon, expected,
                "For OCR '{}', expected '{}' but got '{}'",
                raw_ocr, expected, weapon
            );
        }
    }

    #[test]
    fn test_strict_jaro_winkler_rejection() {
        // Unrelated garbage text should not match weapons
        let candidates = get_test_candidates();
        let cleaned = clean_ocr_text("RANDOM TEXT THAT SHOULD NOT MATCH");
        let matched = match_weapon(&cleaned, &candidates);
        assert!(matched.is_none(), "Expected no match but got: {:?}", matched);
    }

    #[test]
    fn test_additional_weapons_coverage() {
        let candidates = get_test_candidates();

        let cases = [
            ("K416 Assault Rifle", "K416"),
            ("M4A1 Assault Rifle", "M4A1"),
            ("AK-12 Assault Rifle", "AK-12"),
            ("CI-19 Compact Assault Rifle", "CI-19"),
            ("Desert Eagle Pistol", "DESERT EAGLE"),
            ("FS-12 Shotgun", "FS-12"),
            ("S12K Shotgun", "S12K"),
            ("M1014 Shotgun", "M1014"),
            ("M870 Shotgun", "M870"),
            ("AWM Sniper Rifle", "AWM"),
            ("M700 Sniper Rifle", "M700"),
            ("SV-98 Sniper Rifle", "SV-98"),
            ("SVD Sniper Rifle", "SVD"),
            ("SVCH Marksman Rifle", "SVCH"),
            ("SKS Marksman Rifle", "SKS"),
            ("VSS Marksman Rifle", "VSS"),
            ("MP5 SMG", "MP5"),
            ("MP7 Submachine Gun", "MP7"),
            ("P90 Submachine Gun", "P90"),
            ("Vector SMG", "VECTOR"),
            ("Uzi SMG", "UZI"),
            ("Bizon SMG", "BIZON"),
            ("Vityaz SMG", "VITYAZ"),
            ("QCQ171 SMG", "QCQ171"),
            ("QJB-201 Light Machine Gun", "QJB-201"),
            ("M250 Light Machine Gun", "M250"),
            ("CAR-15 Carbine", "CAR-15"),
            ("M16A4 Assault Rifle", "M16A4"),
            ("SCAR-H Battle Rifle", "SCAR-H"),
            ("MK47 Assault Rifle", "MK47"),
            ("PTR-32 Assault Rifle", "PTR-32"),
            ("SR-25 Marksman Rifle", "SR-25"),
            ("PSG-1 Marksman Rifle", "PSG-1"),
            ("Mini-14 Marksman Rifle", "MINI-14"),
            ("G-17 Pistol", "G-17"),
            ("G18 Pistol", "G18"),
            ("M1911 Pistol", "M1911"),
            (".357 Revolver", ".357"),
            ("93R Pistol", "93R"),
            ("M82 Sniper Rifle", "M82"),
            ("M7 Battle Rifle", "M7"),
            ("MDR Assault Rifle", "MDR"),
            ("RM277 Assault Rifle", "RM277"),
            ("SG552 Assault Rifle", "SG552"),
            ("SR-3M Assault Rifle", "SR-3M"),
            ("SR9 SMG", "SR9"),
            ("MK4 SMG", "MK4"),
            ("AR-57 SMG", "AR-57"),
            ("SMG-45 SMG", "SMG-45"),
            ("QBZ95-1 Assault Rifle", "QBZ95-1"),
            ("ASH-12 Assault Rifle", "ASH-12"),
            ("K437 Assault Rifle", "K437"),
            ("KC-17 Assault Rifle", "KC-17"),
            ("MCX LT Assault Rifle", "MCX LT"),
        ];

        for (raw_ocr, expected) in cases {
            let cleaned = clean_ocr_text(raw_ocr);
            let matched = match_weapon(&cleaned, &candidates);
            assert!(
                matched.is_some(),
                "Failed to match '{}' (cleaned: '{}')",
                raw_ocr,
                cleaned
            );
            let (weapon, _) = matched.unwrap();
            assert_eq!(
                weapon, expected,
                "For OCR '{}', expected '{}' but got '{}'",
                raw_ocr, expected, weapon
            );
        }
    }

    #[test]
    fn test_source_normalized_ranking() {
        let b_dftools_high = BuildItem {
            server: "Garena".into(),
            mode: "Warfare".into(),
            weapon_type: "Assault Rifle".into(),
            weapon: "AK-12".into(),
            classification: "DFTools, Cấu hình đề xuất".into(),
            tags: vec!["DFTools".into()],
            copy_count: 1000,
            loadout_code: "code-dftools-high".into(),
            is_custom: false,
            is_favorite: false,
        };
        let b_dftools_mid = BuildItem {
            server: "Garena".into(),
            mode: "Warfare".into(),
            weapon_type: "Assault Rifle".into(),
            weapon: "AK-12".into(),
            classification: "DFTools".into(),
            tags: vec!["DFTools".into()],
            copy_count: 500,
            loadout_code: "code-dftools-mid".into(),
            is_custom: false,
            is_favorite: false,
        };
        let b_dftools_low = BuildItem {
            server: "Garena".into(),
            mode: "Warfare".into(),
            weapon_type: "Assault Rifle".into(),
            weapon: "AK-12".into(),
            classification: "DFTools".into(),
            tags: vec!["DFTools".into()],
            copy_count: 100,
            loadout_code: "code-dftools-low".into(),
            is_custom: false,
            is_favorite: false,
        };
        let b_dfbuilds_meta = BuildItem {
            server: "Garena".into(),
            mode: "Warfare".into(),
            weapon_type: "Assault Rifle".into(),
            weapon: "AK-12".into(),
            classification: "DFBuilds, Meta Build".into(),
            tags: vec!["DFBuilds".into(), "Meta".into()],
            copy_count: 0,
            loadout_code: "code-dfbuilds-meta".into(),
            is_custom: false,
            is_favorite: false,
        };
        let b_dfbuilds_normal = BuildItem {
            server: "Garena".into(),
            mode: "Warfare".into(),
            weapon_type: "Assault Rifle".into(),
            weapon: "AK-12".into(),
            classification: "DFBuilds".into(),
            tags: vec!["DFBuilds".into()],
            copy_count: 0,
            loadout_code: "code-dfbuilds-normal".into(),
            is_custom: false,
            is_favorite: false,
        };
        let b_fpsmeta_rec = BuildItem {
            server: "Garena".into(),
            mode: "Warfare".into(),
            weapon_type: "Assault Rifle".into(),
            weapon: "AK-12".into(),
            classification: "FPSMeta, Cấu hình đề xuất".into(),
            tags: vec!["FPSMeta".into(), "Đề xuất".into()],
            copy_count: 0,
            loadout_code: "code-fpsmeta-rec".into(),
            is_custom: false,
            is_favorite: false,
        };

        let slice = vec![
            &b_dftools_low,
            &b_dfbuilds_normal,
            &b_dftools_mid,
            &b_fpsmeta_rec,
            &b_dftools_high,
            &b_dfbuilds_meta,
        ];

        let sorted = sort_builds_prioritized(&slice, &HashSet::new());
        let codes: Vec<&str> = sorted.iter().map(|b| b.loadout_code.as_str()).collect();

        // 1. Meta tier (tier 0): b_dfbuilds_meta must rank #1 even with copy_count 0
        assert_eq!(codes[0], "code-dfbuilds-meta");

        // 2. Recommended tier (tier 1):
        //    - b_dftools_high (score 1.0)
        //    - b_fpsmeta_rec (baseline score 0.65)
        assert_eq!(codes[1], "code-dftools-high");
        assert_eq!(codes[2], "code-fpsmeta-rec");

        // 3. Normal tier (tier 2):
        //    - b_dfbuilds_normal (baseline score 0.65) must come BEFORE b_dftools_mid (score 0.50) and low (0.10)
        assert_eq!(codes[3], "code-dfbuilds-normal");
        assert_eq!(codes[4], "code-dftools-mid");
        assert_eq!(codes[5], "code-dftools-low");
    }

    #[test]
    fn test_favorites_management_and_priority() {
        let b_meta = BuildItem {
            server: "Garena".into(),
            mode: "Warfare".into(),
            weapon_type: "Assault Rifle".into(),
            weapon: "M4A1".into(),
            classification: "Meta Build".into(),
            tags: vec!["Meta".into()],
            copy_count: 5000,
            loadout_code: "code-m4a1-meta".into(),
            is_custom: false,
            is_favorite: false,
        };
        let b_normal_pinned = BuildItem {
            server: "Garena".into(),
            mode: "Warfare".into(),
            weapon_type: "Assault Rifle".into(),
            weapon: "M4A1".into(),
            classification: "Normal Build".into(),
            tags: vec![],
            copy_count: 10,
            loadout_code: "code-m4a1-pinned".into(),
            is_custom: false,
            is_favorite: true,
        };
        let b_custom = BuildItem {
            server: "Garena".into(),
            mode: "Warfare".into(),
            weapon_type: "Assault Rifle".into(),
            weapon: "M4A1".into(),
            classification: "Custom Laser".into(),
            tags: vec!["Custom".into()],
            copy_count: 999_999,
            loadout_code: "code-m4a1-custom".into(),
            is_custom: true,
            is_favorite: true,
        };

        let mut fav_set = HashSet::new();
        fav_set.insert("code-m4a1-pinned".to_string());
        fav_set.insert("code-m4a1-custom".to_string());

        let slice = vec![&b_meta, &b_normal_pinned, &b_custom];
        let sorted = sort_builds_prioritized(&slice, &fav_set);
        let codes: Vec<&str> = sorted.iter().map(|b| b.loadout_code.as_str()).collect();

        // Favorite & custom builds must be ranked at Tier -1, above Meta (Tier 0)
        assert_eq!(codes[0], "code-m4a1-custom");
        assert_eq!(codes[1], "code-m4a1-pinned");
        assert_eq!(codes[2], "code-m4a1-meta");
    }

    #[test]
    fn test_confidence_threshold_categorization() {
        // High confidence tier (>= 0.90) -> Auto copy
        assert_eq!(classify_confidence(1.0), ConfidenceTier::High);
        assert_eq!(classify_confidence(0.95), ConfidenceTier::High);
        assert_eq!(classify_confidence(0.90), ConfidenceTier::High);

        // Suspicious confidence tier (0.75 <= score < 0.90) -> Confirm before copy
        assert_eq!(classify_confidence(0.899), ConfidenceTier::Suspicious);
        assert_eq!(classify_confidence(0.85), ConfidenceTier::Suspicious);
        assert_eq!(classify_confidence(0.75), ConfidenceTier::Suspicious);

        // Rejected tier (< 0.75) -> Unrecognized
        assert_eq!(classify_confidence(0.749), ConfidenceTier::Rejected);
        assert_eq!(classify_confidence(0.60), ConfidenceTier::Rejected);
        assert_eq!(classify_confidence(0.10), ConfidenceTier::Rejected);
    }

    #[test]
    fn test_weapon_search_matching_and_aliases() {
        let candidates = vec![
            "725 DOUBLE-BARREL".to_string(),
            "COMPOUND BOW".to_string(),
            "M4A1".to_string(),
            "AR-57".to_string(),
            "MINI-14".to_string(),
            "P90".to_string(),
            "G3".to_string(),
            "DESERT EAGLE".to_string(),
            "G-17".to_string(),
            "MARLIN LEVER-ACTION".to_string(),
        ];

        // 1. Vietnamese alias without diacritics
        assert_eq!(
            find_best_matching_weapon(&candidates, "hai nong"),
            Some(&"725 DOUBLE-BARREL".to_string())
        );
        assert_eq!(
            find_best_matching_weapon(&candidates, "cung"),
            Some(&"COMPOUND BOW".to_string())
        );

        // 2. Vietnamese alias with diacritics
        assert_eq!(
            find_best_matching_weapon(&candidates, "hai nòng"),
            Some(&"725 DOUBLE-BARREL".to_string())
        );
        assert_eq!(
            find_best_matching_weapon(&candidates, "cung phức hợp"),
            Some(&"COMPOUND BOW".to_string())
        );
        assert_eq!(
            find_best_matching_weapon(&candidates, "cần lever"),
            Some(&"MARLIN LEVER-ACTION".to_string())
        );

        // 3. Compact / Dash-less queries
        assert_eq!(
            find_best_matching_weapon(&candidates, "ar57"),
            Some(&"AR-57".to_string())
        );
        assert_eq!(
            find_best_matching_weapon(&candidates, "mini14"),
            Some(&"MINI-14".to_string())
        );
        assert_eq!(
            find_best_matching_weapon(&candidates, "g17"),
            Some(&"G-17".to_string())
        );

        // 4. Nicknames
        assert_eq!(
            find_best_matching_weapon(&candidates, "deagle"),
            Some(&"DESERT EAGLE".to_string())
        );
        assert_eq!(
            find_best_matching_weapon(&candidates, "glock"),
            Some(&"G-17".to_string())
        );
        assert_eq!(
            find_best_matching_weapon(&candidates, "725"),
            Some(&"725 DOUBLE-BARREL".to_string())
        );

        // 5. Prefix / Standard
        assert_eq!(
            find_best_matching_weapon(&candidates, "m4"),
            Some(&"M4A1".to_string())
        );
        assert_eq!(
            find_best_matching_weapon(&candidates, "g3"),
            Some(&"G3".to_string())
        );
        assert_eq!(
            find_best_matching_weapon(&candidates, "p90"),
            Some(&"P90".to_string())
        );

        // 6. Unmatched query returns None
        assert_eq!(find_best_matching_weapon(&candidates, "xyz123"), None);
    }

    #[test]
    fn test_startup_weapon_selection() {
        let items: Vec<BuildItem> = serde_json::from_str(BUILDS_JSON).expect("valid builds json");
        let candidates = get_test_candidates();

        // 1. Under default Garena + Warfare, M4A1 should be selected
        let selected = determine_startup_weapon(&candidates, &items, ServerFilter::Garena, ModeFilter::Warfare);
        assert_eq!(selected, Some("M4A1".to_string()));

        // 2. Under Infinity + Warfare, M4A1 should be selected
        let selected_inf = determine_startup_weapon(&candidates, &items, ServerFilter::Infinity, ModeFilter::Warfare);
        assert_eq!(selected_inf, Some("M4A1".to_string()));

        // 3. Fallback scenario when M4A1 is not in candidate list
        let filtered_candidates: Vec<String> = candidates.into_iter().filter(|w| w != "M4A1").collect();
        let fallback = determine_startup_weapon(&filtered_candidates, &items, ServerFilter::Garena, ModeFilter::Warfare);
        assert!(fallback.is_some());
        let fallback_weapon = fallback.unwrap();
        assert_ne!(fallback_weapon, "M4A1");
        assert!(items.iter().any(|b| b.weapon == fallback_weapon && ServerFilter::Garena.matches(&b.server)));
    }

    #[test]
    fn test_occluded_suffix_plus_category_matching() {
        let candidates = get_test_candidates();

        // 1. Multi-char (>= 2 chars) occluded suffix + category resolves at High confidence (>= 0.90)
        let occluded_multi_cases = [
            ("...K4 Submachine Gun", "MK4"),
            ("K4 Submachine Gun", "MK4"),
            ("...93 Sniper Rifle", "R93"),
            ("93 Sniper Rifle", "R93"),
            ("...ector Submachine Gun", "VECTOR"),
            ("ector Submachine Gun", "VECTOR"),
            ("...R-3M Assault Rifle", "SR-3M"),
            ("...R-3M Compact Assault Rifle", "SR-3M"),
            ("R-3M Assault Rifle", "SR-3M"),
            ("...BZ95-1 Assault Rifle", "QBZ95-1"),
            ("...Z95-1 Assault Rifle", "QBZ95-1"),
            ("95-1 Assault Rifle", "QBZ95-1"),
        ];

        for (raw_ocr, expected) in occluded_multi_cases {
            let (matched, cleaned) = recognize_weapon_from_title(raw_ocr, &candidates);
            assert!(
                matched.is_some(),
                "Failed suffix+category match for '{}' (cleaned: '{}')",
                raw_ocr,
                cleaned
            );
            let (weapon, score) = matched.unwrap();
            assert_eq!(
                weapon, expected,
                "Occluded '{}' (cleaned: '{}'): expected '{}' got '{}'",
                raw_ocr, cleaned, expected, weapon
            );
            assert!(
                score >= 0.90,
                "Unique occluded suffix+category '{}' -> '{}' should have high confidence (>= 0.90), got {:.2}",
                raw_ocr,
                weapon,
                score
            );
            assert_eq!(classify_confidence(score), ConfidenceTier::High);
        }

        // 1b. 1-character occluded code (`...7 Battle Rifle`) resolves M7 but MUST cap <= 0.82 (confirm required)
        for single_char_case in ["...7 Battle Rifle", "7 Battle Rifle", ".7 Battle Rifle"] {
            let (matched, cleaned) = recognize_weapon_from_title(single_char_case, &candidates);
            let (weapon, score) = matched.unwrap_or_else(|| {
                panic!("Expected M7 confirm match for '{}' (cleaned: '{}')", single_char_case, cleaned)
            });
            assert_eq!(weapon, "M7");
            assert!(
                score <= 0.82,
                "1-char code '{}' must never auto-copy (expected <= 0.82, got {:.3})",
                single_char_case,
                score
            );
            assert_eq!(classify_confidence(score), ConfidenceTier::Suspicious);
        }

        // 2. Ambiguous 2-match suffix (`...14 Marksman Rifle` matches M14 & MINI-14) MUST block Auto-Copy (`margin < 0.12`)
        let amb_dec = recognize_weapon_with_decision("...14 Marksman Rifle", 1.0, None, &candidates);
        let amb_w = amb_dec.weapon.expect("ambiguous suffix should surface top candidate for confirm");
        assert!(amb_w == "M14" || amb_w == "MINI-14");
        assert!(amb_dec.margin < 0.12, "M14 vs MINI-14 margin should be < 0.12, got {:.3}", amb_dec.margin);
        assert!(!amb_dec.can_auto_copy, "Close margin < 0.12 must block Auto-Copy and require confirmation");

        // 3. Broad ambiguous 1-char suffix in large pool (`...2 Assault Rifle` matches AK-12, PTR-32, SG552, ASH-12) must be rejected
        let (too_broad, _) = recognize_weapon_from_title("...2 Assault Rifle", &candidates);
        assert!(
            too_broad.is_none(),
            "3+ way ambiguous suffix should be rejected, got {:?}",
            too_broad
        );

        // 4. Category-only text with no weapon code must always return None
        for cat_only in [
            "Assault Rifle",
            "Battle Rifle",
            "Submachine Gun",
            "Marksman Rifle",
            "Sniper Rifle",
            "Light Machine Gun",
            "Shotgun",
            "Pistol",
            "... Battle Rifle",
            "--- Submachine Gun",
        ] {
            let (m, _) = recognize_weapon_from_title(cat_only, &candidates);
            assert!(
                m.is_none(),
                "Category-only string '{}' must be rejected, got {:?}",
                cat_only,
                m
            );
        }
    }

    #[test]
    fn test_v201_sticky_suffix_16_9_aspect_and_ctc_safety() {
        let candidates = get_test_candidates();

        // 1. Sticky trailing HUD noise + 19 real Warfare occluded / noisy strings MUST resolve with Auto-Copy
        let warfare_cases = [
            ("4A1Assault Rifle", "M4A1"),
            ("3552 Assault Rifle", "SG552"),
            ("S552 Assault Rifleof oadu", "SG552"),
            ("3Z95-1Assault Rifle", "QBZ95-1"),
            ("QZ95-1 Assault Rifle", "QBZ95-1"),
            ("317 Assault Rifle", "KC-17"),
            ("137 Assault Rifle", "K437"),
            ("116 Assault Rifle", "K416"),
            ("JG Assault Rifle", "AUG"),
            ("K47 Assault Rifle", "MK47"),
            ("57 Assault Rifle", "AR-57"),
            ("AR-57 Assault Rifle", "AR-57"),
            ("tomoson Submachine Gun", "THOMPSON"),
            ("20171 Submachine Gun", "QCQ171"),
            ("aver-action Rifle", "MARLIN LEVER-ACTION"),
            ("SS Marksman Rifle", "VSS"),
            ("7CH Marksman Rifle", "SVCH"),
            ("7D Sniger Rifle", "SVD"),
            ("-98 Sniner Rifle", "SV-98"),
            ("VM Sniger Rifle", "AWM"),
            ("Jng Phurc Hop", "COMPOUND BOW"),
        ];

        for (raw_ocr, expected) in warfare_cases {
            let dec = recognize_weapon_with_decision(raw_ocr, 0.88, None, &candidates);
            assert_eq!(
                dec.weapon,
                Some(expected),
                "Warfare OCR '{}' (cleaned: '{}'): expected '{}', got {:?}",
                raw_ocr,
                dec.cleaned,
                expected,
                dec.weapon
            );
            assert!(
                dec.score >= 0.75,
                "Warfare OCR '{}' -> '{}' expected score >= 0.75, got {:.3}",
                raw_ocr,
                expected,
                dec.score
            );
            assert!(
                dec.can_auto_copy,
                "Warfare OCR '{}' -> '{}' must pass all 3 auto-copy guards (score={:.3}, margin={:.3}, locked={}, len={}, ctc={:.3})",
                raw_ocr,
                expected,
                dec.score,
                dec.margin,
                dec.category_locked,
                dec.code_alnum_len,
                dec.ctc_confidence
            );
        }

        // Verify K47 specifically prefers MK47 over K437 inside Assault Rifle lock
        let k47_dec = recognize_weapon_with_decision("K47 Assault Rifle", 0.90, None, &candidates);
        assert_eq!(k47_dec.weapon, Some("MK47"));
        assert!(k47_dec.margin >= 0.12);

        // Hard Category Lock: unmatched code inside a locked category must NEVER escape to another category
        let locked_miss = recognize_weapon_with_decision("57 Sniper Rifle", 0.95, None, &candidates);
        assert_ne!(locked_miss.weapon, Some("AR-57"));
        assert!(!locked_miss.can_auto_copy);

        // 2. Three mandatory Auto-Copy >= 75% safety guards
        // When score >= 0.80, margin requirement relaxes to >= 0.05; below 0.80 keeps >= 0.12
        assert_eq!(evaluate_auto_copy_guards(0.78, 0.20, true, 3, 0.85), ConfidenceTier::High);
        assert_eq!(evaluate_auto_copy_guards(0.78, 0.11, true, 3, 0.95), ConfidenceTier::Suspicious);
        assert_eq!(evaluate_auto_copy_guards(0.95, 0.08, true, 3, 0.95), ConfidenceTier::High);
        assert_eq!(evaluate_auto_copy_guards(0.95, 0.04, true, 3, 0.95), ConfidenceTier::Suspicious);
        assert_eq!(evaluate_auto_copy_guards(0.74, 0.20, true, 3, 0.85), ConfidenceTier::Rejected);
        assert_eq!(evaluate_auto_copy_guards(0.95, 0.50, false, 3, 0.95), ConfidenceTier::Suspicious);
        assert_eq!(evaluate_auto_copy_guards(0.95, 0.50, true, 1, 0.95), ConfidenceTier::Suspicious);
        assert_eq!(evaluate_auto_copy_guards(0.95, 0.50, true, 3, 0.54), ConfidenceTier::Suspicious);

        // v2.0.2-beta Warfare real-world cases: prefix-category `Shotgun S12KU` -> `S12K` & smooth >= 75% auto-copy
        for (raw_ocr, expected) in [
            ("Shotgun S12KU", "S12K"),
            ("S12KU Shotgun", "S12K"),
            ("CAR-15 Assault Rifle", "CAR-15"),
            ("G3 Battle Rifle", "G3"),
            ("SVD Sniper Rifle", "SVD"),
            ("R93 Sniper Rifle", "R93"),
        ] {
            let dec = recognize_weapon_with_decision(raw_ocr, 0.85, None, &candidates);
            assert_eq!(dec.weapon, Some(expected), "Failed for '{}'", raw_ocr);
            assert!(dec.score >= 0.75, "Score < 0.75 for '{}': {:.3}", raw_ocr, dec.score);
            assert!(
                dec.can_auto_copy,
                "Expected auto-copy true for '{}' (score={:.3}, margin={:.3})",
                raw_ocr,
                dec.score,
                dec.margin
            );
        }

        // End-to-end guard enforcement: no category -> blocks auto-copy; low CTC -> blocks auto-copy; 1-char code -> blocks auto-copy
        let unlocked = recognize_weapon_with_decision("SG552", 0.99, None, &candidates);
        assert_eq!(unlocked.weapon, Some("SG552"));
        assert!(!unlocked.can_auto_copy, "Unlocked category must require manual confirmation");

        let low_ctc = recognize_weapon_with_decision("S552 Assault Rifleof oadu", 0.45, None, &candidates);
        assert_eq!(low_ctc.weapon, Some("SG552"));
        assert!(low_ctc.score <= 0.85);
        assert!(!low_ctc.can_auto_copy);

        let single_char = recognize_weapon_with_decision("7 Battle Rifle", 0.99, None, &candidates);
        assert_eq!(single_char.weapon, Some("M7"));
        assert!(single_char.score <= 0.82);
        assert!(!single_char.can_auto_copy);

        // 3. Centered 16:9 HUD ROI calculation across 16:9 (1920x1080), 21:9 (3440x1440), and 16:10 (1920x1200)
        // Tight center header ROI: x: 24.0% -> 41.5%, y: 15.0% -> 19.6%
        let r_1080 = compute_hud_16_9_roi_rect(1920, 1080, 0.240, 0.415, 0.150, 0.196);
        assert_eq!(r_1080, (461, 162, 336, 50));

        // 3440x1440 (21:9 Ultrawide): virtual_w=2560, offset_x=440
        let r_uw = compute_hud_16_9_roi_rect(3440, 1440, 0.240, 0.415, 0.150, 0.196);
        assert_eq!(r_uw, (1054, 216, 448, 66));

        // 1920x1200 (16:10): virtual_w=1920, offset_x=0
        let r_1610 = compute_hud_16_9_roi_rect(1920, 1200, 0.240, 0.415, 0.150, 0.196);
        assert_eq!(r_1610, (461, 180, 336, 55));
    }

    #[test]
    fn test_center_roi_and_yellow_18plus_preprocessing() {
        // Threshold-free RGB preprocessing must preserve dimmed shadowed 18+ gray tones and keep background dark
        let mut img = xcap::image::RgbaImage::from_pixel(240, 48, xcap::image::Rgba([18, 22, 28, 255]));
        // Dimmed semi-transparent 18+-shadowed glyphs in left 30% (x = 12..56)
        for y in 14..32 {
            for x in 12..56 {
                img.put_pixel(x, y, xcap::image::Rgba([105, 108, 112, 255]));
            }
        }
        // Bright white text glyph simulation on the clean right side (x = 90..190)
        for y in 14..32 {
            for x in 90..190 {
                img.put_pixel(x, y, xcap::image::Rgba([248, 248, 250, 255]));
            }
        }

        let (target_w, nchw) = preprocess_for_paddle_ocr(&img);
        assert!(target_w >= 320 && target_w % 8 == 0);
        assert_eq!(nchw.len(), 3 * 48 * target_w);
        // Dark HUD background stays dark (< -0.7), bright text stays positive (> 0.8),
        // AND mid-tone shadowed gray is preserved intact (-0.30..0.0) without threshold erasure
        assert!(nchw[..48 * target_w].iter().any(|&v| v < -0.75));
        assert!(nchw[..48 * target_w].iter().any(|&v| v > 0.85));
        assert!(nchw[..48 * target_w].iter().any(|&v| v > -0.30 && v < 0.05));

        // Pass 1B gamma boost (0.55) on left 30% lifts dimmed [105,108,112] significantly,
        // keeps dark background dark (<= original 18), and leaves right 70% untouched.
        let boosted = boost_left_region_gamma(&img, 0.30, 0.55);
        assert!(
            boosted.get_pixel(30, 20)[0] > 175,
            "Expected boosted shadowed gray > 175, got {}",
            boosted.get_pixel(30, 20)[0]
        );
        assert!(boosted.get_pixel(5, 5)[0] <= img.get_pixel(5, 5)[0]);
        assert_eq!(boosted.get_pixel(150, 20)[0], img.get_pixel(150, 20)[0]);
    }

    #[test]
    fn test_paddle_ocr_onnx_end_to_end() {
        #[repr(C)]
        #[allow(non_snake_case)]
        struct BITMAPINFOHEADER {
            biSize: u32,
            biWidth: i32,
            biHeight: i32,
            biPlanes: u16,
            biBitCount: u16,
            biCompression: u32,
            biSizeImage: u32,
            biXPelsPerMeter: i32,
            biYPelsPerMeter: i32,
            biClrUsed: u32,
            biClrImportant: u32,
        }
        #[repr(C)]
        #[allow(non_snake_case)]
        struct BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER,
            bmiColors: [u32; 1],
        }
        #[repr(C)]
        struct RECT {
            left: i32,
            top: i32,
            right: i32,
            bottom: i32,
        }
        #[link(name = "gdi32")]
        extern "system" {
            fn CreateCompatibleDC(hdc: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
            fn DeleteDC(hdc: *mut std::ffi::c_void) -> i32;
            fn CreateDIBSection(
                hdc: *mut std::ffi::c_void,
                pbmi: *const BITMAPINFO,
                usage: u32,
                ppv_bits: *mut *mut std::ffi::c_void,
                h_section: *mut std::ffi::c_void,
                offset: u32,
            ) -> *mut std::ffi::c_void;
            fn SelectObject(hdc: *mut std::ffi::c_void, h: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
            fn DeleteObject(ho: *mut std::ffi::c_void) -> i32;
            fn CreateFontW(
                c_height: i32,
                c_width: i32,
                c_escapement: i32,
                c_orientation: i32,
                c_weight: i32,
                b_italic: u32,
                b_underline: u32,
                b_strikeout: u32,
                i_charset: u32,
                i_out_precision: u32,
                i_clip_precision: u32,
                i_quality: u32,
                i_pitch_and_family: u32,
                psz_face_name: *const u16,
            ) -> *mut std::ffi::c_void;
            fn SetBkMode(hdc: *mut std::ffi::c_void, mode: i32) -> i32;
            fn SetTextColor(hdc: *mut std::ffi::c_void, color: u32) -> u32;
        }
        #[link(name = "user32")]
        extern "system" {
            fn DrawTextW(
                hdc: *mut std::ffi::c_void,
                lpch_text: *const u16,
                cch_text: i32,
                lprc: *mut RECT,
                format: u32,
            ) -> i32;
        }

        let render_hud_title = |title: &str, draw_yellow_18_box: bool| -> xcap::image::RgbaImage {
            let w: i32 = 520;
            let h: i32 = 64;
            let mut img = xcap::image::RgbaImage::from_pixel(w as u32, h as u32, xcap::image::Rgba([20, 26, 34, 255]));
            unsafe {
                let hdc = CreateCompatibleDC(std::ptr::null_mut());
                let bmi = BITMAPINFO {
                    bmiHeader: BITMAPINFOHEADER {
                        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                        biWidth: w,
                        biHeight: -h,
                        biPlanes: 1,
                        biBitCount: 32,
                        biCompression: 0,
                        biSizeImage: 0,
                        biXPelsPerMeter: 0,
                        biYPelsPerMeter: 0,
                        biClrUsed: 0,
                        biClrImportant: 0,
                    },
                    bmiColors: [0],
                };
                let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
                let hbm = CreateDIBSection(hdc, &bmi, 0, &mut bits, std::ptr::null_mut(), 0);
                let old_bm = SelectObject(hdc, hbm);

                let face: Vec<u16> = "Arial\0".encode_utf16().collect();
                let hfont = CreateFontW(34, 0, 0, 0, 700, 0, 0, 0, 1, 0, 0, 4, 0, face.as_ptr());
                let old_font = SelectObject(hdc, hfont);
                SetBkMode(hdc, 1); // TRANSPARENT
                SetTextColor(hdc, 0x00F8F8F8); // Bright white

                let wide: Vec<u16> = title.encode_utf16().collect();
                let mut rc = RECT {
                    left: 16,
                    top: 12,
                    right: w - 8,
                    bottom: h - 4,
                };
                DrawTextW(hdc, wide.as_ptr(), wide.len() as i32, &mut rc, 0x20); // DT_SINGLELINE

                let pixel_slice = std::slice::from_raw_parts(bits as *const u8, (w * h * 4) as usize);
                for y in 0..h as u32 {
                    for x in 0..w as u32 {
                        let off = ((y * w as u32 + x) * 4) as usize;
                        let b = pixel_slice[off];
                        let g = pixel_slice[off + 1];
                        let r = pixel_slice[off + 2];
                        if r > 20 || g > 20 || b > 20 {
                            img.put_pixel(x, y, xcap::image::Rgba([r.max(20), g.max(26), b.max(34), 255]));
                        }
                    }
                }

                SelectObject(hdc, old_font);
                DeleteObject(hfont);
                SelectObject(hdc, old_bm);
                DeleteObject(hbm);
                DeleteDC(hdc);
            }

            if draw_yellow_18_box {
                // Draw bright yellow-orange 18+ warning box on the left edge (x = 0..24)
                for y in 4..(h as u32 - 4) {
                    for x in 0..24 {
                        img.put_pixel(x, y, xcap::image::Rgba([255, 195, 25, 255]));
                    }
                }
            }
            img
        };

        let mut engine = PaddleOcrEngine::new().expect("onnxruntime.dll and PaddleOCR v4 ONNX should initialize");
        let candidates = get_test_candidates();

        let synthetic_cases = [
            ("M4A1 Assault Rifle", "M4A1", false),
            ("M7 Battle Rifle", "M7", true),
            ("R93 Sniper Rifle", "R93", true),
            ("MK4 Submachine Gun", "MK4", false),
            ("Vector Submachine Gun", "VECTOR", false),
            ("SR-3M Assault Rifle", "SR-3M", false),
            ("QBZ95-1 Assault Rifle", "QBZ95-1", false),
        ];

        for (title, expected, with_yellow_overlay) in synthetic_cases {
            let img = render_hud_title(title, with_yellow_overlay);
            let (raw_ocr, ctc_conf) = engine.recognize(&img).expect("PaddleOCR inference should succeed");
            let (matched, cleaned) = recognize_weapon_from_title_with_conf(&raw_ocr, ctc_conf, &candidates);
            assert!(
                matched.is_some(),
                "PaddleOCR failed on '{}' -> raw_ocr: '{}', cleaned: '{}', ctc_conf: {:.3}",
                title,
                raw_ocr,
                cleaned,
                ctc_conf
            );
            let (weapon, score) = matched.unwrap();
            assert_eq!(
                weapon, expected,
                "PaddleOCR on '{}' decoded '{}' -> matched '{}' instead of '{}' (score {:.3})",
                title, raw_ocr, weapon, expected, score
            );
        }

        // Occlude first character directly with the yellow 18+ warning rectangle and verify real ONNX + suffix fallback
        for (title, occlude_right_x, expected) in [
            ("M7 Battle Rifle", 42u32, "M7"),
            ("R93 Sniper Rifle", 40u32, "R93"),
        ] {
            let mut img = render_hud_title(title, false);
            for y in 0..img.height() {
                for x in 0..occlude_right_x {
                    img.put_pixel(x, y, xcap::image::Rgba([255, 192, 16, 255]));
                }
            }
            let (raw_ocr, _) = engine.recognize(&img).expect("Occluded PaddleOCR inference should succeed");
            let (matched, cleaned) = recognize_weapon_from_title(&raw_ocr, &candidates);
            assert!(
                matched.is_some(),
                "Occluded PaddleOCR failed on '{}' -> raw_ocr: '{}', cleaned: '{}'",
                title,
                raw_ocr,
                cleaned
            );
            assert_eq!(
                matched.unwrap().0,
                expected,
                "Occluded PaddleOCR on '{}' decoded '{}' -> expected '{}'",
                title,
                raw_ocr,
                expected
            );
        }

        // Category-only header rendered through real ONNX must return None (rejected)
        let cat_img = render_hud_title("Assault Rifle", false);
        let (cat_raw, _) = engine.recognize(&cat_img).expect("Category PaddleOCR inference should succeed");
        let (cat_match, _) = recognize_weapon_from_title(&cat_raw, &candidates);
        assert!(
            cat_match.is_none(),
            "Real PaddleOCR category-only '{}' (decoded '{}') must be rejected, got {:?}",
            "Assault Rifle",
            cat_raw,
            cat_match
        );

        // Helper to blit rendered HUD strip onto a 1920x1080 screen canvas
        let blit_strip = |dst: &mut xcap::image::RgbaImage, src: &xcap::image::RgbaImage, dx: u32, dy: u32, max_w: u32, max_h: u32| {
            let copy_w = src.width().min(max_w).min(dst.width().saturating_sub(dx));
            let copy_h = src.height().min(max_h).min(dst.height().saturating_sub(dy));
            for sy in 0..copy_h {
                for sx in 0..copy_w {
                    dst.put_pixel(dx + sx, dy + sy, *src.get_pixel(sx, sy));
                }
            }
        };

        // Pass 1B ONNX recovery: dim 18+-shadowed text in left 30% of center header (`x: 24.0%..41.5%, y: 15.0%..19.6%`)
        let mut frame_1b = xcap::image::RgbaImage::from_pixel(1920, 1080, xcap::image::Rgba([20, 26, 34, 255]));
        let mut dim_center = render_hud_title("SG552 Assault Rifle", false);
        let dim_w = (dim_center.width() as f32 * 0.25) as u32;
        for y in 0..dim_center.height() {
            for x in 0..dim_w {
                let p = dim_center.get_pixel(x, y);
                // Dim foreground glyphs while keeping subtle warm shadow box so Pass 1B gamma boost recovers cleanly
                if p[0] > 80 {
                    dim_center.put_pixel(x, y, xcap::image::Rgba([95, 92, 88, 255]));
                } else {
                    dim_center.put_pixel(x, y, xcap::image::Rgba([28, 24, 18, 255]));
                }
            }
        }
        blit_strip(&mut frame_1b, &dim_center, 461, 162, 336, 50);
        let out_1b = recognize_from_captured_image(&mut engine, &frame_1b, &candidates)
            .expect("Pass 1B E2E recognition should run cleanly");
        assert_eq!(out_1b.weapon, Some("SG552"), "Pass 1B raw='{}'", out_1b.raw_ocr);
        assert!(out_1b.can_auto_copy, "Pass 1B SG552 should allow auto-copy");

        // Pass 2 ONNX fallback: center header shows ONLY category (`Assault Rifle`), while left white-bordered card
        // carries top-left code + skin (`K437 - Scarlet`) inside `x: 7.0%..23.6%, y: 13.0%..90.0%`
        let mut frame_p2 = xcap::image::RgbaImage::from_pixel(1920, 1080, xcap::image::Rgba([20, 26, 34, 255]));
        blit_strip(&mut frame_p2, &render_hud_title("     Assault Rifle", false), 461, 162, 336, 50);

        // White-bordered active card at (148..428, 300..396) in 1920x1080
        let (cx1, cy1, cx2, cy2) = (148u32, 300u32, 428u32, 396u32);
        let white = xcap::image::Rgba([248, 248, 248, 255]);
        for x in cx1..cx2 {
            frame_p2.put_pixel(x, cy1, white);
            frame_p2.put_pixel(x, cy1 + 1, white);
            frame_p2.put_pixel(x, cy2 - 1, white);
            frame_p2.put_pixel(x, cy2 - 2, white);
        }
        for y in cy1..cy2 {
            frame_p2.put_pixel(cx1, y, white);
            frame_p2.put_pixel(cx1 + 1, y, white);
            frame_p2.put_pixel(cx2 - 1, y, white);
            frame_p2.put_pixel(cx2 - 2, y, white);
        }
        let card_text = render_hud_title("K437 - Scarlet", false);
        blit_strip(&mut frame_p2, &card_text, cx1 + 4, cy1 + 4, cx2 - cx1 - 8, 42);

        let out_p2 = recognize_from_captured_image(&mut engine, &frame_p2, &candidates)
            .expect("Pass 2 E2E recognition should run cleanly");
        assert_eq!(out_p2.weapon, Some("K437"), "Pass 2 raw='{}'", out_p2.raw_ocr);
        assert!(out_p2.can_auto_copy, "Pass 2 K437 + locked Assault Rifle should auto-copy");
    }

    #[test]
    fn test_short_weapon_margin_check_and_exact_preservation() {
        let candidates = get_test_candidates();

        // 1. Absolute exact matches for short weapons (<= 3 chars) must preserve auto-copy (>= 0.99)
        for exact in ["G3", "M7", "P90", "MK4", "R93", "M82", "AUG", "SVD", "VSS", "SKS", "UZI", "PKM"] {
            let matched = match_weapon(exact, &candidates).expect("exact short weapon should match");
            assert_eq!(matched.0, exact);
            assert!(
                matched.1 >= 0.99,
                "Exact match '{}' should preserve >= 0.99 for auto-copy (got {:.3})",
                exact,
                matched.1
            );
            assert_eq!(classify_confidence(matched.1), ConfidenceTier::High);
        }

        // 2. Close Top-1 vs Top-2 margin (< 0.08) for short weapons must downgrade to confirm zone (0.85)
        let ambiguous = match_weapon("G3 P90", &candidates).expect("ambiguous short pair should match top-1");
        assert_eq!(ambiguous.0, "G3");
        assert!((ambiguous.1 - 0.85).abs() < 1e-6, "Expected 0.85 confirm score, got {:.3}", ambiguous.1);
        assert_eq!(classify_confidence(ambiguous.1), ConfidenceTier::Suspicious);

        // 3. Fuzzy branch on short weapons (<= 3 chars) must cap into confirm zone (0.75..0.89)
        for fuzzy_input in ["P90A", "M7A", "R93X"] {
            let matched = match_weapon(fuzzy_input, &candidates).expect("fuzzy short input should match");
            assert!(
                matched.1 <= 0.85 && matched.1 >= 0.75,
                "Fuzzy short match '{}' -> '{}' should stay in confirm zone (got {:.3})",
                fuzzy_input,
                matched.0,
                matched.1
            );
            assert_eq!(classify_confidence(matched.1), ConfidenceTier::Suspicious);
        }
    }

    #[test]
    fn test_validate_downloaded_builds() {
        // 1. Empty payload rejected
        assert!(validate_downloaded_builds(vec![]).is_err());

        let make_item = |weapon: &str, code: &str| BuildItem {
            server: "Garena".to_string(),
            mode: "Warfare".to_string(),
            weapon_type: "Assault Rifle".to_string(),
            weapon: weapon.to_string(),
            classification: "DFTools".to_string(),
            tags: vec!["DFTools".to_string()],
            copy_count: 100,
            loadout_code: code.to_string(),
            is_custom: false,
            is_favorite: false,
        };

        // 2. All invalid entries rejected
        let all_bad = vec![
            make_item("", "M4A1 Assault Rifle-Warfare-6HABCDEF123456"),
            make_item("M4A1", "TooShort-123"),
            make_item("M4A1", "NoDashesAtAll123456789012"),
            make_item("M4A1", "M4A1-Warfare-\n6HABCDEF123456"),
        ];
        assert!(validate_downloaded_builds(all_bad).is_err());

        // 3. Mixed valid and invalid keeps & trims only valid records
        let mixed = vec![
            make_item("Fast", "InvalidShort"),
            make_item("  M4A1  ", "  M4A1 Assault Rifle-Warfare-6HABCDEF123456  "),
            make_item("AK-12", "SingleDash-123456789012345"),
        ];
        let validated = validate_downloaded_builds(mixed).expect("mixed should retain 1 valid build");
        assert_eq!(validated.len(), 1);
        assert_eq!(validated[0].weapon, "M4A1");
        assert_eq!(validated[0].loadout_code, "M4A1 Assault Rifle-Warfare-6HABCDEF123456");

        // 4. Embedded BUILDS_JSON passes validation 100%
        let embedded: Vec<BuildItem> = serde_json::from_str(BUILDS_JSON).expect("valid embedded json");
        let total = embedded.len();
        let validated_embedded = validate_downloaded_builds(embedded.clone()).expect("embedded should pass");
        if validated_embedded.len() != total {
            let kept: HashSet<String> = validated_embedded.iter().map(|b| b.loadout_code.clone()).collect();
            let dropped: Vec<String> = embedded
                .iter()
                .filter(|b| !kept.contains(b.loadout_code.trim()))
                .map(|b| format!("{} => {}", b.weapon, b.loadout_code))
                .collect();
            panic!(
                "Embedded BUILDS_JSON dropped {}/{} builds:\n{}",
                dropped.len(),
                total,
                dropped.join("\n")
            );
        }
    }

    #[test]
    fn test_semantic_validator_mismatch_and_dedup() {
        let make_item = |weapon: &str, code: &str| BuildItem {
            server: "Garena".to_string(),
            mode: "Warfare".to_string(),
            weapon_type: "Rifle".to_string(),
            weapon: weapon.to_string(),
            classification: "DFTools".to_string(),
            tags: vec!["DFTools".to_string()],
            copy_count: 100,
            loadout_code: code.to_string(),
            is_custom: false,
            is_favorite: false,
        };

        let items = vec![
            // Semantic mismatches -> must be dropped
            make_item("P90", "M14 Marksman Rifle-Warfare-6HABCDEF123456"),
            make_item("M7", "M4A1 Assault Rifle-Warfare-6HABCDEF123456"),
            // Valid localized / alias codes -> must be retained
            make_item("CI-19", "騰龍突擊步槍-Warfare-6HABCDEF111111"),
            make_item("Marlin Lever-Action", "Lever-action Rifle-Operations-6HABCDEF222222"),
            make_item("UZI", "UZl Submachine Gun-Warfare-6HABCDEF333333"),
            // Duplicate loadout_code (with extra whitespace) -> only 1 kept
            make_item("M4A1", "  M4A1 Assault Rifle-Warfare-6HABCDEF444444  "),
            make_item("M4A1", "M4A1 Assault Rifle-Warfare-6HABCDEF444444"),
        ];

        let validated = validate_downloaded_builds(items).expect("should keep 4 valid unique builds");
        let codes: Vec<&str> = validated.iter().map(|b| b.loadout_code.as_str()).collect();
        assert_eq!(validated.len(), 4);
        assert!(!codes.contains(&"M14 Marksman Rifle-Warfare-6HABCDEF123456"));
        assert!(!codes.contains(&"M4A1 Assault Rifle-Warfare-6HABCDEF123456"));
        assert!(codes.contains(&"騰龍突擊步槍-Warfare-6HABCDEF111111"));
        assert!(codes.contains(&"Lever-action Rifle-Operations-6HABCDEF222222"));
        assert!(codes.contains(&"UZl Submachine Gun-Warfare-6HABCDEF333333"));
        assert!(codes.contains(&"M4A1 Assault Rifle-Warfare-6HABCDEF444444"));
    }

    #[test]
    fn test_monitor_stable_identity_and_disconnect_fallback() {
        let mon1_id = format_monitor_stable_id("\\\\.\\DISPLAY1", 2560, 1440);
        let mon2_id = format_monitor_stable_id("\\\\.\\DISPLAY2", 1920, 1080);

        // Order swapped compared to original enumeration (DISP2 now at index 0)
        let monitors = vec![
            MonitorDescriptor {
                stable_id: mon2_id.clone(),
                label: "Display 1".to_string(),
                x: 2560,
                y: 0,
                width: 1920,
                height: 1080,
                is_primary: false,
            },
            MonitorDescriptor {
                stable_id: mon1_id.clone(),
                label: "Display 2 [Primary]".to_string(),
                x: 0,
                y: 0,
                width: 2560,
                height: 1440,
                is_primary: true,
            },
        ];

        // 1. Saved stable ID tracks physical monitor even when index shifts
        assert_eq!(
            resolve_monitor_index(&monitors, Some(&mon2_id), Some((0, 0, 2560, 1440))),
            Some(0)
        );
        assert_eq!(
            resolve_monitor_index(&monitors, Some(&mon1_id), Some((2560, 0, 1920, 1080))),
            Some(1)
        );

        // 2. Disconnected saved monitor falls back to active foreground game window monitor
        assert_eq!(
            resolve_monitor_index(
                &monitors,
                Some("\\\\.\\DISPLAY3 (3840x2160)"),
                Some((2560, 0, 1920, 1080)),
            ),
            Some(0)
        );

        // 3. Disconnected monitor with no foreground rect falls back to primary monitor
        assert_eq!(
            resolve_monitor_index(&monitors, Some("\\\\.\\DISPLAY3 (3840x2160)"), None),
            Some(1)
        );
    }

    #[test]
    fn test_atomic_write_verified() {
        let test_dir = std::env::temp_dir().join(format!(
            "df_helper_atomic_test_{}_{}",
            std::process::id(),
            get_current_time_str().replace(':', "")
        ));
        let _ = fs::create_dir_all(&test_dir);
        let target = test_dir.join("test_db.json");
        let tmp = target.with_extension("json.tmp");

        atomic_write_verified(&target, r#"[{"weapon":"M4A1"}]"#).expect("atomic write should succeed");
        assert!(target.exists());
        assert!(!tmp.exists());
        assert_eq!(
            fs::read_to_string(&target).expect("read target"),
            r#"[{"weapon":"M4A1"}]"#
        );

        // Overwrite with new payload preserves target and removes .tmp
        atomic_write_verified(&target, r#"[{"weapon":"AK-12"}]"#).expect("atomic overwrite should succeed");
        assert!(!tmp.exists());
        assert_eq!(
            fs::read_to_string(&target).expect("read target after overwrite"),
            r#"[{"weapon":"AK-12"}]"#
        );

        let _ = fs::remove_dir_all(&test_dir);
    }

    #[test]
    fn test_semver_comparison_with_prerelease() {
        // Pre-release progression: 2.0.1-beta < 2.0.1 < 2.0.2-beta < 2.0.2 < 2.1.0
        assert!(is_newer_version("v2.0.1", "v2.0.1-beta"));
        assert!(is_newer_version("2.0.2-beta", "v2.0.1-beta"));
        assert!(is_newer_version("v2.0.2", "v2.0.1-beta"));
        assert!(is_newer_version("v2.1.0", "v2.0.1-beta"));
        assert!(is_newer_version("v2.0.1-beta.2", "v2.0.1-beta"));
        assert!(is_newer_version("v2.0.1-rc.1", "v2.0.1-beta.2"));

        // Equal or older versions MUST NOT trigger update
        assert!(!is_newer_version("v2.0.1-beta", "v2.0.1-beta"));
        assert!(!is_newer_version("2.0.1-beta", "v2.0.1-beta"));
        assert!(!is_newer_version("v2.0.0", "v2.0.1-beta"));
        assert!(!is_newer_version("v2.0.0-beta", "v2.0.1-beta"));
        assert!(!is_newer_version("v1.4.6", "v2.0.1-beta"));

        // Stable build ignores newer pre-release, but accepts newer stable release
        assert!(!is_newer_version("v2.0.2-beta", "v2.0.1"));
        assert!(is_newer_version("v2.0.2", "v2.0.1"));
        assert!(is_newer_version("v2.1.0", "v2.0.1"));
        assert!(!is_newer_version("v2.0.1-beta", "v2.0.1"));
        assert!(!is_newer_version("invalid-tag", "v2.0.1-beta"));
    }

    #[test]
    fn test_github_releases_payload_selection_and_config_default() {
        let payload = serde_json::json!([
            {
                "tag_name": "v2.1.0-draft",
                "name": "Draft Build",
                "draft": true,
                "prerelease": true,
                "html_url": "https://github.com/TaiGiap/df-loadout-scanner/releases/tag/v2.1.0-draft",
                "body": "Should be ignored",
                "assets": []
            },
            {
                "tag_name": "v2.0.2-beta",
                "name": "DF Loadout Scanner v2.0.2-beta",
                "draft": false,
                "prerelease": true,
                "html_url": "https://github.com/TaiGiap/df-loadout-scanner/releases/tag/v2.0.2-beta",
                "body": "# Title\n---\nCải thiện tốc độ quét OCR và thêm vũ khí mùa mới.\n",
                "assets": [
                    {
                        "name": "DF-Loadout-Scanner-v2.0.2-beta-Windows-x64.zip",
                        "browser_download_url": "https://github.com/TaiGiap/df-loadout-scanner/releases/download/v2.0.2-beta/DF-Loadout-Scanner-v2.0.2-beta-Windows-x64.zip"
                    }
                ]
            },
            {
                "tag_name": "v2.0.1-beta",
                "name": "DF Loadout Scanner v2.0.1-beta",
                "draft": false,
                "prerelease": true,
                "html_url": "https://github.com/TaiGiap/df-loadout-scanner/releases/tag/v2.0.1-beta",
                "body": "Current release",
                "assets": []
            }
        ]);

        // From v2.0.1-beta, selects v2.0.2-beta with zip asset & extracted summary
        let selected = select_latest_update_from_releases(&payload, "v2.0.1-beta")
            .expect("should detect v2.0.2-beta from v2.0.1-beta");
        assert_eq!(selected.tag_name, "v2.0.2-beta");
        assert_eq!(selected.name, "DF Loadout Scanner v2.0.2-beta");
        assert_eq!(selected.summary, "Cải thiện tốc độ quét OCR và thêm vũ khí mùa mới.");
        assert_eq!(
            selected.zip_file_name.as_deref(),
            Some("DF-Loadout-Scanner-v2.0.2-beta-Windows-x64.zip")
        );
        assert!(selected.zip_download_url.is_some());

        // From stable v2.0.1, ignores prerelease v2.0.2-beta
        assert!(select_latest_update_from_releases(&payload, "v2.0.1").is_none());

        // From v2.0.2-beta itself, no newer update
        assert!(select_latest_update_from_releases(&payload, "v2.0.2-beta").is_none());

        // Backward compatibility: old config.json without `auto_check_update` defaults to `true`
        let old_cfg: UserConfig = serde_json::from_str(r#"{"server":"Garena","mode":"Warfare"}"#)
            .expect("old config should deserialize");
        assert!(old_cfg.auto_check_update);
    }

    #[test]
    fn test_zip_archive_validation_and_post_update_flag() {
        let build_minimal_zip = |entry_names: &[&str]| -> Vec<u8> {
            let mut out = Vec::new();
            let mut cd = Vec::new();
            for &name in entry_names {
                let name_bytes = name.as_bytes();
                let local_offset = out.len() as u32;
                // Local file header
                out.extend_from_slice(&[0x50, 0x4B, 0x03, 0x04]);
                out.extend_from_slice(&20u16.to_le_bytes());
                out.extend_from_slice(&0u16.to_le_bytes());
                out.extend_from_slice(&0u16.to_le_bytes());
                out.extend_from_slice(&0u16.to_le_bytes());
                out.extend_from_slice(&0u16.to_le_bytes());
                out.extend_from_slice(&0u32.to_le_bytes());
                out.extend_from_slice(&0u32.to_le_bytes());
                out.extend_from_slice(&0u32.to_le_bytes());
                out.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
                out.extend_from_slice(&0u16.to_le_bytes());
                out.extend_from_slice(name_bytes);

                // Central directory file header
                cd.extend_from_slice(&[0x50, 0x4B, 0x01, 0x02]);
                cd.extend_from_slice(&20u16.to_le_bytes());
                cd.extend_from_slice(&20u16.to_le_bytes());
                cd.extend_from_slice(&0u16.to_le_bytes());
                cd.extend_from_slice(&0u16.to_le_bytes());
                cd.extend_from_slice(&0u16.to_le_bytes());
                cd.extend_from_slice(&0u16.to_le_bytes());
                cd.extend_from_slice(&0u32.to_le_bytes());
                cd.extend_from_slice(&0u32.to_le_bytes());
                cd.extend_from_slice(&0u32.to_le_bytes());
                cd.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
                cd.extend_from_slice(&0u16.to_le_bytes());
                cd.extend_from_slice(&0u16.to_le_bytes());
                cd.extend_from_slice(&0u16.to_le_bytes());
                cd.extend_from_slice(&0u16.to_le_bytes());
                cd.extend_from_slice(&0u32.to_le_bytes());
                cd.extend_from_slice(&local_offset.to_le_bytes());
                cd.extend_from_slice(name_bytes);
            }
            let cd_offset = out.len() as u32;
            let cd_size = cd.len() as u32;
            out.extend_from_slice(&cd);
            // End of central directory
            out.extend_from_slice(&[0x50, 0x4B, 0x05, 0x06]);
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&(entry_names.len() as u16).to_le_bytes());
            out.extend_from_slice(&(entry_names.len() as u16).to_le_bytes());
            out.extend_from_slice(&cd_size.to_le_bytes());
            out.extend_from_slice(&cd_offset.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out
        };

        let test_dir = std::env::temp_dir().join(format!(
            "df_helper_wayb_test_{}_{}",
            std::process::id(),
            get_current_time_str().replace(':', "")
        ));
        let _ = fs::create_dir_all(&test_dir);

        // 1. Valid release .zip with .exe + onnxruntime.dll passes validation
        let valid_zip = test_dir.join("valid_release.zip");
        let valid_bytes = build_minimal_zip(&[
            "DF-Loadout-Scanner/DF-Loadout-Scanner-Windows-x64.exe",
            "DF-Loadout-Scanner/onnxruntime.dll",
        ]);
        fs::write(&valid_zip, &valid_bytes).expect("write valid zip");
        let entries = validate_update_zip_archive(&valid_zip).expect("valid zip should pass");
        assert_eq!(entries.len(), 2);

        // 2. Archive missing .exe MUST fail validation
        let no_exe_zip = test_dir.join("no_exe.zip");
        fs::write(&no_exe_zip, build_minimal_zip(&["DF-Loadout-Scanner/README.md"]))
            .expect("write no_exe zip");
        assert!(validate_update_zip_archive(&no_exe_zip).is_err());

        // 3. Corrupted / non-zip file MUST fail validation
        let bad_zip = test_dir.join("corrupted.zip");
        fs::write(&bad_zip, b"not a zip archive").expect("write bad zip");
        assert!(validate_update_zip_archive(&bad_zip).is_err());

        // 4. One-shot post-update flag is written, read once, and automatically removed
        let flag_path = test_dir.join("just_updated_to.txt");
        write_just_updated_flag_at(&flag_path, "v2.0.3-beta").expect("write flag");
        assert_eq!(
            consume_just_updated_flag_at(&flag_path),
            Some("v2.0.3-beta".to_string())
        );
        assert_eq!(consume_just_updated_flag_at(&flag_path), None);
        assert!(!flag_path.exists());

        let _ = fs::remove_dir_all(&test_dir);
    }
}

