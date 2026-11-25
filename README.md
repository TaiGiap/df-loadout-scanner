# DF Loadout Scanner

Công cụ **mã nguồn mở (Open Source — MIT License)** tự động nhận diện vũ khí thời gian thực qua Windows OCR và đồng bộ mã trang bị (loadout code) cho game Delta Force (Garena & Level Infinite / Steam).

*Open-source (MIT License) real-time weapon OCR scanner and loadout clipboard synchronizer for Delta Force.*

---

## Tải Về Nhanh / Quick Download

Người dùng thông thường chỉ cần tải bản đóng gói sẵn tại mục GitHub Releases:

- **Phiên bản mới nhất (Latest Release):** [DF-Loadout-Scanner Releases](https://github.com/TaiGiap/df-loadout-scanner/releases/latest)
- **Tệp cài đặt:** `DF-Loadout-Scanner-v1.4.6-Windows-x64.zip` (Đã đóng gói trọn gói file thực thi `.exe`, tài liệu hướng dẫn và giấy phép).

> **Tác giả / Developed By:** Anh Bảy Chọ / Tài Giáp  
> **Phiên bản / Version:** `v1.4.6` (Atomic DB Writes, Stable Monitor Identity, Semantic Rust Validator & Win+V Verification)

---

## Hướng Dẫn Cài Đặt & Sử Dụng (3 Bước)

### Bước 1: Tải về & Giải nén
- Tải gói file nén `DF-Loadout-Scanner-Windows-x64.zip` từ trang [Releases](https://github.com/TaiGiap/df-loadout-scanner/releases/latest).
- Giải nén tệp zip vào một thư mục bất kỳ trên máy tính.

### Bước 2: Khởi chạy ứng dụng
- Mở file `DF-Loadout-Scanner-Windows-x64.exe`.
- *Lưu ý:* Nếu Windows SmartScreen hiển thị hộp thoại màu xanh *"Windows protected your PC"*, hãy nhấn **More info** $\rightarrow$ chọn **Run anyway** để tiếp tục. Ứng dụng tự động yêu cầu quyền Administrator để bắt phím tắt toàn cục và chụp vùng tên súng ổn định khi đang mở game toàn màn hình.

### Bước 3: Thiết lập & Quét trong game
1. **Thiết lập hệ thống & game:**
   - Trong game Delta Force: Chuyển chế độ hiển thị sang **Borderless Windowed (Không viền)**.
   - Trên Windows: Nhấn tổ hợp phím `Win + V` và kích hoạt tính năng **Clipboard History (Lưu lịch sử Clipboard)**.
2. **Chọn bộ lọc:** Trên giao diện ứng dụng, chọn **Nguồn** (`Tất cả`, `DFTools`, `DFBuilds`, `FPSMeta`), **Server** (`Garena` hoặc `Infinity`), và **Chế độ chơi** (`Warfare` hoặc `Operations`).
3. **Tùy biến phím tắt (tùy chọn):** Nhấn nút `[⌨]` cạnh nút quét để đổi sang phím bạn muốn (`F1`–`F12`, `Mouse 4`, `Mouse 5`, `Numpad`...). Nhấn `Esc` để hủy.
4. **Quét súng:** Vào giao diện tùy biến vũ khí trong game, nhấn phím tắt (mặc định `F9` hoặc nút đã đổi).
   - Ứng dụng phát âm báo bíp xác nhận.
   - Nhấn `Ctrl + V` để dán ngay bản build có số lượt copy cao nhất vào ô nhập mã trong game.
   - Nhấn `Win + V` để chọn nhanh các cấu hình đề xuất khác đã được lưu sẵn trong lịch sử khay nhớ tạm.

---

## Tính Năng Nổi Bật / Key Features

1. **Quét vũ khí thời gian thực & Bộ xử lý OCR 3 tầng ưu tiên vùng sạch (Loại bỏ lệ thuộc vùng bị `18+` che):**
   - **Tầng 1 (Nguồn chính — Thẻ súng cột trái):** Tự động dò đồng thời 4 cạnh viền trắng sáng (trái/phải/trên/dưới) ở cột danh sách bên trái kể cả khi đang cuộn trang, phóng to 2.5x, lọc chữ trắng và cắt bỏ tên skin sau `" - "` / `" – "` (`M7 Battle Rifle - Đỏ` $\rightarrow$ `M7`). Đạt độ tin cậy `>= 90%` sẽ trả kết quả ngay lập tức.
   - **Tầng 2 (Dự phòng 1 — Đoạn mô tả vũ khí góc dưới cột giữa):** Nếu Tầng 1 chưa đạt `>= 90%`, tự động quét vùng mô tả dưới bảng phụ kiện (`y: 76.0%`–`86.5%`, sạch 100% khỏi bảng cảnh báo `18+`), phóng to 2.0x, trích xuất 4–5 từ đầu câu và lọc bỏ từ dẫn đầu chung chung (`SÚNG`, `THE`, `KHẨU`, `MỘT`).
   - **Tầng 3 (Dự phòng cuối — Tiêu đề cột giữa đã né `18+`):** Chỉ chạy khi cả 2 tầng trên chưa khớp, đồng thời dịch lề trái vùng chụp sang `26.8%` chiều rộng màn hình để né hoàn toàn hộp cảnh báo `18+`.
   - **Sửa 14 lỗi nhận diện súng:** Khắc phục triệt để các trường hợp đọc nhầm font game (`G3`, `P90`, `AR-57`, `PSG-1`, `M82`, `M7`, `MK4`, `R93`, `QBZ95-1`, `SG552`, `CI-19`, `SR-3M`, `VECTOR`, `725 Double-Barrel`).
   - Đổi phím tắt quét tức thì ngay trên thanh điều khiển với nút `[⌨]`, hỗ trợ đầy đủ `F1`–`F12`, phím điều hướng, phím số `Numpad`, phím đặc biệt và **hai nút chuột phụ bên hông** (`Mouse 4` / `Mouse 5`).

2. **Tối ưu giảm tải CPU/GPU khi cửa sổ chạy nền:**
   - Ứng dụng tự động phát hiện trạng thái cửa sổ (`focused` / `minimized`) để tạm dừng vẽ lại giao diện khi thu nhỏ. Ở chế độ chạy nền trong trận đấu, luồng lắng nghe phím tắt sử dụng vòng lặp kiểm tra siêu nhẹ mỗi `25ms` với mức tiêu thụ tài nguyên không đáng kể và chỉ kích hoạt chụp ảnh/OCR khi bạn bấm phím quét.

3. **Tự động kiểm tra bản cập nhật mới (Auto Update Checker):**
   - Tự động kiểm tra phiên bản mới từ GitHub Releases khi khởi động; hiển thị huy hiệu `[BẢN MỚI]` trên thanh Header giúp người dùng mở nhanh trang tải về chỉ với 1 cú click.

4. **Tự động chọn bản build hot nhất theo nguồn & lượt copy:**
   - Thuật toán thông minh tự động lọc cấu hình có số lượt sao chép (copy count) cao nhất phù hợp với Server, Chế độ và Trang nguồn bạn đang chọn để đưa ngay vào `Ctrl + V`.
   - Hiển thị huy hiệu số lượt sao chép thực tế (ví dụ: `📋 3,612 lượt copy`) trên từng thẻ trang bị.

5. **Lưu hàng đợi vào Windows Clipboard History (`Win + V`):**
   - Đưa cấu hình tối ưu nhất lên đầu khay nhớ tạm (`Ctrl + V`).
   - Lần lượt đẩy các cấu hình phụ/đề xuất khác vào lịch sử Clipboard (`Win + V`) cách nhau 150ms để tiện tra cứu và thay đổi nhanh trong trận đấu.

6. **Thanh lọc 5 cấp độ chuyên sâu:**
   - Lọc nhanh theo **Trang nguồn** (`ALL`, `DFTools`, `DFBuilds`, `FPSMeta`), **Server**, **Chế độ chơi**, **Hệ súng** (8 phân loại) và **75+ Vũ khí**. Tự động ghi nhớ các lựa chọn vào cấu hình máy.

7. **Cơ sở dữ liệu lớn (2.200+ Cấu hình) & Cập nhật tự động an toàn:**
   - Dữ liệu chuẩn hóa từ 3 cổng cộng đồng trực tuyến với cơ chế chống trùng lặp theo mã phụ kiện.
   - Đường dẫn cập nhật dữ liệu mùa mới được khóa cứng vào kênh máy chủ chính thức của tác giả, đảm bảo dữ liệu sạch và an toàn.

8. **Bảo vệ toàn vẹn & Bản quyền:**
   - Ứng dụng được tích hợp cơ chế bảo vệ toàn vẹn tệp thực thi và bản quyền tác giả.

9. **Tính tương thích & An toàn kỹ thuật (Anti-Cheat Compatibility):**
   - Được thiết kế để giảm thiểu tối đa can thiệp vào trò chơi: không đọc bộ nhớ (RAM), không tiêm tệp (DLL Injection), không chỉnh sửa tiến trình và không tự động hóa thao tác phím/chuột. Tuy nhiên, tính tương thích với các hệ thống chống gian lận (ACE) không được bảo đảm chính thức từ nhà phát hành.
   - Chỉ sử dụng Windows API chuẩn để chụp ảnh màn hình và ghi dữ liệu văn bản vào Clipboard.

10. **Ủng hộ tác giả (Support Me / VietQR Động):**
   - Nút `[❤ ỦNG HỘ]` trực tiếp trên thanh Header mở nhanh cửa sổ hỗ trợ tác giả.
   - Tích hợp mã VietQR động Techcombank tự điền đúng số tiền với 3 mốc nhanh (`1.000đ` - 4 xu delta, `5.000đ` - 20 xu delta, `10.000đ` - ly trà tắc) hoặc số tiền tùy ý.
   - Hỗ trợ sao chép số tài khoản (`1903 9673 0960 12` - GIÁP ĐỨC TÀI) và mở mã QR trên trình duyệt khi cần.

11. **Tab Chuyên Biệt Yêu Thích & Quản Lý Mã Cá Nhân (`favorites.json`):**
   - **Tab riêng biệt `[★ YÊU THÍCH]`:** Thay thế nút lọc toggle cũ bằng hệ thống 2 Tab rõ ràng trên Header: `[🔍 QUÉT & TRA CỨU]` và `[★ YÊU THÍCH (N)]` hiển thị số lượng cấu hình đã lưu.
   - **Thoát nhanh bằng phím `ESC`:** Khi đang ở Tab Yêu thích, nhấn phím `ESC` hoặc nút bấm nổi bật `[← QUAY LẠI (ESC)]` trên banner để lập tức quay lại màn hình quét chính.
   - **Quản lý tập trung toàn bộ súng:** Hiển thị danh sách tất cả các bản build đã ghim và mã cá nhân của toàn bộ súng trong game.
   - **Nút `[🔍 Xem súng]`:** Nhảy ngay đến khẩu súng tương ứng ở màn hình chính và chuyển về Tab `QUÉT & TRA CỨU`.
   - **Lưu trữ độc lập & an toàn:** Tách biệt hoàn toàn vào tệp `favorites.json`, không bao giờ bị ghi đè hay mất dữ liệu khi cập nhật DB (`[CẬP NHẬT DB]`).
   - **Ghim bản build yêu thích & mã riêng:** Nút ngôi sao `[☆ Lưu]` / `[★ Đã ghim]` đẩy cấu hình lên ưu tiên số 1 (Tier -1), modal `[+ THÊM MÃ RIÊNG]` và nút `[📋 COPY TẤT CẢ VÀO WIN+V]` ngay trong Tab.

12. **Cơ chế Ngưỡng Tin Cậy OCR (Confidence Threshold - Chống Copy Nhầm):**
   - **Độ tin cậy cao (`>= 90%`):** Tự động nhận diện và đồng bộ ngay mã tối ưu vào Clipboard (`Ctrl + V` và `Win + V`).
   - **Vùng nghi ngờ (`75% - 89%`):** Phát âm báo cảnh báo và hiển thị kết quả kèm nút `[✔ XÁC NHẬN & COPY]`. **Tuyệt đối không tự ý ghi đè Clipboard** cho đến khi người dùng chủ động nhấn xác nhận, ngăn chặn triệt để rủi ro dán nhầm mã súng khác trong trận đấu.
   - **Từ chối nhận diện (`< 75%`):** Phát âm báo lỗi và không nhận diện nếu hình ảnh OCR bị che khuất quá mức hoặc văn bản rác.

---

## English Summary

**DF Loadout Scanner** is a lightweight, real-time weapon recognition and loadout synchronizer for Delta Force (Garena & Level Infinite / Steam).

### Quick Start (3 Steps)
1. **Download:** Grab `DF-Loadout-Scanner-v1.4.6-Windows-x64.zip` from [GitHub Releases](https://github.com/TaiGiap/df-loadout-scanner/releases/latest) and extract it.
2. **Launch:** Run `DF-Loadout-Scanner-Windows-x64.exe`. If the Windows SmartScreen prompt appears, click **More info** $\rightarrow$ **Run anyway** (requires admin rights for global hotkey capture and stable fullscreen screenshot).
3. **Use in Game:** Set game display to **Borderless Windowed** and enable Windows **Clipboard History (`Win + V`)**. In gunsmith menu, press hotkey (default `F9` or customizable to `Mouse 4/5` / `F1-F12`). Hit `Ctrl + V` to paste the top build, or `Win + V` to browse alternate builds.

### Key Highlights
- **Atomic Verified Writes & Semantic Rust DB Validator:** Database updates verify `fs::rename`/`fs::copy` output size (> 0 bytes) before committing, preserving existing builds on disk/RAM on write failures, while cross-checking weapon names inside `loadout_code` and deduplicating records.
- **Stable Multi-Monitor Identity & Foreground Detection:** Automatically captures whichever display hosts the active game window (`GetForegroundWindow` + `MonitorFromWindow`), with stable `<DeviceName> (<W>x<H>)` monitor locking saved in `config.json` and safe auto-fallback if a display is unplugged.
- **Resilient Clipboard Init/Retry & Win+V Verification:** Retries both `Clipboard::new()` and `set_text()` up to 3 times (`35ms` interval), accurately reporting `success_count / total_count` pushed into `Win + V` and playing audio alerts strictly upon verified outcomes.
- **Short-Weapon Margin Check:** Evaluates Top-1 vs Top-2 candidate margins (`< 0.08`) and fuzzy matches on short weapons ($\le 3$ chars like `G3`, `M7`, `P90`, `MK4`, `R93`) into confirmation mode (`0.85`).
- **Clean-First 3-Tier OCR Pipeline (Immune to 18+ Warning Box):** Reversed scan priority eliminates reliance on the obstructed header region:
  - **Tier 1 (Primary — Left Weapon Card):** Scans left weapon list (`x: 7.0%–23.5%`, `y: 13.0%–90.0%`) detecting 4 bright white card edges simultaneously (top/bottom/left/right even when scrolled), upscales 2.5x with white-text isolation, strips skin suffixes (`" - "`, `" – "`), and returns immediately at `>= 90%` confidence.
  - **Tier 2 (Fallback 1 — Bottom-Center Description):** Crops weapon description text below the attachment slots (`x: 24.5%–39.5%`, `y: 76.0%–86.5%` — 100% clear of the 18+ notice), upscales 2.0x, and extracts the first 4–5 words while stripping leading stop words (`SÚNG`, `THE`, `KHẨU`, `MỘT`).
  - **Tier 3 (Final Fallback — Offset Center Header):** Only runs if Tiers 1 and 2 yield no match, starting crop at `x = 26.8%` to cleanly dodge the 18+ overlay box.
- **2-Row Clean Filter Layout & Smart Startup Weapon:** Reorganized the filter panel into two spacious rows (Row 1: Source, Server, Mode, Type; Row 2: 220px Search box with `[×]` clear button and 240px Weapon dropdown), preventing right-edge truncation across all DPI scales. Automatically prioritizes `M4A1` (or first weapon with available builds under default Garena server) on startup instead of `.357` (0 builds), instantly displaying rich loadouts upon launch.
- **Enhanced Filter Search & Vietnamese Aliases:** Real-time auto-selection and dropdown filtering support case-insensitive, dash-less (`"ar57"`, `"mini14"`), and Vietnamese aliases (`"hai nong"` -> `725 DOUBLE-BARREL`, `"cung"` -> `COMPOUND BOW`).
- **Dedicated Favorites Tab & ESC Quick Exit (`favorites.json`):** Dedicated `[★ FAVORITES (N)]` tab displaying all pinned builds and custom loadouts for all weapons with quick jump button (`[🔍 View Gun]`) to weapon page, fast exit via `ESC` key or `[← BACK (ESC)]` banner button, independent storage (`favorites.json`), and instant custom code modal (`[+ ADD CUSTOM]`).
- **OCR Confidence Thresholding (Anti-Miscopy Protection):** Tiered recognition prevents unwanted clipboard overwrites: high confidence (`>= 90%`) auto-syncs, suspicious matches (`75%-89%`) require explicit confirmation via `[✔ CONFIRM & COPY]`, and low confidence (`< 75%`) triggers rejection.
- **Semantic Data Validator & Clean Code Guarantee:** Deep cross-validation ensures every weapon code belongs 100% to the matching weapon, removing corrupted/mismatched codes and rogue newlines.
- **Source-Normalized Ranking:** Dynamically balances popularity scores across multiple sources, allowing high-quality builds from DFBuilds and FPSMeta to be fairly interleaved with DFTools without getting pushed to the bottom.
- **14 Specific Weapon OCR Fixes & Anti-Junk Filtering:** Resolves misreadings for `G3`, `P90`, `AR-57`, `PSG-1`, `M82`, `M7`, `MK4`, `R93`, `QBZ95-1`, `SG552`, `CI-19`, `SR-3M`, `VECTOR`, and `725 Double-Barrel`, with strict length difference checks protecting short weapon names.
- **Ultra-Lightweight Background Loop:** Minimizing the window pauses UI rendering, while hotkey monitoring relies on an ultra-lightweight `25ms` polling loop with minimal CPU footprint, triggering screen capture and OCR only on demand.
- **Auto Release Check:** Built-in update notifier surfaces a clickable badge in Header when a new release is available.
- **Custom Hotkey:** Dedicated `[⌨]` rebind button supporting `F1`–`F12`, `Numpad`, and side mouse buttons (`Mouse 4` / `Mouse 5`).
- **Smart Auto-Pick & Clipboard Sync:** Automatically feeds the loadout with highest copy count to `Ctrl + V`, populating alternate builds to `Win + V`.
- **Multi-Source Database:** Over 2,100 verified builds aggregated from `DFTools`, `DFBuilds`, and `FPSMeta`.
- **Technical Safety & Anti-Cheat Compatibility:** Designed to minimize game interference: no game memory access, DLL injection, process modification, or automated gameplay input. Compatibility with anti-cheat systems is not officially guaranteed.
- **Support / Dynamic VietQR:** Built-in modal supporting instant donation presets (1K, 5K, 10K VND or custom) via auto-generated Techcombank VietQR and one-click account copying.

---

## Tự Biên Dịch Từ Mã Nguồn / Build from Source

Dự án được viết bằng **Rust** (tối ưu cho Windows 10/11 64-bit). Cộng đồng và lập trình viên có thể tự do tải mã nguồn về chỉnh sửa, gỡ lỗi hoặc tự biên dịch tệp thực thi:

1. **Cài đặt công cụ:**
   - Cài đặt Rust & Cargo thông qua [rustup.rs](https://rustup.rs/) (khuyên dùng toolchain `stable-x86_64-pc-windows-msvc` hoặc `gnu`).
   - Đảm bảo gói ngôn ngữ **English (United States)** đã được cài trong Windows (*Settings → Time & Language → Language & region*) để Windows OCR API hoạt động.
2. **Clone mã nguồn & Chạy thử:**
   ```bash
   git clone https://github.com/TaiGiap/df-loadout-scanner.git
   cd df-loadout-scanner
   cargo test
   cargo run
   ```
3. **Biên dịch bản phát hành tối ưu (`Release`):**
   ```bash
   cargo build --release
   ```
   Tệp thực thi `.exe` sẽ được xuất ra tại `target/release/df_clipboard_helper.exe`.

---

## Đóng Góp / Contributing

Mọi đóng góp từ cộng đồng đều được hoan nghênh!
- **Báo lỗi & Đề xuất tính năng (Issues):** Mở [GitHub Issues](https://github.com/TaiGiap/df-loadout-scanner/issues) khi bạn phát hiện lỗi giao diện, súng mới chưa có trong danh mục hoặc lỗi nhận diện OCR (kèm ảnh chụp vùng tên súng càng tốt).
- **Đóng góp mã nguồn (Pull Requests):** Fork kho lưu trữ, tạo nhánh mới (`git checkout -b feat/ten-tinh-nang`), đảm bảo toàn bộ kiểm thử `cargo test` đều đạt PASS 100% trước khi gửi Pull Request.
- **Cập nhật dữ liệu vũ khí (`fetch_builds.py`):** Dữ liệu `builds.json` được tự động cập nhật định kỳ qua GitHub Actions từ 3 cổng cộng đồng.

---

## Nguồn Dữ Liệu, Ghi Nhận & Chính Sách Gỡ Bỏ / Data Attribution & Takedown

### 1. Nguồn Dữ Liệu Tham Khảo Cộng Đồng (Community Attribution)
Cơ sở dữ liệu cấu hình trang bị được tổng hợp miễn phí phục vụ cộng đồng người chơi tra cứu thuận tiện từ 3 cổng thông tin trực tuyến:
- **DeltaForceTools:** [deltaforcetools.gg](https://deltaforcetools.gg/vi/weapon-builds)
- **DeltaForceBuilds:** [deltaforcebuilds.com](https://deltaforcebuilds.com)
- **FPSMeta:** [fpsmeta.com](https://fpsmeta.com) ([fpsmeta.gg](https://fpsmeta.gg/delta-force-meta))

Phần mềm được phát hành **mã nguồn mở (Open Source — MIT License)** và miễn phí 100% cho cộng đồng — mọi khoản ủng hộ (trà tắc/cà phê) từ người dùng đều hoàn toàn tự nguyện và không khóa bất kỳ tính năng nào. Ứng dụng hoạt động hoàn toàn độc lập, không có mối liên kết hay tài trợ từ các trang web nêu trên cũng như nhà phát hành trò chơi Delta Force (Team Jade / TiMi Studio Group / Garena).

### 2. Tiếp Nhận Khiếu Nại & Gỡ Bỏ Dữ Liệu (Notice & Takedown Policy)
Tác giả tôn trọng quyền sở hữu trí tuệ và mong muốn của các đơn vị quản trị nguồn dữ liệu:
- Mọi yêu cầu gỡ bỏ nguồn hoặc dữ liệu trích dẫn (Opt-out / Takedown Request) xin vui lòng gửi qua:
  - **GitHub Issues:** [Tạo Issue Yêu Cầu Gỡ Bỏ](https://github.com/TaiGiap/df-loadout-scanner/issues)
  - **Email Liên Hệ Trực Tiếp:** [tai.gd361@gmail.com](mailto:tai.gd361@gmail.com)
- Yêu cầu hợp lệ sẽ được xử lý và gỡ bỏ hoàn toàn khỏi hệ thống phân phối trong vòng **24–48 giờ**.

---

## 📋 Nhật Ký Cập Nhật / Changelog

| Phiên bản | Điểm mới & Cải tiến chính |
| :--- | :--- |
| **v1.4.6 (Mới nhất)** | **[P0] Atomic Write Verification:** Bổ sung `atomic_write_verified` xác nhận file đích `> 0` bytes khi `fs::rename`/`fs::copy`, không bỏ qua lỗi ghi, bảo toàn DB cũ nếu ghi thất bại; **[P1] Định danh màn hình ổn định:** Chuyển sang `selected_monitor_id` (`<Tên thiết bị> (<W>x<H>)`) tự động fallback về cửa sổ game khi tháo màn hình phụ; **[P1] Gia cố Clipboard & Win+V:** Thêm `init_clipboard_with_retry` (3 lần, `35ms`) và báo cáo chính xác tỷ lệ nạp `success/total` vào `Win + V`; **[P1] Semantic Validator phía Rust:** Đối chiếu chéo tên súng trong `loadout_code` với trường `weapon` và khử trùng lặp theo `loadout_code` trước khi ghi `builds_local.json`; Bổ sung kiểm tra khớp phiên bản tự động trong GitHub Actions release. |
| **v1.4.5** | Hỗ trợ **Đa Màn Hình thông minh** (tự động nhận diện màn hình chứa cửa sổ game đang focus qua Windows API `GetForegroundWindow`/`MonitorFromWindow`, kèm tùy chọn khóa màn hình thủ công lưu trong `config.json`); Gia cố độ tin cậy **Clipboard** với cơ chế thử lại tối đa 3 lần (`35ms`) + tự khởi tạo lại instance và chỉ phát tiếng bíp `MB_OK` khi bản build đầu tiên ghi thành công (báo lỗi rõ ràng nếu thất bại); Nâng cấp `match_weapon` với **Top-1 vs Top-2 Margin Check (`< 0.08`)** cho nhóm súng tên ngắn ($\le 3$ ký tự) chuyển về vùng xác nhận thủ công (`0.85`); Bổ sung `validate_downloaded_builds` kiểm tra tính hợp lệ dữ liệu tải về phía Rust trước khi ghi `builds_local.json`. |
| **v1.4.4** | Đảo ngược thứ tự ưu tiên quét OCR sang mô hình 3 tầng sạch 100% để loại bỏ lệ thuộc vùng tiêu đề bị bảng `18+` che khuất: **(1) Nguồn chính** quét thẻ súng đang chọn ở cột danh sách bên trái (`x: 7.0%–23.5%`, `y: 13.0%–90.0%`) bằng dò đồng thời 4 cạnh viền trắng sáng, phóng to 2.5x, cắt tên skin sau `" - "` / `" – "`, trả kết quả ngay khi đạt `>= 90%`; **(2) Dự phòng 1** đọc đoạn mô tả vũ khí góc dưới cột giữa (`x: 24.5%–39.5%`, `y: 76.0%–86.5%`) phóng to 2.0x, trích xuất 4–5 từ đầu và lọc từ dẫn đầu (`SÚNG`, `THE`, `KHẨU`, `MỘT`); **(3) Dự phòng cuối** chỉ chạy khi cả 2 tầng trên chưa khớp với lề trái dịch sang `26.8%` né hoàn toàn hộp `18+`. |
| **v1.4.3** | Tách thanh `BỘ LỌC:` thành 2 hàng ngang rõ ràng (Dòng 1: Nguồn, Server, Chế độ, Loại; Dòng 2: Ô tìm kiếm súng ~220px kèm nút `[×]` và ComboBox Chọn vũ khí ~240px) chống cắt lẹm mép phải ở mọi tỉ lệ DPI; Tự động ưu tiên chọn `M4A1` (hoặc súng đầu tiên có sẵn build khớp với Server Garena mặc định) khi khởi động thay vì chọn `.357` (0 cấu hình) giúp người dùng thấy ngay danh sách build khi vừa mở app. |
| **v1.4.2** | Sửa triệt để lỗi giao diện chồng lấn trên Header: Chuyển ô `Tìm: [Tên súng...]` xuống thanh `BỘ LỌC:` (ngay trước ComboBox `Vũ khí:`) kèm nút nhỏ `[×]` xóa nhanh; Tối ưu hóa thuật toán tìm kiếm vũ khí không phân biệt hoa/thường, tự động bỏ qua dấu gạch ngang (`ar57`, `mini14`), hỗ trợ toàn diện bảng bí danh tiếng Việt (`hai nong` $\rightarrow$ `725 DOUBLE-BARREL`, `cung` $\rightarrow$ `COMPOUND BOW`...); Tự động chọn khẩu súng khớp nhất đầu tiên ngay khi gõ để tải cấu hình tức thì và lọc gọn danh sách trong dropdown `Vũ khí:`. |
| **v1.4.1** | Nâng cấp toàn diện trải nghiệm quản lý Yêu thích: Thay thế nút lọc toggle bằng hệ thống Tab chuyên biệt `[🔍 QUÉT & TRA CỨU]` và `[★ YÊU THÍCH (N)]` ngay trên Header; Quản lý tập trung toàn bộ cấu hình đã ghim và mã cá nhân của tất cả các súng; Bổ sung phím tắt `ESC` và nút `[← QUAY LẠI (ESC)]` thoát nhanh về màn hình quét; Thêm nút `[🔍 Xem súng]` nhảy thẳng đến trang tra cứu của súng; Tích hợp nút `[+ THÊM MÃ RIÊNG]` và `[COPY TẤT CẢ VÀO WIN+V]` trực tiếp trong Tab Yêu thích. |
| **v1.4.0** | Tích hợp hệ thống Build Yêu Thích và Mã Cá Nhân độc lập (`favorites.json`) không lo mất dữ liệu khi cập nhật DB; Thêm nút ghim `[☆ Lưu]` / `[★ Đã ghim]` đẩy cấu hình lên ưu tiên số 1 (Tier -1, badge `★ YÊU THÍCH`); Thêm modal `[+ THÊM MÃ RIÊNG]`; Bổ sung cơ chế phân tầng ngưỡng tin cậy OCR (Confidence Threshold) chống tự động ghi đè Clipboard khi kết quả ở vùng nghi ngờ 75%–89% và yêu cầu xác nhận thủ công `[✔ XÁC NHẬN & COPY]`. |
| **v1.3.7** | Nâng cấp bộ kiểm duyệt ngữ nghĩa (Semantic Data Validator) loại bỏ 100% bản ghi sai tên súng bên trong loadout code và mã rác; áp dụng thuật toán xếp hạng cân bằng đa nguồn (Source-Normalized Ranking) hiển thị đan xen công bằng các bản build giữa DFTools, DFBuilds và FPSMeta; chuẩn hóa tuyên bố tương thích Anti-Cheat chính xác và khách quan. |
| **v1.3.6** | Nâng cấp toàn diện độ nét OCR: Phóng to 2.5x kèm lọc ngưỡng tương phản cao (triệt tiêu nền mờ bảng 18+), cơ chế quét dự phòng thông minh (Fallback ROI) tự động nhận diện thẻ súng đang chọn ở cột bên trái và cắt bỏ tên skin, lọc triệt để từ rác bảng cảnh báo 18+ và sửa 14 lỗi nhận diện súng thực tế (G3, P90, AR-57, PSG-1, M82, M7, MK4, R93, QBZ95-1, SG552, CI-19, SR-3M, VECTOR, 725 Double-Barrel). |
| **v1.3.5** | Nâng cấp toàn diện bộ nhận diện vũ khí OCR: Tự động loại bỏ hậu tố phân loại súng (Tiếng Anh & Tiếng Việt), bổ sung bảng ánh xạ bí danh (`Shotgun Hai Nòng` $\rightarrow$ `725 Double-Barrel`, `Cung phức hợp` $\rightarrow$ `Compound Bow`...), siết chặt thuật toán so khớp 5 tầng (sửa triệt để lỗi nhận nhầm `G3`, `M249`, `PKM`, `M14`, `R93`) và hiển thị cảnh báo thông minh khi bộ lọc hiện tại chưa có cấu hình. |
| **v1.3.4** | Tích hợp cửa sổ `[❤ ỦNG HỘ / SUPPORT]` với mã VietQR Techcombank động (tự điền số tiền theo 3 mốc `1.000đ`, `5.000đ`, `10.000đ` hoặc tùy ý), sao chép nhanh STK và chuẩn hóa tuyên bố Freeware 100% cho cộng đồng. |
| **v1.3.3** | Bổ sung bộ kiểm duyệt dữ liệu `Data Validator` trong bộ cào lọc sạch mã build lỗi/rác, tự động đối chiếu sửa `mode` và ưu tiên thứ tự khớp chính xác tên súng OCR 100%. |
| **v1.3.2** | Tối ưu hóa hiệu năng tạm dừng render khi game fullscreen (giảm tải CPU/GPU chạy nền), tự động sửa lỗi OCR đọc nhầm ký tự tương đồng (`0/O`, `1/I/L`, `5/S`, `8/B`) và huy hiệu thông báo khi có bản cập nhật mới trên Header. |
| **v1.3.1** | Thêm nút `[⌨]` đổi phím tắt quét súng trực tiếp trên thanh Header (hỗ trợ toàn diện `F1–F12`, `Numpad`, các phím điều hướng và 2 phím chuột phụ `Mouse 4/5`). |
| **v1.3.0** | Hỗ trợ bộ lọc theo 4 nguồn dữ liệu (`ALL`, `DFTools`, `DFBuilds`, `FPSMeta`), hiển thị lượt copy thực tế trên thẻ trang bị và thuật toán tự động chọn bản build có lượt copy cao nhất vào `Ctrl + V`. |

---

## Bản Quyền & Giấy Phép / License & Disclaimer

Dự án được phát hành theo giấy phép **MIT License (Open Source)** — xem chi tiết tại tệp [LICENSE](LICENSE) (`Copyright (c) 2026 Giáp Đức Tài`).

- Người dùng và lập trình viên được tự do sử dụng, sao chép, sửa đổi, hợp nhất, xuất bản và phân phối lại mã nguồn cũng như bản biên dịch theo các điều khoản của giấy phép MIT.

### Tuyên Bố Miễn Trừ Trách Nhiệm (Disclaimer)
- Công cụ độc lập hoạt động hoàn toàn qua Windows OCR API chuẩn, không chứa bất kỳ tài sản trí tuệ, hình ảnh hay mã nguồn nào của trò chơi Delta Force (Team Jade / TiMi Studio Group / Garena).
- *An independent utility operating strictly via standard Windows OCR APIs, containing no intellectual property, assets, images, or source code from Delta Force (Team Jade / TiMi Studio Group / Garena).*
