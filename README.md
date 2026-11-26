# DF Loadout Scanner

Công cụ **mã nguồn mở (Open Source — MIT License)** tự động nhận diện vũ khí thời gian thực bằng AI **PaddleOCR v4 (ONNX Runtime CPU ngoại tuyến)** và đồng bộ mã trang bị (*loadout code*) cho **Delta Force** (*Warfare* & *Operations* — Garena / Level Infinite / Steam).

> **Tác giả:** Anh Bảy Chọ / Tài Giáp  
> **Phiên bản hiện tại:** `v2.0.2-beta` | **Kho mã nguồn:** [TaiGiap/df-loadout-scanner](https://github.com/TaiGiap/df-loadout-scanner)

---

## 📦 Tải Về & Sử Dụng Nhanh (3 Bước)

### 1. Tải về & Giải nén
- Tải gói phát hành mới nhất tại [GitHub Releases](https://github.com/TaiGiap/df-loadout-scanner/releases/latest):
  **`DF-Loadout-Scanner-v2.0.2-beta-Windows-x64.zip`**
- Giải nén **toàn bộ tệp trong gói** (`DF-Loadout-Scanner-Windows-x64.exe` và `onnxruntime.dll`) vào **cùng một thư mục**. Không cần cài thêm gói ngôn ngữ Windows OCR.

### 2. Thiết lập game & Windows
- Trong *Delta Force*, chuyển hiển thị sang **Borderless Windowed (Không viền)**.
- Trên Windows, bấm `Win + V` để bật **Clipboard History** (lưu nhiều mã trang bị cùng lúc).

### 3. Khởi chạy & Quét súng
- Chạy `DF-Loadout-Scanner-Windows-x64.exe` (nếu Windows SmartScreen hiện cảnh báo, chọn **More info** $\rightarrow$ **Run anyway**).
- Vào màn hình tùy biến vũ khí trong game và bấm phím tắt **`F9`** (hoặc phím đã đổi qua `[⌨]`):
  - **`Ctrl + V`**: dán ngay bản build tối ưu nhất.
  - **`Win + V`**: chọn nhanh các bản build đề xuất khác trong hàng đợi.

---

## ✨ Tính Năng Nổi Bật

- **AI PaddleOCR v4 ONNX Ngoại Tuyến (`en_PP-OCRv4_rec.onnx` + `onnxruntime.dll`):**
  - Chạy hoàn toàn offline trên CPU (~15ms/lần quét), hỗ trợ mọi tỷ lệ màn hình (`16:9`, `21:9` Ultrawide, `16:10`) nhờ quy đổi khung HUD `16:9` trung tâm (`x: 24.0%–41.5%`, `y: 15.0%–19.6%`).
  - Tiền xử lý RGB không ngưỡng trắng kết hợp quy trình 2 tầng (**Pass 1A** $\rightarrow$ **Pass 1B** kéo tương phản + gamma `0.55` vùng `18+` $\rightarrow$ **Pass 2** đọc góc trên trái thẻ đang chọn né tên skin) và **khóa cứng Loại súng** (`Shotgun Hai Nòng`, `Cung Phức Hợp`, `Lever-action`, `S12KU` $\rightarrow$ `S12K`...).
- **Auto-Copy `>= 75%` Có Kiểm Soát (3 Rào Chắn An Toàn):**
  - Chỉ tự động nạp Clipboard khi đạt đồng thời: đã khóa loại súng (`category_locked`), chênh lệch Top-1/Top-2 đạt ngưỡng (`>= 0.05` khi `score >= 0.80`, `>= 0.12` dưới `0.80`), mã `>= 2` ký tự và `ctc_confidence >= 0.55`. Thiếu điều kiện sẽ hiện nút **`[✔ XÁC NHẬN & COPY]`**.
- **Bộ Lọc 5 Cấp Độ & Tìm Kiếm Thông Minh:**
  - Lọc theo **Nguồn** (`ALL`, `DFTools`, `DFBuilds`, `FPSMeta`), **Server** (`Garena`, `Infinity`), **Chế độ** (`Warfare`, `Operations`), **Loại súng** và **Tên súng** (hỗ trợ tìm không dấu gạch ngang như `ar57`, `mini14` và biệt danh tiếng Việt trên ô tìm kiếm).
- **Tab Yêu Thích (`favorites.json`), Lịch Sử & `Win + V`:**
  - Ghim bản build ưu tiên số 1, thêm mã cá nhân (`[+ THÊM MÃ RIÊNG]`), thoát nhanh bằng `ESC`, xem lịch sử quét và nạp hàng đợi 5 mã vào `Win + V`.
- **Kiểm Tra Cập Nhật Tự Động (GitHub Releases API):**
  - Tự động kiểm tra bản mới khi mở app (SemVer hỗ trợ `-beta`), nút **`[KIỂM TRA CẬP NHẬT]`** trên thanh công cụ, banner thông báo kèm nút mở trang Release hoặc tải gói `.zip` an toàn vào `DF-Loadout-Scanner-Data/downloads/`.
- **Ghi Dữ Liệu Nguyên Tử (`atomic_write_verified`) & An Toàn Kỹ Thuật:**
  - Xác nhận dung lượng `> 0` bytes trước khi thay thế `builds_local.json` / `favorites.json` / `config.json`. Không đọc RAM game, không DLL injection, không macro gameplay (chỉ dùng Windows API chụp màn hình và Clipboard).

---

## ⌨️ Phím Tắt & Tùy Chọn Nâng Cao

| Mục | Mô tả |
| :--- | :--- |
| **`F9`** (mặc định) | Phím tắt quét vũ khí toàn cục. Đổi sang `F1`–`F12`, `Numpad`, `Mouse 4` / `Mouse 5` qua nút **`[⌨]`** (`Esc` để hủy). |
| **`ESC`** | Quay lại màn hình quét chính khi đang ở Tab `★ YÊU THÍCH`. |
| **`DF_OCR_DEBUG=1`** | Biến môi trường (mặc định TẮT). Khi bật `DF_OCR_DEBUG=1`, lưu ảnh cắt `pass1a_center.png`, `pass1b_boosted.png`, `pass2_leftcard.png` và `decision.log` vào `DF-Loadout-Scanner-Data/debug_ocr/`. |
| **`DF-Loadout-Scanner-Data/config.json`** | Lưu cấu hình bộ lọc, ngôn ngữ (`vi`/`en`), phím tắt, `selected_monitor_id` và `auto_check_update` (mặc định `true`). |

---

## 🛠️ Tự Biên Dịch Từ Mã Nguồn (Build from Source)

Yêu cầu [Rust & Cargo](https://rustup.rs/) và tệp `onnxruntime.dll` (ONNX Runtime v1.31.x Windows x64 CPU) đặt trong `target/debug/` hoặc cạnh file `.exe`:

```powershell
git clone https://github.com/TaiGiap/df-loadout-scanner.git
cd df-loadout-scanner
cargo test
cargo build --release
```

---

## 📋 Lịch Sử Cập Nhật / Changelog

| Phiên bản | Điểm mới & Cải tiến chính |
| :--- | :--- |
| **v2.0.2-beta (Mới nhất)** | Ngoại lệ `Shotgun Hai Nòng` $\rightarrow$ `725 DOUBLE-BARREL`; sửa tiền tố loại súng & dọn ký tự rác đuôi (`Shotgun S12KU` $\rightarrow$ `S12K`); tinh chỉnh rào chắn Auto-Copy (`margin >= 0.05` khi `score >= 0.80`) cho `CAR-15`, `G3`, `SVD`, `R93`; tích hợp kiểm tra cập nhật GitHub Releases (SemVer `-beta`, banner thông báo, nút `[KIỂM TRA CẬP NHẬT]` tách gọn trên Header, tải `.zip` vào `downloads/`). |
| **v2.0.1-beta** | Tiền xử lý RGB không ngưỡng trắng + khung HUD `16:9` đa tỷ lệ (`x: 24.0%–41.5%, y: 15.0%–19.6%`); quy trình 2 tầng Pass 1A / Pass 1B (gamma `0.55`) / Pass 2 (thẻ trái góc trên + Loại súng đã khóa); khóa cứng Loại súng; Auto-Copy `>= 75%` với 3 rào chắn; sửa tràn Status Bar và thêm `DF_OCR_DEBUG=1`. |
| **v2.0.0-beta** | Thay Windows OCR bằng lõi **PaddleOCR v4 Offline (`en_PP-OCRv4_rec.onnx` + `onnxruntime.dll`)** chạy trên CPU; khóa vùng quét tiêu đề giữa và khớp cặp `[Hậu tố mã súng + Loại súng]`. |
| **v1.4.6** | `atomic_write_verified` chống mất DB; định danh màn hình ổn định (`selected_monitor_id`); gia cố Clipboard & `Win + V`; kiểm duyệt ngữ nghĩa dữ liệu phía Rust. |
| **v1.4.5** | Tự động nhận diện màn hình đang chạy game (`GetForegroundWindow` / `MonitorFromWindow`); Margin Check súng tên ngắn; kiểm tra hợp lệ DB khi tải về. |
| **v1.4.4** | Mô hình quét 3 tầng né bảng cảnh báo `18+`. |
| **v1.4.3** | Chia thanh `BỘ LỌC` thành 2 hàng gọn gàng; tự động ưu tiên chọn `M4A1` khi khởi động. |
| **v1.4.2** | Sửa lỗi đè ô tìm kiếm trên Header; hỗ trợ tìm kiếm không dấu gạch ngang và bí danh tiếng Việt trên UI. |
| **v1.4.1** | Tách Tab chuyên biệt `[★ YÊU THÍCH]`, nút `[🔍 Xem súng]` và thoát nhanh bằng `ESC`. |
| **v1.4.0** | Lưu trữ cấu hình yêu thích & mã cá nhân độc lập (`favorites.json`) cùng nút `[✔ XÁC NHẬN & COPY]`. |

---

## 📄 Nguồn Dữ Liệu, Bản Quyền & Miễn Trừ Trách Nhiệm

- **Nguồn dữ liệu cộng đồng:** Tổng hợp từ [DeltaForceTools](https://deltaforcetools.gg/vi/weapon-builds), [DeltaForceBuilds](https://deltaforcebuilds.com) và [FPSMeta](https://fpsmeta.com) ([fpsmeta.gg](https://fpsmeta.gg/delta-force-meta)). Mọi yêu cầu gỡ bỏ dữ liệu xin gửi qua [GitHub Issues](https://github.com/TaiGiap/df-loadout-scanner/issues) hoặc [tai.gd361@gmail.com](mailto:tai.gd361@gmail.com).
- **Giấy phép:** Phát hành theo **MIT License** — xem [LICENSE](LICENSE) (`Copyright (c) 2026 Giáp Đức Tài`).
- **Miễn trừ trách nhiệm:** Công cụ độc lập hoạt động qua chụp ảnh màn hình, PaddleOCR v4 offline và Clipboard hệ thống; không chứa mã nguồn hay tài sản trí tuệ của *Delta Force* (Team Jade / TiMi Studio Group / Garena).
