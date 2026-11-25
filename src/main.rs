#![windows_subsystem = "windows"]

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, RwLock};
use std::thread::{self, sleep};
use std::time::{Duration, Instant};

use arboard::Clipboard;
use eframe::egui::{self, Align, Color32, Frame, Layout, Margin, RichText, Rounding, Stroke, Vec2};
use serde::{Deserialize, Serialize};
use windows::{
    core::HSTRING,
    Globalization::Language as WinLanguage,
    Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap},
    Media::Ocr::OcrEngine,
    Storage::Streams::DataWriter,
    Win32::Foundation::{BOOL, HWND},
    Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST},
    Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED},
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
const APP_VERSION: &str = "v1.4.5";

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
    selected_monitor_index: Option<usize>,
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
    selected_monitor_index: Option<usize>,
}

fn default_server() -> String { "Garena".to_string() }
fn default_mode() -> String { "Warfare".to_string() }
fn default_source() -> String { "ALL".to_string() }
fn default_lang() -> String { "vi".to_string() }
fn default_hotkey_vk() -> i32 { 0x78 } // VK_F9
fn default_hotkey_name() -> String { "F9".to_string() }

impl Default for UserConfig {
    fn default() -> Self {
        Self {
            server: default_server(),
            mode: default_mode(),
            source: default_source(),
            lang: default_lang(),
            hotkey_vk: default_hotkey_vk(),
            hotkey_name: default_hotkey_name(),
            selected_monitor_index: None,
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

fn save_favorites(favorites: &[BuildItem]) {
    let path = favorites_file_path();
    let tmp_path = path.with_extension("json.tmp");
    if let Ok(content) = serde_json::to_string_pretty(favorites) {
        if fs::write(&tmp_path, &content).is_ok() {
            if fs::rename(&tmp_path, &path).is_err() {
                let _ = fs::copy(&tmp_path, &path);
                let _ = fs::remove_file(&tmp_path);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfidenceTier {
    High,       // >= 0.90: Tự động sao chép vào Clipboard
    Suspicious, // 0.75 <= score < 0.90: Vùng nghi ngờ, yêu cầu xác nhận thủ công
    Rejected,   // < 0.75: Từ chối / không nhận diện
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

fn save_user_config(config: &UserConfig) {
    let path = config_file_path();
    let tmp_path = path.with_extension("json.tmp");
    if let Ok(content) = serde_json::to_string_pretty(config) {
        if fs::write(&tmp_path, &content).is_ok() {
            if fs::rename(&tmp_path, &path).is_err() {
                let _ = fs::copy(&tmp_path, &path);
                let _ = fs::remove_file(&tmp_path);
            }
        }
    }
}

enum WorkerEvent {
    WeaponScanned {
        weapon: String,
        score: f64,
        raw_ocr: String,
        builds: Vec<BuildItem>,
        is_uncertain: bool,
        clipboard_error: Option<String>,
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
    OcrUnavailable,
    NewVersionAvailable(String),
}

enum DbUpdateResult {
    Success { builds: Vec<BuildItem>, count: usize },
    Error(String),
}

const BUILDS_JSON: &str = include_str!("../builds.json");

/// Validates downloaded database records before persisting to `builds_local.json`.
/// Ensures payload is non-empty and each retained record has non-empty `weapon`
/// and a valid `loadout_code` (`>= 18` chars, at least 2 `-` separators, no newlines).
fn validate_downloaded_builds(raw_items: Vec<BuildItem>) -> Result<Vec<BuildItem>, String> {
    if raw_items.is_empty() {
        return Err("Dữ liệu rỗng / Empty database".to_string());
    }

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
            item.weapon = weapon_trimmed;
            item.loadout_code = code_trimmed;
            Some(item)
        })
        .collect();

    if valid.is_empty() {
        return Err("Không có bản ghi hợp lệ trong dữ liệu tải về / No valid builds found in downloaded payload".to_string());
    }

    Ok(valid)
}

fn history_file_path() -> PathBuf {
    app_data_dir().join("df_history.json")
}

fn local_builds_file_path() -> PathBuf {
    app_data_dir().join("builds_local.json")
}

fn local_builds_tmp_file_path() -> PathBuf {
    app_data_dir().join("builds_local.json.tmp")
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

fn check_ocr_available() -> bool {
    let _ = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };
    let language_tag = HSTRING::from("en-US");
    let engine = if let Ok(lang) = WinLanguage::CreateLanguage(&language_tag) {
        OcrEngine::TryCreateFromLanguage(&lang).ok()
    } else {
        None
    }.or_else(|| OcrEngine::TryCreateFromUserProfileLanguages().ok());
    engine.is_some()
}

fn load_history() -> Vec<HistoryItem> {
    let path = history_file_path();
    if let Ok(content) = fs::read_to_string(&path) {
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        Vec::new()
    }
}

fn save_history(history: &[HistoryItem]) {
    let path = history_file_path();
    if let Ok(content) = serde_json::to_string_pretty(history) {
        let _ = fs::write(path, content);
    }
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

/// In-game weapon categories (Vietnamese & English) to strip from OCR text.
/// Ordered from longest to shortest phrase to prevent partial substring collisions.
const CATEGORY_PHRASES: &[&str] = &[
    // Vietnamese (longest first)
    "SUNG TRUONG CHIEN DAU",
    "SUNG TRUONG TAN CONG",
    "SUNG TRUONG THIEN XA",
    "SUNG MAY HANG NHE",
    "SUNG MAY DA DUNG",
    "SUNG MAY DA NANG",
    "SUNG TIEU LIEN",
    "SUNG BAN TIA",
    "SUNG TRUONG GON NHE",
    "SUNG TRUONG HANG NANG",
    "SUNG TRUONG CO DIEN",
    "SUNG TRUONG CHIEN THUAT",
    "SUNG TIEU LIEN GON NHE",
    "SUNG MAY HANG NANG",
    "SUNG TRUONG",
    "SUNG NGAN",
    "SUNG SAN",
    "SUNG LUC",
    "SUNG MAY",
    "TIEU LIEN",
    "GON NHE",
    "HANG NANG",
    "CO DIEN",
    "CHIEN THUAT",
    // English (longest first)
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

/// Weapon alias mapping (Vietnamese, abbreviations, variants -> exact builds.json canonical name).
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
    ("LEVER ACTION", "MARLIN LEVER-ACTION"),
    ("MARLIN", "MARLIN LEVER-ACTION"),
    ("357 REVOLVER", ".357"),
    ("357", ".357"),
    ("MINI 14", "MINI-14"),
    ("MINI14", "MINI-14"),
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

fn resolve_alias_internal(text: &str, strict_common_words: bool) -> Option<&'static str> {
    let trimmed = text.trim();
    for &(alias, canonical) in WEAPON_ALIASES {
        if trimmed == alias {
            return Some(canonical);
        }
        // In prose descriptions, avoid matching ambiguous short common words ("CUNG", "NO") as prefixes/suffixes
        if strict_common_words && matches!(alias, "CUNG" | "NO") {
            continue;
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
    resolve_alias_internal(text, false)
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
            continue;
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

    text
}

fn clean_ocr_text(raw: &str) -> String {
    let raw_skin_stripped = strip_skin_suffix(raw);

    // Find the best line: first line with weapon content after stripping banner words / lowercase junk
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
                && t.len() <= 3
                && raw_tokens.len() > 1
                && (raw_tokens[1].chars().next().map_or(false, |c| c.is_uppercase() || c.is_ascii_digit()))
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

    // 1. Convert to unaccented uppercase
    let normalized = strip_vietnamese_diacritics(&best_line);

    // 2. Filter allowed characters: alphanumeric, hyphen, underscore, dot, and space
    let filtered: String = normalized
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' { c } else { ' ' })
        .collect();

    let tokens: Vec<&str> = filtered
        .split_whitespace()
        .filter(|t| !BANNER_18_STOP_WORDS.contains(t))
        .collect();
    let text = tokens.join(" ");

    // 3. Check aliases before category stripping (e.g. "SHOTGUN HAI NONG" -> "725 DOUBLE-BARREL")
    if let Some(canonical) = resolve_alias(&text) {
        return canonical.to_string();
    }

    // 4. Strip category phrases and trailing category abbreviations
    let stripped = strip_category_phrases(text);

    // 5. Check aliases again after stripping
    if let Some(canonical) = resolve_alias(&stripped) {
        return canonical.to_string();
    }

    stripped
}

/// Generic leading words commonly starting weapon description sentences in VI/EN.
const DESCRIPTION_LEADING_WORDS: &[&str] = &[
    "SUNG", "THE", "KHAU", "MOT", "CHIEC", "CAY", "MAU", "A", "AN",
];

/// Extracts and cleans the weapon name candidate from the bottom-center description
/// paragraph (first 4–5 meaningful words, removing generic leading words).
fn extract_description_weapon_text(raw: &str) -> String {
    if raw.trim().is_empty() {
        return String::new();
    }

    let normalized = strip_vietnamese_diacritics(raw);
    let chars: Vec<char> = normalized.chars().collect();
    let mut filtered = String::with_capacity(chars.len());

    for (i, &c) in chars.iter().enumerate() {
        if c.is_alphanumeric() || c == '_' {
            filtered.push(c);
        } else if c == '-' {
            let prev_alnum = i > 0 && chars[i - 1].is_alphanumeric();
            let next_alnum = i + 1 < chars.len() && chars[i + 1].is_alphanumeric();
            if prev_alnum && next_alnum {
                filtered.push('-');
            } else {
                filtered.push(' ');
            }
        } else if c == '.' {
            let next_digit = i + 1 < chars.len() && chars[i + 1].is_ascii_digit();
            if next_digit {
                filtered.push('.');
            } else {
                filtered.push(' ');
            }
        } else {
            filtered.push(' ');
        }
    }

    // Take initial window so long category/article prefixes don't push out weapon token
    let mut tokens: Vec<String> = filtered
        .split_whitespace()
        .take(10)
        .map(|s| s.to_string())
        .collect();

    if tokens.is_empty() {
        return String::new();
    }

    // Strip unambiguous leading articles/classifiers first (keep "SUNG" for compound checks below)
    while !tokens.is_empty()
        && tokens[0] != "SUNG"
        && DESCRIPTION_LEADING_WORDS.contains(&tokens[0].as_str())
    {
        tokens.remove(0);
    }

    if !tokens.is_empty() && tokens[0] == "SUNG" {
        let with_sung = tokens.iter().take(5).cloned().collect::<Vec<_>>().join(" ");
        if let Some(canonical) = resolve_alias_internal(&with_sung, true) {
            return canonical.to_string();
        }

        let without_sung = tokens.iter().skip(1).take(5).cloned().collect::<Vec<_>>().join(" ");
        if let Some(canonical) = resolve_alias_internal(&without_sung, true) {
            return canonical.to_string();
        }

        let joined_all = tokens.join(" ");
        let mut stripped_cat = false;
        for &phrase in CATEGORY_PHRASES {
            if let Some(rest) = joined_all.strip_prefix(&format!("{} ", phrase)) {
                tokens = rest.split_whitespace().map(|s| s.to_string()).collect();
                stripped_cat = true;
                break;
            }
        }
        if !stripped_cat {
            tokens.remove(0);
        }
    }

    while !tokens.is_empty() && DESCRIPTION_LEADING_WORDS.contains(&tokens[0].as_str()) {
        tokens.remove(0);
    }

    // Extract first 5 meaningful words from the description opening
    tokens.truncate(5);
    let snippet = tokens.join(" ");
    if snippet.is_empty() {
        return String::new();
    }

    if let Some(canonical) = resolve_alias_internal(&snippet, true) {
        return canonical.to_string();
    }

    let stripped = strip_category_phrases(snippet);
    if let Some(canonical) = resolve_alias_internal(&stripped, true) {
        return canonical.to_string();
    }

    stripped
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
fn is_newer_version(remote: &str, current: &str) -> bool {
    let parse_parts = |v: &str| -> Vec<u32> {
        v.trim_start_matches(|c: char| c == 'v' || c == 'V')
            .split('.')
            .filter_map(|s| s.parse::<u32>().ok())
            .collect()
    };
    let r = parse_parts(remote);
    let c = parse_parts(current);
    if r.len() == 3 && c.len() == 3 {
        r > c
    } else {
        remote.trim() != current.trim()
    }
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

fn match_weapon<'a>(cleaned: &str, candidates: &'a [String]) -> Option<(&'a str, f64)> {
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

fn upscale_and_threshold_image(
    img: &xcap::image::RgbaImage,
    scale: f64,
    threshold: u8,
) -> (u32, u32, Vec<u8>) {
    let old_w = img.width();
    let old_h = img.height();
    let new_w = ((old_w as f64) * scale).round() as u32;
    let new_h = ((old_h as f64) * scale).round() as u32;

    let mut bgra_bytes = Vec::with_capacity((new_w * new_h * 4) as usize);

    for y in 0..new_h {
        let src_yf = (y as f64) / scale;
        let y0 = (src_yf.floor() as u32).min(old_h.saturating_sub(1));
        let y1 = (y0 + 1).min(old_h.saturating_sub(1));
        let dy = (src_yf - y0 as f64).clamp(0.0, 1.0);

        for x in 0..new_w {
            let src_xf = (x as f64) / scale;
            let x0 = (src_xf.floor() as u32).min(old_w.saturating_sub(1));
            let x1 = (x0 + 1).min(old_w.saturating_sub(1));
            let dx = (src_xf - x0 as f64).clamp(0.0, 1.0);

            let p00 = img.get_pixel(x0, y0);
            let p10 = img.get_pixel(x1, y0);
            let p01 = img.get_pixel(x0, y1);
            let p11 = img.get_pixel(x1, y1);

            let luma = |p: &xcap::image::Rgba<u8>| -> f64 {
                p[0] as f64 * 0.299 + p[1] as f64 * 0.587 + p[2] as f64 * 0.114
            };

            let l0 = luma(p00) * (1.0 - dx) + luma(p10) * dx;
            let l1 = luma(p01) * (1.0 - dx) + luma(p11) * dx;
            let interpolated_luma = l0 * (1.0 - dy) + l1 * dy;

            let val = if interpolated_luma >= threshold as f64 { 255u8 } else { 0u8 };
            bgra_bytes.push(val); // B
            bgra_bytes.push(val); // G
            bgra_bytes.push(val); // R
            bgra_bytes.push(255); // A
        }
    }

    (new_w, new_h, bgra_bytes)
}

fn run_ocr_on_cropped(
    img: &xcap::image::RgbaImage,
    ocr_engine: &OcrEngine,
    scale: f64,
    threshold: u8,
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let (new_w, new_h, bgra_bytes) = upscale_and_threshold_image(img, scale, threshold);

    let writer = DataWriter::new()?;
    writer.WriteBytes(&bgra_bytes)?;
    let buffer = writer.DetachBuffer()?;

    let bitmap = SoftwareBitmap::CreateCopyFromBuffer(
        &buffer,
        BitmapPixelFormat::Bgra8,
        new_w as i32,
        new_h as i32,
    )?;

    let ocr_result = ocr_engine.RecognizeAsync(&bitmap)?.get()?;
    let text = ocr_result.Text()?.to_string();
    Ok(text)
}

/// Crops a region of the captured screen using normalized [0.0..1.0] coordinates,
/// safely clamping to image dimensions.
fn crop_screen_region(
    full_image: &xcap::image::RgbaImage,
    x_pct: f64,
    y_pct: f64,
    w_pct: f64,
    h_pct: f64,
) -> xcap::image::RgbaImage {
    let img_w = full_image.width();
    let img_h = full_image.height();
    let screen_w = img_w as f64;
    let screen_h = img_h as f64;

    let crop_x = ((screen_w * x_pct).round() as u32).min(img_w.saturating_sub(1));
    let crop_y = ((screen_h * y_pct).round() as u32).min(img_h.saturating_sub(1));
    let crop_w = ((screen_w * w_pct).round() as u32)
        .min(img_w.saturating_sub(crop_x))
        .max(1);
    let crop_h = ((screen_h * h_pct).round() as u32)
        .min(img_h.saturating_sub(crop_y))
        .max(1);

    xcap::image::imageops::crop_imm(full_image, crop_x, crop_y, crop_w, crop_h).to_image()
}

/// Scans the left weapon list column (`x: 7.0%..23.5%`, `y: 13.0%..90.0%`) to locate
/// the active weapon card highlighted by bright white borders (simultaneously detecting
/// left/right vertical edges and top/bottom horizontal edges across scrolled positions).
/// Returns `(title_x, title_y, title_w, title_h)` strictly inside the card borders.
fn find_selected_card_title_rect(full_image: &xcap::image::RgbaImage) -> Option<(u32, u32, u32, u32)> {
    let img_w = full_image.width();
    let img_h = full_image.height();
    let screen_w = img_w as f64;
    let screen_h = img_h as f64;

    let col_x = ((screen_w * 0.070).round() as u32).min(img_w);
    let col_x2 = ((screen_w * 0.235).round() as u32).min(img_w);
    let col_w = col_x2.saturating_sub(col_x);
    let col_y = ((screen_h * 0.130).round() as u32).min(img_h);
    let col_y2 = ((screen_h * 0.900).round() as u32).min(img_h);
    let col_h = col_y2.saturating_sub(col_y);

    if col_w < 50 || col_h < 100 {
        return None;
    }

    let cw = col_w as usize;
    let ch = col_h as usize;

    // Precompute bright achromatic white border mask row counts and column prefix sums
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

    let h_min = ((screen_h * 0.095).round() as usize).max(50).min(ch);
    let h_max = ((screen_h * 0.138).round() as usize).min(ch).max(h_min);
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

            // Left vertical border column
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

            // Right vertical border column
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

            // Reward simultaneous 4-edge frame match (guards against single shared border with adjacent card)
            let top_lr_min = top_ratio.min(left_ratio).min(right_ratio);
            let bot_lr_min = bottom_ratio.min(left_ratio).min(right_ratio);
            let score = top_ratio
                + bottom_ratio
                + left_ratio
                + right_ratio
                + 1.5 * top_lr_min
                + 1.0 * bot_lr_min;

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
    let detected_w = card_right.saturating_sub(card_left);
    let pad_x = ((detected_w as f64 * 0.018).round().max(3.0)) as u32;

    let (title_x, title_w) = if detected_w > col_w / 2 && detected_w > pad_x * 2 + 10 {
        (card_left + pad_x, detected_w - pad_x * 2)
    } else {
        (
            col_x + (col_w as f64 * 0.04).round() as u32,
            (col_w as f64 * 0.92).round() as u32,
        )
    };

    let title_y = abs_y + ((screen_h * 0.006).round().max(3.0)) as u32;
    let title_h = (screen_h * 0.042).round() as u32;

    let title_x = title_x.min(img_w.saturating_sub(1));
    let title_y = title_y.min(img_h.saturating_sub(1));
    let title_w = title_w.min(img_w - title_x).max(1);
    let title_h = title_h.min(img_h - title_y).max(1);

    Some((title_x, title_y, title_w, title_h))
}

/// Step 1 (Primary): Scans the bright white-bordered weapon card in the left list column,
/// upscales 2.5x with white-text isolation, strips skin suffixes, and runs match_weapon.
fn scan_left_column_card<'a>(
    full_image: &xcap::image::RgbaImage,
    ocr_engine: &OcrEngine,
    candidates: &'a [String],
) -> Option<(Option<(&'a str, f64)>, String, String)> {
    let (title_x, title_y, title_w, title_h) = find_selected_card_title_rect(full_image)?;
    let title_cropped =
        xcap::image::imageops::crop_imm(full_image, title_x, title_y, title_w, title_h).to_image();

    if let Ok(raw) = run_ocr_on_cropped(&title_cropped, ocr_engine, 2.5, 140) {
        let cleaned = clean_ocr_text(&raw);
        let matched = match_weapon(&cleaned, candidates);
        return Some((matched, raw, cleaned));
    }

    None
}

/// Step 2 (Fallback 1): Scans the weapon description box at bottom-center (`x: 24.5%..39.5%`,
/// `y: 76.0%..86.5%`), upscales 2.0x, extracts leading weapon words, and runs match_weapon.
fn scan_description_roi<'a>(
    full_image: &xcap::image::RgbaImage,
    ocr_engine: &OcrEngine,
    candidates: &'a [String],
) -> Option<(Option<(&'a str, f64)>, String, String)> {
    let desc_cropped = crop_screen_region(full_image, 0.245, 0.760, 0.150, 0.105);
    if let Ok(raw) = run_ocr_on_cropped(&desc_cropped, ocr_engine, 2.0, 120) {
        let cleaned = extract_description_weapon_text(&raw);
        let matched = match_weapon(&cleaned, candidates);
        return Some((matched, raw, cleaned));
    }

    None
}

/// Step 3 (Final Fallback): Scans the center column weapon header ROI shifted right (`x: 26.8%`)
/// to avoid the 18+ warning box completely.
fn scan_center_header_fallback<'a>(
    full_image: &xcap::image::RgbaImage,
    ocr_engine: &OcrEngine,
    candidates: &'a [String],
) -> Result<(Option<(&'a str, f64)>, String, String), Box<dyn std::error::Error + Send + Sync>> {
    let center_cropped = crop_screen_region(full_image, 0.268, 0.140, 0.180, 0.060);
    let raw = run_ocr_on_cropped(&center_cropped, ocr_engine, 2.5, 140)?;
    let cleaned = clean_ocr_text(&raw);
    let matched = match_weapon(&cleaned, candidates);
    Ok((matched, raw, cleaned))
}

/// Enumerates connected monitors for the manual UI selector dropdown.
fn enumerate_monitors(lang: AppLang) -> Vec<(usize, String)> {
    let Ok(monitors) = xcap::Monitor::all() else {
        return Vec::new();
    };
    monitors
        .into_iter()
        .enumerate()
        .map(|(idx, m)| {
            let primary_tag = if m.is_primary() {
                lang.t(" [Chính]", " [Primary]")
            } else {
                ""
            };
            let label = format!(
                "{} {} ({}x{}){}",
                lang.t("Màn", "Display"),
                idx + 1,
                m.width(),
                m.height(),
                primary_tag
            );
            (idx, label)
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

/// Selects the target monitor: uses manual index if provided and valid, otherwise matches
/// the monitor hosting the foreground window (`GetForegroundWindow` -> `MonitorFromWindow`),
/// falling back to the primary monitor.
fn select_target_monitor(
    selected_monitor_index: Option<usize>,
) -> Result<xcap::Monitor, Box<dyn std::error::Error + Send + Sync>> {
    let monitors = xcap::Monitor::all()?;
    if monitors.is_empty() {
        return Err("Không tìm thấy màn hình nào".into());
    }

    if let Some(idx) = selected_monitor_index {
        if let Some(m) = monitors.get(idx) {
            return Ok(m.clone());
        }
    }

    if let Some((fx, fy, fw, fh)) = get_foreground_monitor_rect() {
        if let Some(m) = monitors.iter().find(|m| {
            m.x() == fx && m.y() == fy && m.width() as i32 == fw && m.height() as i32 == fh
        }) {
            return Ok(m.clone());
        }
        if let Some(m) = monitors.iter().find(|m| m.x() == fx && m.y() == fy) {
            return Ok(m.clone());
        }
        let f_right = fx as i64 + fw as i64;
        let f_bottom = fy as i64 + fh as i64;
        if let Some(best) = monitors.iter().max_by_key(|m| {
            let mx1 = m.x() as i64;
            let my1 = m.y() as i64;
            let mx2 = mx1 + m.width() as i64;
            let my2 = my1 + m.height() as i64;
            let ox = (f_right.min(mx2) - (fx as i64).max(mx1)).max(0);
            let oy = (f_bottom.min(my2) - (fy as i64).max(my1)).max(0);
            ox * oy
        }) {
            let mx1 = best.x() as i64;
            let my1 = best.y() as i64;
            let mx2 = mx1 + best.width() as i64;
            let my2 = my1 + best.height() as i64;
            let overlap = (f_right.min(mx2) - (fx as i64).max(mx1)).max(0)
                * (f_bottom.min(my2) - (fy as i64).max(my1)).max(0);
            if overlap > 0 {
                return Ok(best.clone());
            }
        }
    }

    monitors
        .iter()
        .find(|m| m.is_primary())
        .cloned()
        .or_else(|| monitors.into_iter().next())
        .ok_or_else(|| "Không tìm thấy màn hình chính".into())
}

/// Writes text to the Windows Clipboard retrying up to `max_retries` times (35ms apart),
/// re-initializing `arboard::Clipboard` for one final attempt if all retries fail.
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

    match Clipboard::new() {
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

/// Writes a priority-ordered slice of codes (top build at index 0) into Clipboard/Win+V
/// in reverse order so index 0 ends up on `Ctrl + V`.
/// Returns `Ok(())` only if index 0 succeeds.
fn copy_codes_sequence_to_clipboard(
    cb_opt: &mut Option<Clipboard>,
    codes: &[String],
    delay_ms: u64,
) -> Result<(), String> {
    if codes.is_empty() {
        return Ok(());
    }

    if cb_opt.is_none() {
        *cb_opt = Clipboard::new().ok();
    }
    let cb = cb_opt
        .as_mut()
        .ok_or_else(|| "Không thể truy cập Windows Clipboard".to_string())?;

    let mut top_result: Result<(), String> = Err("Không có dữ liệu sao chép".to_string());
    let total = codes.len();
    for (rev_idx, code) in codes.iter().rev().enumerate() {
        let res = copy_to_clipboard_with_retry(cb, code, 3);
        let is_top_priority = rev_idx + 1 == total;
        if is_top_priority {
            top_result = res;
        } else if total > 1 {
            sleep(Duration::from_millis(delay_ms));
        }
    }
    top_result
}

fn capture_and_recognize_weapon<'a>(
    ocr_engine: &OcrEngine,
    candidates: &'a [String],
    selected_monitor_index: Option<usize>,
) -> Result<(Option<(&'a str, f64)>, String, String), Box<dyn std::error::Error + Send + Sync>> {
    let target_monitor = select_target_monitor(selected_monitor_index)?;
    let full_image = target_monitor.capture_image()?;

    let mut best_match: Option<(&'a str, f64, String, String)> = None;
    let mut diag_raw = String::new();
    let mut diag_cleaned = String::new();

    // Step 1 (Primary): Left column white-bordered selected card (100% immune to 18+ overlay)
    if let Some((matched_opt, raw, cleaned)) = scan_left_column_card(&full_image, ocr_engine, candidates) {
        if !raw.trim().is_empty() {
            diag_raw = raw.clone();
            diag_cleaned = cleaned.clone();
        }
        if let Some((matched, score)) = matched_opt {
            if score >= 0.90 {
                return Ok((Some((matched, score)), raw, cleaned));
            }
            best_match = Some((matched, score, raw, cleaned));
        }
    }

    // Step 2 (Fallback 1): Weapon description paragraph below attachments (x: 24.5%..39.5%, y: 76.0%..86.5%)
    if let Some((matched_opt, raw, cleaned)) = scan_description_roi(&full_image, ocr_engine, candidates) {
        if diag_raw.is_empty() && !raw.trim().is_empty() {
            diag_raw = raw.clone();
            diag_cleaned = cleaned.clone();
        }
        if let Some((matched, score)) = matched_opt {
            if score >= 0.90 {
                return Ok((Some((matched, score)), raw, cleaned));
            }
            let should_replace = best_match.as_ref().map_or(true, |&(_, best_s, _, _)| score > best_s);
            if should_replace {
                best_match = Some((matched, score, raw, cleaned));
            }
        }
    }

    // Step 3 (Final Fallback): Only run if neither Step 1 nor Step 2 matched, shifting x to 26.8%
    if best_match.is_none() {
        let (matched_opt, raw, cleaned) =
            scan_center_header_fallback(&full_image, ocr_engine, candidates)?;
        if let Some((matched, score)) = matched_opt {
            return Ok((Some((matched, score)), raw, cleaned));
        }
        if !raw.trim().is_empty() {
            diag_raw = raw;
            diag_cleaned = cleaned;
        }
    }

    if let Some((matched, score, raw, cleaned)) = best_match {
        Ok((Some((matched, score)), raw, cleaned))
    } else {
        Ok((None, diag_raw, diag_cleaned))
    }
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
        let _ = unsafe { RoInitialize(RO_INIT_MULTITHREADED) };

        let language_tag = HSTRING::from("en-US");
        let ocr_engine = if let Ok(lang) = WinLanguage::CreateLanguage(&language_tag) {
            OcrEngine::TryCreateFromLanguage(&lang).ok()
        } else {
            None
        }.or_else(|| OcrEngine::TryCreateFromUserProfileLanguages().ok());

        let ocr_engine = match ocr_engine {
            Some(e) => e,
            None => {
                let _ = tx.send(WorkerEvent::OcrUnavailable);
                ctx.request_repaint();
                return;
            }
        };

        let mut clipboard = Clipboard::new().ok();
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
                    let (cur_server, cur_mode, cur_source, cur_monitor_idx) = {
                        let f = active_filters.read().unwrap();
                        (f.server, f.mode, f.source, f.selected_monitor_index)
                    };

                    match capture_and_recognize_weapon(&ocr_engine, &candidates, cur_monitor_idx) {
                        Ok((Some((matched_weapon, score)), raw_ocr, cleaned)) => {
                            let conf_tier = classify_confidence(score);
                            if conf_tier == ConfidenceTier::Rejected {
                                unsafe {
                                    let _ = MessageBeep(MB_ICONERROR);
                                }
                                let _ = tx.send(WorkerEvent::ScanFailed {
                                    raw_ocr,
                                    cleaned,
                                });
                                ctx.request_repaint();
                                was_pressed = is_pressed;
                                continue;
                            }

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

                            if conf_tier == ConfidenceTier::High {
                                // Độ tin cậy cao (>= 90%): Tự động nạp vào Clipboard kèm Retry
                                let mut clipboard_error: Option<String> = None;
                                if !selected.is_empty() {
                                    let codes: Vec<String> =
                                        selected.iter().map(|b| b.loadout_code.clone()).collect();
                                    match copy_codes_sequence_to_clipboard(&mut clipboard, &codes, 150) {
                                        Ok(()) => {
                                            unsafe {
                                                let _ = MessageBeep(MB_OK);
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
                                });
                            } else {
                                // Vùng nghi ngờ (75% - 89%): KHÔNG tự động ghi đè Clipboard, yêu cầu xác nhận
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
                                });
                            }
                            ctx.request_repaint();
                        }
                        Ok((None, raw_ocr, cleaned)) => {
                            unsafe {
                                let _ = MessageBeep(MB_ICONERROR);
                            }
                            let _ = tx.send(WorkerEvent::ScanFailed {
                                raw_ocr,
                                cleaned,
                            });
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
    selected_monitor_index: Option<usize>,
    available_monitors: Vec<(usize, String)>,

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

    new_version_available: Option<String>,

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

        let ocr_available = check_ocr_available();
        let available_monitors = enumerate_monitors(lang);
        let selected_monitor_index = config
            .selected_monitor_index
            .filter(|&idx| idx < available_monitors.len());

        let active_filters = Arc::new(RwLock::new(ActiveFilters {
            server: selected_server,
            mode: selected_mode,
            source: selected_source,
            selected_monitor_index,
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

        // Background thread to check for newer version on GitHub Releases
        let tx_version = tx.clone();
        let ctx_version = cc.egui_ctx.clone();
        thread::spawn(move || {
            let url = "https://api.github.com/repos/TaiGiap/df-loadout-scanner/releases/latest";
            let resp = ureq::get(url)
                .set("User-Agent", "df-loadout-scanner")
                .timeout(Duration::from_secs(5))
                .call();

            if let Ok(res) = resp {
                if let Ok(json) = res.into_json::<serde_json::Value>() {
                    if let Some(tag) = json["tag_name"].as_str() {
                        if is_newer_version(tag, APP_VERSION) {
                            let _ = tx_version.send(WorkerEvent::NewVersionAvailable(tag.to_string()));
                            ctx_version.request_repaint();
                        }
                    }
                }
            }
        });

        let (tx_db_update, rx_db_update) = channel();

        let default_weapon = determine_startup_weapon(&unique_weapons, &builds, selected_server, selected_mode);

        let (initial_status, is_err) = if !ocr_available {
            let msg = match lang {
                AppLang::Vi => format!("CẢNH BÁO: Windows thiếu gói OCR English (US)! Phím {} bị tắt. Vui lòng cài đặt 'English (United States)' trong Windows Settings > Time & Language. Bạn vẫn có thể tra cứu thủ công bên dưới.", hotkey_name),
                AppLang::En => format!("WARNING: Windows OCR language package missing! {} is disabled. Please install 'English (United States)' in Windows Settings > Time & Language. Manual lookup is still available.", hotkey_name),
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
            selected_monitor_index,
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
            new_version_available: None,
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
        if let Some(pos) = self.favorites.iter().position(|f| f.loadout_code == build.loadout_code) {
            self.favorites.remove(pos);
            let msg = match self.lang {
                AppLang::Vi => format!("Đã bỏ ghim cấu hình [{}]", build.weapon),
                AppLang::En => format!("Unpinned loadout for [{}]", build.weapon),
            };
            self.status_message = Some((msg, Instant::now(), false));
        } else {
            let mut fav = build.clone();
            fav.is_favorite = true;
            self.favorites.insert(0, fav);
            let msg = match self.lang {
                AppLang::Vi => format!("Đã ghim cấu hình [{}] vào Yêu thích!", build.weapon),
                AppLang::En => format!("Pinned [{}] loadout to Favorites!", build.weapon),
            };
            self.status_message = Some((msg, Instant::now(), false));
        }
        save_favorites(&self.favorites);
        self.sync_shared_favorites();
    }

    fn delete_custom_build(&mut self, code: &str) {
        let mut removed_weapon = String::new();
        if let Some(pos) = self.favorites.iter().position(|f| f.loadout_code == code) {
            removed_weapon = self.favorites[pos].weapon.clone();
            self.favorites.remove(pos);
        }
        save_favorites(&self.favorites);
        self.sync_shared_favorites();
        let msg = match self.lang {
            AppLang::Vi => format!("Đã xóa mã cá nhân cho [{}]", removed_weapon),
            AppLang::En => format!("Deleted custom loadout for [{}]", removed_weapon),
        };
        self.status_message = Some((msg, Instant::now(), false));
    }

    fn save_current_config(&self) {
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
            selected_monitor_index: self.selected_monitor_index,
        };
        save_user_config(&cfg);
    }

    fn sync_active_filters(&self) {
        if let Ok(mut f) = self.active_filters.write() {
            f.server = self.selected_server;
            f.mode = self.selected_mode;
            f.source = self.selected_source;
            f.selected_monitor_index = self.selected_monitor_index;
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
        save_history(&self.history);
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
        let copy_res = match Clipboard::new() {
            Ok(mut cb) => copy_to_clipboard_with_retry(&mut cb, code, 3),
            Err(e) => Err(e.to_string()),
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
        success_msg: String,
        err_prefix: String,
    ) {
        let tx = self.tx_events.clone();
        let ctx = self.egui_ctx.clone();

        thread::spawn(move || {
            let codes: Vec<String> = selected.iter().map(|b| b.loadout_code.clone()).collect();
            let mut cb_opt = Clipboard::new().ok();
            match copy_codes_sequence_to_clipboard(&mut cb_opt, &codes, 150) {
                Ok(()) => {
                    unsafe {
                        let _ = MessageBeep(MB_OK);
                    }
                    let _ = tx.send(WorkerEvent::WinVCopyCompleted {
                        top_build: selected.into_iter().next(),
                        msg: success_msg,
                        is_error: false,
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
        let count = selected.len();
        let success_msg = match self.lang {
            AppLang::Vi => format!("Đã đẩy {} cấu hình của {} vào Win + V (Meta ở đầu)!", count, weapon_name),
            AppLang::En => format!("Pushed {} builds for {} to Win + V (Meta at top)!", count, weapon_name),
        };
        let err_prefix = match self.lang {
            AppLang::Vi => format!("Lỗi ghi Clipboard cho {}", weapon_name),
            AppLang::En => format!("Clipboard write error for {}", weapon_name),
        };
        self.spawn_win_v_copy_worker(selected, success_msg, err_prefix);
    }

    fn copy_all_favorites_to_win_v(&mut self) {
        let selected: Vec<BuildItem> = self.favorites.iter().take(5).cloned().collect();
        if selected.is_empty() {
            return;
        }
        let count = selected.len();
        let success_msg = match self.lang {
            AppLang::Vi => format!("Đã đẩy {} cấu hình yêu thích vào Win + V!", count),
            AppLang::En => format!("Pushed {} favorite builds to Win + V!", count),
        };
        let err_prefix = self.lang.t("Lỗi ghi Clipboard cho Yêu thích", "Clipboard write error for Favorites").to_string();
        self.spawn_win_v_copy_worker(selected, success_msg, err_prefix);
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
                                        let tmp_path = local_builds_tmp_file_path();
                                        let serialized = serde_json::to_string(&valid_items).unwrap_or(body);
                                        match fs::write(&tmp_path, serialized) {
                                            Ok(_) => {
                                                if fs::rename(&tmp_path, &local_path).is_err() {
                                                    let _ = fs::copy(&tmp_path, &local_path);
                                                    let _ = fs::remove_file(&tmp_path);
                                                }
                                                let _ = tx.send(DbUpdateResult::Success {
                                                    builds: valid_items,
                                                    count,
                                                });
                                            }
                                            Err(e) => {
                                                let _ = tx.send(DbUpdateResult::Error(format!("Lỗi ghi file tạm: {}", e)));
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
                } => {
                    self.selected_weapon = Some(weapon.clone());
                    self.current_tab = AppTab::Scanner;
                    let s = self.selected_server.as_str();
                    let m = self.selected_mode.as_str();
                    let src = self.selected_source.as_str();
                    let is_empty = builds.is_empty();

                    if is_uncertain {
                        self.pending_confirm_builds = Some((weapon.clone(), builds));
                        self.pending_confirm_score = Some(score);
                        let msg = match self.lang {
                            AppLang::Vi => format!(
                                "Nghi ngờ nhận diện: [{}] ({:.1}%) — Bấm [✔ XÁC NHẬN & COPY] để sao chép (OCR: '{}')",
                                weapon,
                                score * 100.0,
                                raw_ocr.trim()
                            ),
                            AppLang::En => format!(
                                "Suspicious match: [{}] ({:.1}%) — Click [✔ CONFIRM & COPY] to copy (OCR: '{}')",
                                weapon,
                                score * 100.0,
                                raw_ocr.trim()
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
                        let msg = match self.lang {
                            AppLang::Vi => {
                                if is_empty {
                                    format!("Đã nhận diện súng [{}] ({:.1}%) nhưng chưa có cấu hình cho bộ lọc hiện tại [{}/{}/{}]", weapon, score * 100.0, src, s, m)
                                } else {
                                    format!("Nhận diện: {} ({:.1}%) [{}/{}/{}] - OCR: '{}'", weapon, score * 100.0, src, s, m, raw_ocr.trim())
                                }
                            }
                            AppLang::En => {
                                if is_empty {
                                    format!("Recognized gun [{}] ({:.1}%) but no builds found for current filter [{}/{}/{}]", weapon, score * 100.0, src, s, m)
                                } else {
                                    format!("Recognized: {} ({:.1}%) [{}/{}/{}] - OCR: '{}'", weapon, score * 100.0, src, s, m, raw_ocr.trim())
                                }
                            }
                        };
                        self.status_message = Some((msg, Instant::now(), is_empty));

                        if let Some(top_build) = builds.first() {
                            self.record_history_entry(
                                &weapon,
                                &top_build.classification,
                                top_build.copy_count,
                                &top_build.loadout_code,
                            );
                        }
                    }
                }
                WorkerEvent::ScanFailed { raw_ocr, cleaned } => {
                    self.pending_confirm_builds = None;
                    self.pending_confirm_score = None;
                    let msg = match self.lang {
                        AppLang::Vi => format!("Không nhận diện được súng! (OCR: '{}' | Xử lý: '{}')", raw_ocr.trim(), cleaned),
                        AppLang::En => format!("Weapon not recognized! (OCR: '{}' | Cleaned: '{}')", raw_ocr.trim(), cleaned),
                    };
                    self.status_message = Some((msg, Instant::now(), true));
                }
                WorkerEvent::WinVCopyCompleted {
                    top_build,
                    msg,
                    is_error,
                } => {
                    if !is_error {
                        if let Some(top) = top_build {
                            self.record_history_entry(
                                &top.weapon,
                                &top.classification,
                                top.copy_count,
                                &top.loadout_code,
                            );
                        }
                    }
                    self.status_message = Some((msg, Instant::now(), is_error));
                }
                WorkerEvent::OcrUnavailable => {
                    self.ocr_available = false;
                    let msg = match self.lang {
                        AppLang::Vi => format!("CẢNH BÁO: Windows thiếu gói OCR English (US)! Phím {} bị tắt. Vui lòng cài đặt 'English (United States)' trong Windows Settings > Time & Language. Bạn vẫn có thể tra cứu thủ công bên dưới.", self.hotkey_name),
                        AppLang::En => format!("WARNING: Windows OCR language package missing! {} is disabled. Please install 'English (United States)' in Windows Settings > Time & Language. Manual lookup is still available.", self.hotkey_name),
                    };
                    self.status_message = Some((msg, Instant::now(), true));
                }
                WorkerEvent::NewVersionAvailable(tag) => {
                    self.new_version_available = Some(tag);
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
                        right: 10.0,
                        top: 8.0,
                        bottom: 8.0,
                    }),
            )
            .show(ctx, |ui| {
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

                    ui.add_space(4.0);
                    ui.add(egui::Label::new(RichText::new("|").color(COLOR_BORDER).size(13.0)).selectable(false));
                    ui.add_space(4.0);

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

                    ui.add_space(6.0);
                    ui.add(egui::Label::new(RichText::new("|").color(COLOR_BORDER).size(13.0)).selectable(false));
                    ui.add_space(6.0);

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
                        .inner_margin(Margin::symmetric(6.0, 2.0))
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
                                    AppLang::Vi => format!("Windows thiếu gói OCR en-US. Phím {} bị tắt. Vui lòng cài gói ngôn ngữ English (United States) trong Windows Settings.", self.hotkey_name),
                                    AppLang::En => format!("Windows OCR package (en-US) missing. {} is disabled. Please install English (United States) in Windows Settings.", self.hotkey_name),
                                });
                            }
                        });

                    ui.add_space(4.0);

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
                        ui.add_space(2.0);

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

                        ui.add_space(2.0);

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

                        ui.add_space(2.0);

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

                        ui.add_space(2.0);

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

                        ui.add_space(2.0);

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

                        ui.add_space(2.0);

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
                                self.lang.t("[ĐANG TẢI...]", "[UPDATING...]").to_string(),
                                false,
                                COLOR_AMBER,
                            )
                        } else if let Some(rem_secs) = remaining_cooldown_secs {
                            let rem_mins = (rem_secs + 59) / 60;
                            let label = match self.lang {
                                AppLang::Vi => format!("[CHỜ {}M]", rem_mins),
                                AppLang::En => format!("[WAIT {}M]", rem_mins),
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

                        // Prominent New Version Badge Button if an update is available on GitHub
                        if let Some(new_ver) = &self.new_version_available {
                            ui.add_space(2.0);
                            let ver_label = match self.lang {
                                AppLang::Vi => format!("[BẢN MỚI {}]", new_ver),
                                AppLang::En => format!("[NEW {}]", new_ver),
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
                                AppLang::Vi => format!("Đã có phiên bản mới {} trên GitHub! Bấm để mở trang tải về.", new_ver),
                                AppLang::En => format!("New release {} is available on GitHub! Click to open download page.", new_ver),
                            };

                            if ui.add(ver_btn).on_hover_text(ver_tooltip).clicked() {
                                ctx.open_url(egui::OpenUrl::same_tab("https://github.com/TaiGiap/df-loadout-scanner/releases/latest"));
                            }
                        }
                    });
                });
            });

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
                    let monitor_label = match self.selected_monitor_index {
                        None => self.lang.t("Tự động (Game)", "Auto (Game)").to_string(),
                        Some(idx) => self
                            .available_monitors
                            .iter()
                            .find(|(i, _)| *i == idx)
                            .map(|(_, name)| name.clone())
                            .unwrap_or_else(|| format!("{} {}", self.lang.t("Màn", "Display"), idx + 1)),
                    };
                    let prev_mon = self.selected_monitor_index;
                    let cb_resp = egui::ComboBox::from_id_salt("filter_monitor_cb")
                        .selected_text(RichText::new(monitor_label).size(12.0))
                        .width(155.0)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.selected_monitor_index,
                                None,
                                self.lang.t("Tự động (Theo cửa sổ game)", "Auto (Active Window)"),
                            );
                            for (idx, name) in &self.available_monitors {
                                ui.selectable_value(&mut self.selected_monitor_index, Some(*idx), name);
                            }
                        });
                    if cb_resp.response.clicked() {
                        self.available_monitors = enumerate_monitors(self.lang);
                    }
                    if prev_mon != self.selected_monitor_index {
                        self.sync_active_filters();
                        self.save_current_config();
                    }
                });
            });

        let mut confirm_pending = false;
        let mut cancel_pending = false;

        // Bottom Status Bar
        egui::TopBottomPanel::bottom("status_bar")
            .frame(
                Frame::default()
                    .fill(COLOR_CARD)
                    .stroke(Stroke::new(1.0_f32, COLOR_BORDER))
                    .inner_margin(Margin::symmetric(14.0, 6.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if let Some((msg, _time, is_error)) = &self.status_message {
                        let icon = if *is_error { "[!] " } else { "[i] " };
                        let color = if *is_error { COLOR_AMBER } else { COLOR_ACCENT };
                        ui.add(egui::Label::new(RichText::new(format!("{}{}", icon, msg)).color(color).size(12.0)).selectable(false));
                    }

                    if self.pending_confirm_builds.is_some() {
                        ui.add_space(8.0);
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
                                "Nhận diện ở ngưỡng nghi ngờ (75%-89%). Bấm để xác nhận súng này và sao chép vào Clipboard",
                                "Suspicious OCR match (75%-89%). Click to confirm weapon and copy loadouts to Clipboard"
                            ))
                            .clicked()
                        {
                            confirm_pending = true;
                        }

                        let cancel_btn = egui::Button::new(
                            RichText::new("✕")
                                .color(COLOR_TEXT_MUTED)
                                .size(11.0),
                        )
                        .fill(Color32::TRANSPARENT);

                        if ui.add(cancel_btn).clicked() {
                            cancel_pending = true;
                        }
                    }

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
                    });
                });
            });

        if confirm_pending {
            if let Some((weapon, builds)) = self.pending_confirm_builds.take() {
                if !builds.is_empty() {
                    let codes: Vec<String> = builds.iter().map(|b| b.loadout_code.clone()).collect();
                    let mut cb_opt = Clipboard::new().ok();
                    match copy_codes_sequence_to_clipboard(&mut cb_opt, &codes, 150) {
                        Ok(()) => {
                            unsafe {
                                let _ = MessageBeep(MB_OK);
                            }
                            if let Some(top_build) = builds.first() {
                                self.record_history_entry(
                                    &weapon,
                                    &top_build.classification,
                                    top_build.copy_count,
                                    &top_build.loadout_code,
                                );
                            }
                            let msg = match self.lang {
                                AppLang::Vi => format!("Đã xác nhận súng [{}] và sao chép cấu hình vào Clipboard!", weapon),
                                AppLang::En => format!("Confirmed weapon [{}] and copied loadout to Clipboard!", weapon),
                            };
                            self.status_message = Some((msg, Instant::now(), false));
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
                            save_history(&self.history);
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
                            ui.label(RichText::new(format!("• Quét tự động ({}): Vào xem bảng chỉ số súng trong game và nhấn {}. App sẽ đọc tên súng qua OCR và đẩy các cấu hình (Hot nhất ở đầu) vào Win + V.", self.hotkey_name, self.hotkey_name)).color(COLOR_TEXT_MAIN).size(11.0));
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
                            ui.label(RichText::new(format!("• Auto Scan ({}): Inspect weapon stats in-game and press {}. Native OCR reads weapon and feeds builds (Top copies first) into Win + V.", self.hotkey_name, self.hotkey_name)).color(COLOR_TEXT_MAIN).size(11.0));
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
                                        "Phần mềm được phát hành mã nguồn mở hoàn toàn (MIT License) và miễn phí 100% cho cộng đồng — mọi khoản ủng hộ (trà tắc/cà phê) từ người dùng đều hoàn toàn tự nguyện và không khóa bất kỳ tính năng nào. Hoạt động hoàn toàn độc lập với Team Jade / TiMi Studio Group / Garena và các trang web nguồn. Tiện ích hoạt động thuần túy qua Windows Native OCR API và Clipboard hệ thống, không chứa mã nguồn hay tài sản game.",
                                        "The software is 100% free and Open Source (MIT License) for the community — any support/donations (tea/coffee) from users are entirely voluntary and do not lock any features. Completely independent from Team Jade / TiMi Studio Group / Garena and source websites. Operates strictly via native Windows OCR APIs and Clipboard, containing no game code or proprietary assets."
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
                let copy_res = match arboard::Clipboard::new() {
                    Ok(mut cb) => copy_to_clipboard_with_retry(&mut cb, DONATE_ACCOUNT_NO, 3),
                    Err(e) => Err(e.to_string()),
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
                    save_favorites(&self.favorites);
                    self.sync_shared_favorites();

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
            ("Shotgun Hai Nòng", "725 DOUBLE-BARREL"),
            ("Cung phức hợp", "COMPOUND BOW"),
        ];

        for (raw_ocr, expected) in test_cases {
            let cleaned = clean_ocr_text(raw_ocr);
            let matched = match_weapon(&cleaned, &candidates);
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
        let cleaned_m249 = clean_ocr_text("M249 Light Machine Gun");
        let matched_m249 = match_weapon(&cleaned_m249, &candidates).unwrap();
        assert_eq!(matched_m249.0, "M249");
        assert_ne!(matched_m249.0, "MINI-14");

        // M14 must NOT be identified as M16A4
        let cleaned_m14 = clean_ocr_text("M14 Marksman Rifle");
        let matched_m14 = match_weapon(&cleaned_m14, &candidates).unwrap();
        assert_eq!(matched_m14.0, "M14");
        assert_ne!(matched_m14.0, "M16A4");
    }

    #[test]
    fn test_vietnamese_weapon_names() {
        let candidates = get_test_candidates();

        let vi_cases = [
            ("G3 Súng trường chiến đấu", "G3"),
            ("M249 Súng máy hạng nhẹ", "M249"),
            ("PKM Súng máy đa năng", "PKM"),
            ("PKM Súng máy đa dụng", "PKM"),
            ("M14 Súng trường thiện xạ", "M14"),
            ("R93 Súng bắn tỉa", "R93"),
            ("Hai Nòng", "725 DOUBLE-BARREL"),
            ("Súng săn Hai Nòng", "725 DOUBLE-BARREL"),
            ("Cung", "COMPOUND BOW"),
            ("Nỏ phức hợp", "COMPOUND BOW"),
        ];

        for (raw_ocr, expected) in vi_cases {
            let cleaned = clean_ocr_text(raw_ocr);
            let matched = match_weapon(&cleaned, &candidates);
            assert!(
                matched.is_some(),
                "Failed to match Vietnamese OCR '{}' (cleaned: '{}')",
                raw_ocr,
                cleaned
            );
            let (weapon, _) = matched.unwrap();
            assert_eq!(
                weapon, expected,
                "For Vietnamese OCR '{}', expected '{}' but got '{}'",
                raw_ocr, expected, weapon
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
    fn test_v136_error_cases_and_skin_stripping() {
        let candidates = get_test_candidates();

        let cases = [
            // 1. G3
            ("G3 Battle Rifle", "G3"),
            ("Chơi quá 180 phút G3 Battle Rifle", "G3"),
            ("qua G3 Battle Rifle", "G3"),
            ("G3 Cổ điển", "G3"),
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
            ("M82 Hạng nặng", "M82"),
            // 6. M7
            ("M7 Battle Rifle - Đỏ", "M7"),
            ("M7 Battle Rifle", "M7"),
            ("180 phút M7 Battle Rifle", "M7"),
            ("M7 Súng trường gọn nhẹ", "M7"),
            // 7. MK4
            ("MK4 SMG", "MK4"),
            ("MK4 Chiến thuật", "MK4"),
            ("MK4", "MK4"),
            // 8. R93
            ("R93 Sniper Rifle", "R93"),
            ("R93 Súng bắn tỉa", "R93"),
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
            ("Shotgun Hai Nòng", "725 DOUBLE-BARREL"),
            ("Hai Nòng", "725 DOUBLE-BARREL"),
            ("Súng săn Hai Nòng", "725 DOUBLE-BARREL"),
            ("725 Double-Barrel - Sa Mạc", "725 DOUBLE-BARREL"),
            ("Double-Barrel", "725 DOUBLE-BARREL"),
            ("725", "725 DOUBLE-BARREL"),
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
    fn test_description_weapon_extraction_vi_en() {
        let candidates = get_test_candidates();

        let desc_cases = [
            // Prompt core examples
            ("M7 là súng trường chiến đấu thế hệ mới sử dụng cỡ đạn...", "M7"),
            ("The AR-57 uses 5.7x28mm rounds with high muzzle velocity...", "AR-57"),
            ("Súng Shotgun Hai Nòng có uy lực cận chiến vượt trội...", "725 DOUBLE-BARREL"),
            ("SR-3M là Assault Rifle cơ động cao...", "SR-3M"),
            // Additional Vietnamese/English variants & leading stop words
            ("Khẩu M14 là súng trường thiện xạ bán tự động...", "M14"),
            ("Một khẩu PSG-1 trang bị kính ngắm tầm xa...", "PSG-1"),
            ("Khẩu súng trường chiến đấu G3 sử dụng đạn 7.62x51mm...", "G3"),
            ("Súng săn Hai Nòng sử dụng đạn ghém 12 gauge...", "725 DOUBLE-BARREL"),
            ("Cung phức hợp có thể bắn tên nổ tầm xa...", "COMPOUND BOW"),
            ("The 725 Double-Barrel is a break-action shotgun...", "725 DOUBLE-BARREL"),
            ("The .357 Revolver fires heavy magnum rounds...", ".357"),
            ("The Battle Rifle SCAR-H delivers high stopping power...", "SCAR-H"),
            ("Desert Eagle là súng lục hạng nặng...", "DESERT EAGLE"),
            ("MCX LT là súng trường tấn công sử dụng đạn .300 Blackout...", "MCX LT"),
            ("Marlin Lever-Action là súng trường đòn bẩy cổ điển...", "MARLIN LEVER-ACTION"),
            ("Súng Shotgun Hai Nòng.", "725 DOUBLE-BARREL"),
        ];

        for (raw_desc, expected) in desc_cases {
            let cleaned = extract_description_weapon_text(raw_desc);
            let matched = match_weapon(&cleaned, &candidates);
            assert!(
                matched.is_some(),
                "Failed to recognize weapon from description '{}' (cleaned: '{}')",
                raw_desc,
                cleaned
            );
            let (weapon, score) = matched.unwrap();
            assert_eq!(
                weapon, expected,
                "Description '{}' (cleaned: '{}'): expected '{}' but got '{}'",
                raw_desc, cleaned, expected, weapon
            );
            assert!(
                score >= 0.90,
                "Description '{}' matched '{}' with low confidence {:.2}",
                raw_desc, weapon, score
            );
        }

        // Common prose words like "Cùng", "Nó", "No" must NOT false-trigger COMPOUND BOW
        for prose in [
            "Cùng với độ giật thấp giúp kiểm soát ổn định",
            "Nó sử dụng loại đạn đặc biệt tầm xa",
            "No recoil when firing short controlled bursts",
        ] {
            let cleaned = extract_description_weapon_text(prose);
            if let Some((w, _)) = match_weapon(&cleaned, &candidates) {
                assert_ne!(w, "COMPOUND BOW", "Prose '{}' falsely matched COMPOUND BOW (cleaned: '{}')", prose, cleaned);
            }
        }
    }

    #[test]
    fn test_left_card_skin_stripping_and_border_detection() {
        let candidates = get_test_candidates();

        // 1. Skin suffix stripping across ASCII hyphen, en-dash, em-dash, multiline
        let skin_cases = [
            ("M7 Battle Rifle - Đỏ", "M7"),
            ("PSG-1 Marksman Rifle - Nghệ Thuật Đương Đại", "PSG-1"),
            ("PSG-1 Marksman Rifle – Nghệ Thuật Đương Đại", "PSG-1"),
            ("M7 Battle Rifle — Huyền Thoại\nLv.45", "M7"),
            ("M4A1 Assault Rifle- Đỏ Crimson", "M4A1"),
            ("CAR-15 Carbine -Sa Mạc", "CAR-15"),
            ("725 Double-Barrel - Bạch Kim", "725 DOUBLE-BARREL"),
            ("Shotgun Hai Nòng – Hỏa Lực", "725 DOUBLE-BARREL"),
        ];

        for (raw_card, expected) in skin_cases {
            let cleaned = clean_ocr_text(raw_card);
            let matched = match_weapon(&cleaned, &candidates);
            assert!(
                matched.is_some(),
                "Failed to match left-card title '{}' (cleaned: '{}')",
                raw_card,
                cleaned
            );
            let (weapon, score) = matched.unwrap();
            assert_eq!(
                weapon, expected,
                "Card title '{}' expected '{}' got '{}'",
                raw_card, expected, weapon
            );
            assert!(score >= 0.90, "Expected >= 0.90 for '{}' got {:.2}", raw_card, score);
        }

        // 2. Geometric 4-edge white border detection on synthetic 1920x1080 frame
        let mut img = xcap::image::RgbaImage::from_pixel(1920, 1080, xcap::image::Rgba([22, 26, 32, 255]));

        // Blank dark frame should return None
        assert!(find_selected_card_title_rect(&img).is_none());

        // Draw an unselected dim card near top (y=200..320) with a bright gold skin accent bar
        for x in 150..430 {
            img.put_pixel(x, 200, xcap::image::Rgba([70, 75, 80, 255]));
            img.put_pixel(x, 320, xcap::image::Rgba([70, 75, 80, 255]));
            // Gold rarity bar inside unselected card (bright R/G, low B)
            for y in 280..286 {
                img.put_pixel(x, y, xcap::image::Rgba([255, 205, 30, 255]));
            }
        }

        // Draw selected white-bordered card scrolled lower on list (x=150..430, y=580..704)
        let card_x1 = 150u32;
        let card_x2 = 430u32;
        let card_y1 = 580u32;
        let card_y2 = 704u32;
        let white = xcap::image::Rgba([245, 245, 245, 255]);

        for x in card_x1..=card_x2 {
            img.put_pixel(x, card_y1, white);
            img.put_pixel(x, card_y1 + 1, white);
            img.put_pixel(x, card_y2 - 1, white);
            img.put_pixel(x, card_y2, white);
        }
        for y in card_y1..=card_y2 {
            img.put_pixel(card_x1, y, white);
            img.put_pixel(card_x1 + 1, y, white);
            img.put_pixel(card_x2 - 1, y, white);
            img.put_pixel(card_x2, y, white);
        }

        let rect = find_selected_card_title_rect(&img);
        assert!(rect.is_some(), "Failed to locate white-bordered selected card");
        let (tx, ty, tw, th) = rect.unwrap();
        assert!(tx > card_x1, "Title X ({}) should be inside left border ({})", tx, card_x1);
        assert!(tx + tw < card_x2, "Title right ({}) should be inside right border ({})", tx + tw, card_x2);
        assert!(ty > card_y1 && ty < card_y1 + 16, "Title Y ({}) should sit right below top border ({})", ty, card_y1);
        assert!(th >= 30 && ty + th < card_y2, "Title height ({}) should fit inside card", th);
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
        let validated_embedded = validate_downloaded_builds(embedded).expect("embedded should pass");
        assert_eq!(validated_embedded.len(), total);
    }
}

