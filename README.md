# DF Loadout Scanner

Công cụ tự động nhận diện vũ khí thời gian thực qua Windows OCR và đồng bộ mã trang bị cho game Delta Force (Garena & Level Infinite / Steam).

*Real-time weapon OCR scanner and loadout clipboard synchronizer for Delta Force.*

---

## Tải Về & Cài Đặt Nhanh / Quick Download

Người dùng thông thường chỉ cần tải bản đóng gói sẵn trên GitHub Releases, không cần cài đặt môi trường Rust:

- **Tải phiên bản mới nhất (Latest Release):** [DF-Loadout-Scanner Releases](https://github.com/TaiGiap/df-loadout-scanner/releases/latest)
  - `DF-Loadout-Scanner-Windows-x64.zip` (Thư mục nén đóng gói đầy đủ, khuyến nghị).
  - `DF-Loadout-Scanner-Windows-x64.exe` (File thực thi độc lập Portable).
- **Hướng dẫn:** Giải nén hoặc mở trực tiếp file `DF-Loadout-Scanner-Windows-x64.exe` để chạy. Ứng dụng sẽ tự động tạo thư mục con `DF-Loadout-Scanner-Data` để lưu trữ dữ liệu cấu hình một cách ngăn nắp.

*For regular users, download the pre-built release package directly from [GitHub Releases](https://github.com/TaiGiap/df-loadout-scanner/releases/latest). You can choose between the recommended `DF-Loadout-Scanner-Windows-x64.zip` archive or the standalone `DF-Loadout-Scanner-Windows-x64.exe` binary (portable, no installation required).*

---

## Tác giả / Developed By

- **Anh Bảy Chọ / Tài Giáp**
- **Phiên bản / Version:** v1.2.0 - Security & Integrity Edition (Garena & Level Infinite / Steam)

---

## Tính Năng Nổi Bật / Key Features

### Tiếng Việt

1. **Quét vũ khí thời gian thực (Phím tắt `F9`):**
   - Tự động chụp vùng tên súng từ màn hình chính bằng thư viện `xcap`.
   - Nhận diện ký tự qua Windows Native Media OCR (`Windows.Media.Ocr.OcrEngine`).
   - Tự động sửa lỗi đọc ký tự bằng thuật toán so khớp mờ Jaro-Winkler (> 0.72).

2. **Tự động đẩy vào Windows Clipboard History (`Win + V`):**
   - Ưu tiên cấu hình Meta vào khay nhớ tạm hiện thời (`Ctrl + V`).
   - Lần lượt đẩy các cấu hình đề xuất còn lại vào lịch sử Clipboard (`Win + V`) cách nhau 150ms.

3. **Thanh Bộ Lọc 4 Cấp (Filter Bar):**
   - **Khu vực máy chủ (Server):** `Garena` và `Infinity (Global / Level Infinite / Steam)`.
   - **Chế độ chơi (Mode):** `Chiến Trường Toàn Diện (Warfare)`, `Chiến Dịch Sinh Tồn (Operations)`, hoặc `Tất cả chế độ`.
   - **Phân loại vũ khí (Weapon Type):** `Assault Rifle`, `SMG`, `LMG`, `Marksman Rifle`, `Sniper Rifle`, `Shotgun`, `Pistol`, `Special`.
   - **Vũ khí (Weapon):** Danh sách 70+ khẩu súng được lọc thông minh theo điều kiện đang chọn, tự động chuyển sang súng đầu tiên có dữ liệu khi đổi bộ lọc.

4. **Cơ sở dữ liệu lớn (2.200+ Cấu hình từ các nguồn cộng đồng):**
   - Thu thập từ các cổng dữ liệu cộng đồng trực tuyến với cơ chế lọc trùng lặp tuyệt đối theo `loadout_code`.
   - **Tuân thủ pháp lý & Sở hữu trí tuệ:** Tách biệt hoàn toàn thương hiệu bên thứ 3, chuẩn hóa sang các nhãn trung lập trên giao diện (`Nguồn Cộng Đồng #1`, `Nguồn Cộng Đồng #2`, `Nguồn Xếp Hạng Meta` / `Community A`, `Community B`, `Meta Rank`).
   - **Khóa kênh cập nhật an toàn:** Đường dẫn cập nhật mùa mới được khóa cứng vào máy chủ GitHub chính thức của tác giả (`TaiGiap/df-loadout-scanner`), ngăn chặn hoàn toàn nguy cơ giả mạo nguồn dữ liệu (Data Spoofing).

5. **Bảo mật & Chống can thiệp (Anti-Tamper & Security):**
   - **Kiểm tra tính toàn vẹn (Integrity Check):** Tính mã băm FNV-1a 64-bit định danh tác giả và ứng dụng ngay lúc khởi động và liên tục trong vòng lặp kết xuất (render loop). Mọi hành vi dùng Hex Editor sửa đổi chuỗi trong file nhị phân sẽ kích hoạt cơ chế tự dừng ứng dụng ngay lập tức.
   - **Phát hiện trình gỡ lỗi (Anti-Debugger):** Tích hợp kiểm tra API `IsDebuggerPresent()` của Windows nhằm ngăn chặn hành vi đính kèm trình gỡ lỗi trái phép.
   - **Tối ưu chống dịch ngược:** Thiết lập `[profile.release]` với `opt-level = 3`, `lto = "fat"`, `codegen-units = 1`, `panic = "abort"` và `strip = "symbols"`, xóa sạch bảng ký hiệu và dấu vết đường dẫn cá nhân.

6. **Giao diện Tactical Dark UI:**
   - Phong cách quân sự Delta Force với bảng màu `#0B1015`, `#131B24`, `#00E5A0`.
   - Hỗ trợ song ngữ qua nút `[LANG: VI]` / `[LANG: EN]`.
   - Bảng hướng dẫn sử dụng song ngữ tự động mở khi khởi động.
   - Chế độ luôn hiển thị trên cùng (Always on Top).
   - Kích thước chuẩn cố định 1140x680 tối ưu không gian hiển thị.

7. **An toàn 100% (Anti-Cheat Safe):**
   - Chỉ chụp ảnh màn hình qua Windows API và ghi vào Clipboard hệ thống.
   - Hoàn toàn không can thiệp bộ nhớ (RAM), không chèn DLL, không chỉnh sửa tệp hay tiến trình game.

---

### English

1. **Real-time Weapon Scanner (Global Hotkey `F9`):**
   - Captures the weapon title region using native Windows OCR and matches it via Jaro-Winkler fuzzy string distance (> 0.72).

2. **Auto-Clipboard History Sync (`Win + V`):**
   - Places the top Meta build on active clipboard (`Ctrl + V`), then feeds recommended builds into Windows Clipboard History (`Win + V`) spaced 150ms apart.

3. **4-Level Dynamic Filter Bar:**
   - Filter by Server (`Garena` / `Infinity`), Game Mode (`Warfare` / `Operations` / `All`), Weapon Type (8 categories), and Weapon (70+ weapons).
   - Automatically resets to the first available weapon when filters change, preventing empty result states.

4. **Large Multi-Source Database (2,200+ Builds):**
   - Aggregated from online community repositories with strict deduplication by `loadout_code`.
   - **Legal & Trademark Compliance:** Seamlessly maps third-party branding to neutral labels (`Community A`, `Community B`, `Meta Rank` / `Nguồn Cộng Đồng #1`, `Nguồn Cộng Đồng #2`, `Nguồn Xếp Hạng Meta`).
   - **Locked Official Update Pipeline:** Hardcoded to the author's verified GitHub repository, preventing malicious data spoofing attacks.

5. **Security & Anti-Tamper Hardening:**
   - **Continuous Integrity Verification:** 64-bit FNV-1a hash verification runs at startup and during every frame render loop. Any hex modification to author identity or app name terminates the process immediately.
   - **Anti-Debugger Guard:** Checks Windows `IsDebuggerPresent()` at launch to block live reverse engineering attempts.
   - **Aggressive Release Stripping:** Release profile compiled with `opt-level = 3`, `lto = "fat"`, `codegen-units = 1`, `panic = "abort"`, and `strip = "symbols"`.

6. **Tactical Military Dark UI:**
   - Delta Force emerald accent palette, bilingual `[LANG: VI / EN]` switcher, User Guide modal, Always on Top toggle, and fixed 1140x680 resolution.

7. **100% Anti-Cheat Compliant:**
   - Operates strictly through Windows display capture and clipboard APIs. Zero memory injection, zero game hook.

---

## Hướng Dẫn Sử Dụng / Quick Start

### 1. Cài đặt hệ thống / System Setup

- **Quyền Administrator (UAC):** File `.exe` tự động yêu cầu quyền Administrator khi mở. Quyền này bắt buộc để Windows cho phép lắng nghe phím tắt toàn cục `F9` khi cửa sổ game Delta Force đang chiếm tiêu điểm (hoàn toàn không can thiệp bộ nhớ hay tiến trình game).
- **Trong game Delta Force:** Chuyển chế độ hiển thị sang **Borderless Windowed (Không viền)**.
- **Trên Windows:** Nhấn `Win + V` và bật tính năng **Clipboard History (Lưu lịch sử Clipboard)**.

### 2. Sử dụng công cụ / Usage

1. Mở file `DF-Loadout-Scanner-Windows-x64.exe` (hoặc `df_clipboard_helper.exe`).
2. Chọn **Server** (`Garena` hoặc `Infinity`) và **Chế độ chơi** (`Warfare` hoặc `Operations`) phù hợp.
3. Trong game, vào giao diện tùy biến vũ khí và nhấn phím **`F9`**.
4. Ứng dụng phát tiếng bíp xác nhận. Nhấn **`Ctrl + V`** để dán ngay bản Meta, hoặc **`Win + V`** để chọn các build khác.

---

## Phân Phối & Bản Quyền Nhị Phân / Distribution

Ứng dụng được biên dịch và đóng gói sẵn dưới dạng Portable Executable độc lập (`.exe`) và kho lưu trữ nén (`.zip`) tại mục [GitHub Releases](https://github.com/TaiGiap/df-loadout-scanner/releases/latest).

Mã nguồn được quản lý và bảo vệ trong kho lưu trữ riêng biệt (Private Repository) nhằm đảm bảo an toàn, bảo lưu quyền sở hữu trí tuệ và chống các hành vi can thiệp trái phép. Dữ liệu cấu hình trang bị mùa mới được đồng bộ công khai tự động tại file [builds.json](builds.json).

---

## Bản Quyền & Giấy Phép / License & Proprietary Notice

Phần mềm được phát hành theo giấy phép **Proprietary Freeware License - All Rights Reserved** (Copyright © 2026 Anh Bảy Chọ / Tài Giáp).

- **Quyền sử dụng:** Người dùng được phép tải về và sử dụng miễn phí bản thực thi nguyên trạng cho mục đích cá nhân, phi thương mại.
- **Nghiêm cấm:** Mọi hành vi sao chép mã nguồn, dịch ngược, giải mã, can thiệp file nhị phân bằng Hex Editor, sửa đổi/xóa tên tác giả hoặc kinh doanh thương mại hóa dưới bất kỳ hình thức nào khi chưa có sự đồng ý bằng văn bản từ tác giả.

### Tuyên Bố Miễn Trừ Trách Nhiệm & Nhãn Hiệu (Disclaimer & Trademarks)
- Công cụ độc lập hoạt động hoàn toàn qua Windows OCR API chuẩn, không chứa bất kỳ tài sản trí tuệ, hình ảnh hay mã nguồn nào của trò chơi Delta Force (Team Jade / TiMi Studio Group / Garena).
- *An independent utility operating strictly via standard Windows OCR APIs, containing no intellectual property, assets, images, or source code from Delta Force (Team Jade / TiMi Studio Group / Garena).*
