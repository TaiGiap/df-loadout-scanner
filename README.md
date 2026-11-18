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
- **Phiên bản / Version:** v1.3.0 - Web Source Filter & Popularity Ranking Edition (Garena & Level Infinite / Steam)

---

## Tính Năng Nổi Bật / Key Features

### Tiếng Việt

1. **Quét vũ khí thời gian thực (Phím tắt `F9`) & Tự động chọn build hot nhất:**
   - Tự động chụp vùng tên súng từ màn hình chính bằng thư viện `xcap`.
   - Nhận diện ký tự qua Windows Native Media OCR (`Windows.Media.Ocr.OcrEngine`).
   - Tự động sửa lỗi đọc ký tự bằng thuật toán so khớp mờ Jaro-Winkler (> 0.72).
   - **Tự động chọn bản build tối ưu nhất:** Lọc theo Server, Mode và Nguồn web đang chọn, tự động nạp cấu hình có số lượt copy cao nhất vào Clipboard hiện thời (`Ctrl + V`).

2. **Tự động đẩy vào Windows Clipboard History (`Win + V`):**
   - Đưa cấu hình có lượt copy cao nhất / Meta lên đầu khay nhớ tạm (`Ctrl + V`).
   - Lần lượt đẩy các cấu hình còn lại vào lịch sử Clipboard (`Win + V`) cách nhau 150ms để tiện tra cứu.

3. **Thanh Bộ Lọc 5 Cấp (5-Level Dynamic Filter Bar):**
   - **Nguồn web (Source):** `Tất cả nguồn (ALL)`, `DFTools` (DeltaForceTools), `DFBuilds` (DeltaForceBuilds), và `FPSMeta`. Ghi nhớ nguồn đã chọn qua file `config.json`.
   - **Khu vực máy chủ (Server):** `Garena` và `Infinity (Global / Level Infinite / Steam)`.
   - **Chế độ chơi (Mode):** `Chiến Trường Toàn Diện (Warfare)`, `Chiến Dịch Sinh Tồn (Operations)`, hoặc `Tất cả chế độ`.
   - **Phân loại vũ khí (Weapon Type):** `Assault Rifle`, `SMG`, `LMG`, `Marksman Rifle`, `Sniper Rifle`, `Shotgun`, `Pistol`, `Special`.
   - **Vũ khí (Weapon):** Danh sách 75+ khẩu súng được lọc thông minh theo điều kiện đang chọn, tự động chuyển sang súng đầu tiên có dữ liệu khi đổi bộ lọc.

4. **Hiển thị số lượt copy & Sắp xếp ưu tiên độ phổ biến:**
   - Hiển thị huy hiệu số lượt sao chép thực tế (ví dụ: `📋 3,612 lượt copy`) trên từng thẻ build.
   - Thuật toán sắp xếp ưu tiên hiển thị các bản build có số lượt copy cao nhất lên đầu, kết hợp cùng mức độ ưu tiên Meta/đề xuất.

5. **Cơ sở dữ liệu lớn (2.200+ Cấu hình từ 3 nguồn cộng đồng):**
   - Thu thập và chuẩn hóa từ các cổng dữ liệu cộng đồng `DeltaForceTools`, `DeltaForceBuilds` và `FPSMeta` với cơ chế lọc trùng lặp tuyệt đối theo `loadout_code`.
   - Ghi nhận nguồn minh bạch trên giao diện từng bản build giúp người chơi dễ dàng theo dõi.
   - **Khóa kênh cập nhật an toàn:** Đường dẫn cập nhật mùa mới được khóa cứng vào máy chủ GitHub chính thức của tác giả (`TaiGiap/df-loadout-scanner`), ngăn chặn hoàn toàn nguy cơ giả mạo nguồn dữ liệu (Data Spoofing).

6. **Bảo mật & Chống can thiệp (Anti-Tamper & Security):**
   - **Kiểm tra tính toàn vẹn (Integrity Check):** Tính mã băm FNV-1a 64-bit định danh tác giả và ứng dụng ngay lúc khởi động và liên tục trong vòng lặp kết xuất (render loop). Mọi hành vi dùng Hex Editor sửa đổi chuỗi trong file nhị phân sẽ kích hoạt cơ chế tự dừng ứng dụng ngay lập tức.
   - **Phát hiện trình gỡ lỗi (Anti-Debugger):** Tích hợp kiểm tra API `IsDebuggerPresent()` của Windows nhằm ngăn chặn hành vi đính kèm trình gỡ lỗi trái phép.
   - **Tối ưu chống dịch ngược:** Thiết lập `[profile.release]` với `opt-level = 3`, `lto = "fat"`, `codegen-units = 1`, `panic = "abort"` và `strip = "symbols"`, xóa sạch bảng ký hiệu và dấu vết đường dẫn cá nhân.

7. **Giao diện Tactical Dark UI:**
   - Phong cách quân sự Delta Force với bảng màu `#0B1015`, `#131B24`, `#00E5A0`.
   - Hỗ trợ song ngữ qua nút `[LANG: VI]` / `[LANG: EN]`.
   - Bảng hướng dẫn sử dụng song ngữ tự động mở khi khởi động.
   - Chế độ luôn hiển thị trên cùng (Always on Top).
   - Kích thước chuẩn cố định 1140x680 tối ưu không gian hiển thị, căn chỉnh hoàn hảo các dropdown lọc.

8. **An toàn 100% (Anti-Cheat Safe):**
   - Chỉ chụp ảnh màn hình qua Windows API và ghi vào Clipboard hệ thống.
   - Hoàn toàn không can thiệp bộ nhớ (RAM), không chèn DLL, không chỉnh sửa tệp hay tiến trình game.

---

### English

1. **Real-time Weapon Scanner (Global Hotkey `F9`) & Smart Best Build Auto-Pick:**
   - Captures the weapon title region using native Windows OCR and matches it via Jaro-Winkler fuzzy string distance (> 0.72).
   - Automatically selects the build with the highest copy count matching current Server, Mode, and Web Source, feeding it into active clipboard (`Ctrl + V`).

2. **Auto-Clipboard History Sync (`Win + V`):**
   - Places the top/most popular build on active clipboard (`Ctrl + V`), then feeds recommended builds into Windows Clipboard History (`Win + V`) spaced 150ms apart.

3. **5-Level Dynamic Filter Bar:**
   - Filter by Web Source (`ALL`, `DFTools`, `DFBuilds`, `FPSMeta`), Server (`Garena` / `Infinity`), Game Mode (`Warfare` / `Operations` / `All`), Weapon Type (8 categories), and Weapon (75+ weapons).
   - Automatically resets to the first available weapon when filters change, preventing empty result states.
   - Persists selected source across sessions in `config.json`.

4. **Copy Count Badge & Popularity Ranking:**
   - Displays real-world copy count badges (e.g. `📋 3,612 copies`) on each card.
   - Prioritizes most copied loadouts at the top alongside Meta/Recommended status.

5. **Large Multi-Source Database (2,200+ Builds):**
   - Aggregated from online community repositories (`DeltaForceTools`, `DeltaForceBuilds`, `FPSMeta`) with strict deduplication by `loadout_code`.
   - Transparent attribution displayed for each loadout card.
   - **Locked Official Update Pipeline:** Hardcoded to the author's verified GitHub repository, preventing malicious data spoofing attacks.

6. **Security & Anti-Tamper Hardening:**
   - **Continuous Integrity Verification:** 64-bit FNV-1a hash verification runs at startup and during every frame render loop. Any hex modification to author identity or app name terminates the process immediately.
   - **Anti-Debugger Guard:** Checks Windows `IsDebuggerPresent()` at launch to block live reverse engineering attempts.
   - **Aggressive Release Stripping:** Release profile compiled with `opt-level = 3`, `lto = "fat"`, `codegen-units = 1`, `panic = "abort"`, and `strip = "symbols"`.

7. **Tactical Military Dark UI:**
   - Delta Force emerald accent palette, bilingual `[LANG: VI / EN]` switcher, User Guide modal, Always on Top toggle, and fixed 1140x680 resolution with compactly tuned dropdowns.

8. **100% Anti-Cheat Compliant:**
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

## Nguồn Dữ Liệu, Ghi Nhận & Chính Sách Gỡ Bỏ / Data Sources, Attribution & Takedown Policy

### 1. Nguồn Dữ Liệu Tham Khảo Cộng Đồng (Community Attribution)
Cơ sở dữ liệu cấu hình trang bị được tổng hợp nhằm mục đích tra cứu phi lợi nhuận cho người chơi từ 3 cổng cộng đồng trực tuyến:
- **DeltaForceTools:** [deltaforcetools.gg](https://deltaforcetools.gg/vi/weapon-builds)
- **DeltaForceBuilds:** [deltaforcebuilds.com](https://deltaforcebuilds.com)
- **FPSMeta:** [fpsmeta.com](https://fpsmeta.com) ([fpsmeta.gg](https://fpsmeta.gg/delta-force-meta))

Ứng dụng hoàn toàn độc lập, phi thương mại, không có mối liên kết hay tài trợ từ các chủ sở hữu trang web nêu trên cũng như nhà phát hành trò chơi Delta Force (Team Jade / TiMi Studio Group / Garena).

### 2. Quy Trình Tiếp Nhận Khiếu Nại & Gỡ Bỏ Dữ Liệu (Notice & Takedown Policy)
Tác giả tôn trọng quyền sở hữu trí tuệ và mong muốn của các quản trị viên/chủ sở hữu nguồn dữ liệu cộng đồng:
- Nếu quý đơn vị hoặc chủ sở hữu không muốn dữ liệu được trích dẫn hay tổng hợp trong công cụ này, vui lòng gửi yêu cầu gỡ bỏ (Takedown / Opt-out Request) thông qua:
  - **GitHub Issues:** [Tạo Issue Yêu Cầu Gỡ Bỏ](https://github.com/TaiGiap/df-loadout-scanner/issues)
  - **Email Liên Hệ Trực Tiếp:** [tai.gd361@gmail.com](mailto:tai.gd361@gmail.com)
- Mọi yêu cầu hợp lệ từ chủ sở hữu nguồn sẽ được xử lý, loại bỏ hoàn toàn khỏi bộ cào và cơ sở dữ liệu phân phối trong vòng **24–48 giờ**.

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
