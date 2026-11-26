# DF Loadout Scanner

Công cụ **mã nguồn mở (MIT License)** tự động nhận diện vũ khí thời gian thực bằng AI **PaddleOCR v4 (ONNX Runtime CPU ngoại tuyến)** và đồng bộ mã trang bị (*loadout code*) cho **Delta Force** (*Warfare* & *Operations* — Garena / Level Infinite / Steam).

- **Tác giả:** Anh Bảy Chọ / Tài Giáp
- **Phiên bản hiện tại:** `v2.0.11-Beta`
- **Kho mã nguồn:** [TaiGiap/df-loadout-scanner](https://github.com/TaiGiap/df-loadout-scanner)

---

## Tải Về & Sử Dụng Nhanh

### 1. Tải về & Giải nén
- Tải gói phát hành mới nhất tại [GitHub Releases](https://github.com/TaiGiap/df-loadout-scanner/releases/latest):
  `DF-Loadout-Scanner-v2.0.11-Beta-Windows-x64.zip` (kèm tệp kiểm tra toàn vẹn `SHA256SUMS.txt`).
- Giải nén **toàn bộ tệp trong gói** (`DF-Loadout-Scanner-Windows-x64.exe` và `onnxruntime.dll`) vào **cùng một thư mục**. Không cần cài thêm gói ngôn ngữ Windows OCR.

### 2. Thiết lập hệ thống & game
- Trong *Delta Force*, đặt chế độ hiển thị là **Borderless Windowed (Không viền)**.
- Trên Windows, bấm `Win + V` để bật **Clipboard History** (lưu hàng đợi nhiều mã trang bị).

### 3. Khởi chạy & Quét súng
- Chạy `DF-Loadout-Scanner-Windows-x64.exe` (nếu Windows SmartScreen hiện cảnh báo, chọn **More info** $\rightarrow$ **Run anyway**).
- Vào màn hình tùy biến vũ khí trong game và bấm phím tắt **`F9`** (hoặc phím đã đổi qua `[⌨]`):
  - **`Ctrl + V`**: dán bản build tối ưu nhất.
  - **`Win + V`**: chọn các bản build đề xuất khác trong hàng đợi.

---

## Tính Năng Chính

- **PaddleOCR v4 ONNX Ngoại Tuyến (`en_PP-OCRv4_rec.onnx` + `onnxruntime.dll`):**
  - Chạy hoàn toàn offline trên CPU (~15ms/lần quét), tự quy đổi khung HUD `16:9` trung tâm (`16:9`, `21:9` Ultrawide, `16:10`) tại `x: 24.0%–41.5%`, `y: 15.0%–19.6%`.
  - Tiền xử lý RGB không ngưỡng trắng kết hợp quy trình 2 tầng (**Pass 1A** $\rightarrow$ **Pass 1B** kéo tương phản + gamma `0.55` vùng `18+` $\rightarrow$ **Pass 2** quét góc trên trái thẻ đang chọn né tên skin) và **khóa cứng Loại súng** (`Shotgun Hai Nòng`, `Cung Phức Hợp`, `Lever-action`, `S12KU` $\rightarrow$ `S12K`).
- **Auto-Copy `>= 75%` Với 3 Rào Chắn An Toàn:**
  - Chỉ tự động nạp Clipboard khi đạt đồng thời: đã khóa loại súng (`category_locked`), chênh lệch Top-1/Top-2 (`>= 0.05` khi `score >= 0.80`, `>= 0.12` dưới `0.80`), mã `>= 2` ký tự và `ctc_confidence >= 0.55`. Thiếu điều kiện sẽ chuyển nút **`[✔ XÁC NHẬN & COPY]`**.
- **Bộ Lọc 5 Cấp Độ, Xem Trước Ảnh Gunsmith & Hàng Đợi `Win + V`:**
  - Lọc theo Nguồn (`ALL`, `DFTools`, `DFBuilds`, `FPSMeta`), Server, Chế độ, Loại súng và Tên súng (hỗ trợ tìm không dấu gạch ngang như `ar57`, `mini14` và biệt danh tiếng Việt trên ô tìm kiếm).
  - Di chuột vào icon con mắt cạnh nút lưu trên các thẻ build có `image_url` để xem trước ảnh cấu hình Gunsmith (tải bất đồng bộ, giải mã PNG/JPEG/WebP và lưu cache cục bộ tại `DF-Loadout-Scanner-Data/image_cache/`).
  - Ghim build yêu thích, thêm mã cá nhân (`favorites.json`), lưu lịch sử quét và nạp hàng đợi 5 mã vào `Win + V`.
- **Tự Động Cập Nhật (Cách B) Xác Minh SHA-256 & Rollback An Toàn:**
  - Kiểm tra phiên bản mới qua GitHub Releases API (hỗ trợ SemVer `-beta` / bản ổn định), tải `.zip` vào `DF-Loadout-Scanner-Data/downloads/`, kiểm tra cấu trúc `.zip` hợp lệ và **đối chiếu mã băm SHA-256** với `SHA256SUMS.txt` (tự hủy và xóa tệp nếu sai lệch hash).
  - Trước khi ghi đè, `apply_update.ps1` sao lưu bản cũ vào `DF-Loadout-Scanner-Data/updates/backup_<PID>` và **tự động rollback** nếu phát sinh lỗi, đồng thời bảo toàn toàn bộ dữ liệu cá nhân (`config.json`, `favorites.json`, `builds_local.json`, `df_history.json`). Ứng dụng chạy bằng quyền người dùng chuẩn (`asInvoker`).
- **Ghi Dữ Liệu Nguyên Tử (`atomic_write_verified`) & An Toàn Kỹ Thuật:**
  - Xác nhận dung lượng `> 0` bytes trước khi thay thế tệp dữ liệu. Không đọc RAM game, không DLL injection, không macro gameplay.

---

## Phím Tắt & Tùy Chọn Nâng Cao

| Mục | Mô tả |
| :--- | :--- |
| **`F9`** (mặc định) | Phím tắt quét vũ khí toàn cục. Đổi sang `F1`–`F12`, `Numpad`, `Mouse 4` / `Mouse 5` qua nút **`[⌨]`** (`Esc` để hủy). |
| **`ESC`** | Quay lại màn hình quét chính khi đang ở Tab Yêu thích hoặc đóng hộp thoại hiện tại. |
| **`DF_OCR_DEBUG=1`** | Biến môi trường (mặc định TẮT). Khi bật `DF_OCR_DEBUG=1`, xuất ảnh `pass1a_center.png`, `pass1b_boosted.png`, `pass2_leftcard.png` và `decision.log` vào `DF-Loadout-Scanner-Data/debug_ocr/`. |
| **`DF-Loadout-Scanner-Data/config.json`** | Lưu cấu hình bộ lọc, ngôn ngữ (`vi`/`en`), phím tắt, `selected_monitor_id` và `auto_check_update` (mặc định `true`). |

---

## Tự Biên Dịch Từ Mã Nguồn

Yêu cầu [Rust & Cargo](https://rustup.rs/) và đặt `onnxruntime.dll` (ONNX Runtime v1.31.x Windows x64 CPU) vào `target/debug/` hoặc cạnh tệp `.exe` (`target/release/df_clipboard_helper.exe`):

```powershell
git clone https://github.com/TaiGiap/df-loadout-scanner.git
cd df-loadout-scanner
cargo test
cargo build --release
```

---

## Lịch Sử Phiên Bản

| Phiên bản | Điểm mới & Cải tiến chính |
| :--- | :--- |
| **v2.0.11-Beta (Mới nhất)** | Mở rộng kích thước khung xem trước ảnh Gunsmith lớn hơn nữa (`820px–980px`, tỷ lệ `0.82` chiều rộng / `0.85` chiều cao cửa sổ) và nâng giới hạn cache ảnh lên `1600x1000` để giữ độ nét cao; vẫn luôn đặt lệch hẳn bên trái hoặc bên phải nút con mắt (`gap 14px`, lề `20px`) không gây mất hover. |
| **v2.0.10-Beta** | Sửa lỗi khung xem trước ảnh nằm chồng lên nút con mắt gây mất hover: luôn đặt khung xem trước nằm hẳn bên trái hoặc bên phải nút con mắt (cách tối thiểu `14px`), tự chọn phía rộng hơn và thu gọn vừa không gian khả dụng trên cửa sổ nhỏ mà vẫn giữ đúng tỷ lệ gốc của ảnh. |
| **v2.0.9-Beta** | Tăng đáng kể kích thước khung xem trước ảnh Gunsmith khi di chuột vào icon con mắt (`render_build_image_preview_popup` đạt chiều rộng `640px–780px` rõ nét, tự co giãn theo kích thước cửa sổ và giữ đúng tỷ lệ gốc của ảnh); nâng độ phân giải texture cache lên `1280x800` và tự động điều chỉnh vị trí popup chống tràn mép phải / mép dưới màn hình. |
| **v2.0.8-Beta** | Sửa lỗi thẻ `#1` bị nở khung bất thường (cố định chiều cao hàng tiêu đề `26px`, đồng nhất kích thước nút `24px` và cắt gọn vùng nhãn trái); giải mã trực tiếp ảnh súng từ `deltaforcetools.gg` bằng cách tách URL gốc khỏi proxy `/_next/image` (trả về PNG/JPG/WebP gốc thay vì bị Next.js ép sang AVIF) và lọc bỏ tệp `.avif` thuần không hỗ trợ trên Windows CPU. |
| **v2.0.7-Beta** | Lọc chặt `image_url` loại bỏ ảnh avatar/user/profile/member/icon/logo và ảnh đại diện nhỏ vuông trong `fetch_builds.py` & `BuildItem::valid_image_url`; tinh gọn lề/khoảng cách thẻ build và căn chỉnh gọn cụm nút tiêu đề trên màn hình. |
| **v2.0.6-Beta** | Sửa triệt để lỗi tràn nút bên phải trên thẻ build có tiêu đề/mô tả dài (cố định cụm nút trước, tự động cắt gọn mô tả kèm tooltip); luôn hiển thị icon con mắt đơn sắc trắng/xám cạnh nút lưu để phân biệt rõ build có/chưa có ảnh (`Build này chưa có hình ảnh đính kèm`). |
| **v2.0.5-Beta** | Hỗ trợ trường `image_url` trong `BuildItem` & `fetch_builds.py` (`deltaforcetools.gg`, `deltaforcebuilds.com`); bổ sung icon con mắt đơn sắc cạnh nút lưu hiển thị khung xem trước ảnh Gunsmith khi lướt chuột kèm tải bất đồng bộ PNG/JPEG/WebP và cache cục bộ tại `DF-Loadout-Scanner-Data/image_cache/`. |
| **v2.0.4-Beta** | Tự động tạo & đính kèm `SHA256SUMS.txt` trên GitHub Releases; kiểm tra đối chiếu SHA-256 trước khi cập nhật Cách B (hủy & xóa gói nếu sai hash); bổ sung cơ chế Backup & Rollback tự động trong `apply_update.ps1` và chuyển manifest Windows về quyền người dùng chuẩn `asInvoker`. |
| **v2.0.3-Beta** | Tăng độ tương phản & độ sáng chữ, viền nút trên nền tối (`[HƯỚNG DẪN]`, `[LANG]`, `[GHIM]`, `[ABOUT]`, `[KIỂM TRA CẬP NHẬT]`, `[CHỜ DB]`); làm gọn `README.md` chuẩn tài liệu kỹ thuật. |
| **v2.0.2-beta** | Ngoại lệ `Shotgun Hai Nòng` $\rightarrow$ `725 DOUBLE-BARREL`; xử lý tiền tố loại súng & ký tự rác đuôi (`Shotgun S12KU` $\rightarrow$ `S12K`); tinh chỉnh rào chắn Auto-Copy (`margin >= 0.05` khi `score >= 0.80`); bổ sung cơ chế tự cập nhật Cách B (`apply_update.ps1`, xác thực `.zip`, tự khởi động lại và bảo toàn dữ liệu người dùng) cùng bố cục Header 2 hàng. |
| **v2.0.1-beta** | Tiền xử lý RGB không ngưỡng trắng + khung HUD `16:9` đa tỷ lệ; quy trình 2 tầng Pass 1A / Pass 1B / Pass 2; khóa cứng Loại súng; Auto-Copy `>= 75%` với 3 rào chắn; sửa tràn Status Bar và thêm `DF_OCR_DEBUG=1`. |
| **v2.0.0-beta** | Thay Windows OCR bằng lõi **PaddleOCR v4 Offline (`en_PP-OCRv4_rec.onnx` + `onnxruntime.dll`)**; khóa vùng quét tiêu đề giữa và khớp cặp `[Hậu tố mã súng + Loại súng]`. |
| **v1.4.6** | `atomic_write_verified`; định danh màn hình ổn định (`selected_monitor_id`); gia cố Clipboard & `Win + V`; kiểm duyệt ngữ nghĩa DB phía Rust. |
| **v1.4.5** | Tự động nhận diện màn hình đang chạy game (`GetForegroundWindow` / `MonitorFromWindow`); Margin Check súng tên ngắn. |
| **v1.4.4** | Mô hình quét 3 tầng né bảng cảnh báo `18+`. |
| **v1.4.3** | Chia thanh bộ lọc thành 2 hàng; tự động ưu tiên chọn `M4A1` khi khởi động. |
| **v1.4.2** | Sửa lỗi đè ô tìm kiếm trên Header; hỗ trợ tìm kiếm không dấu gạch ngang và biệt danh tiếng Việt trên UI. |
| **v1.4.1** | Tách Tab Yêu thích chuyên biệt và thoát nhanh bằng `ESC`. |
| **v1.4.0** | Lưu trữ cấu hình yêu thích & mã cá nhân độc lập (`favorites.json`) cùng nút `[✔ XÁC NHẬN & COPY]`. |

---

## Nguồn Dữ Liệu, Bản Quyền & Miễn Trừ Trách Nhiệm

- **Nguồn dữ liệu cộng đồng:** Tổng hợp từ [DeltaForceTools](https://deltaforcetools.gg/vi/weapon-builds), [DeltaForceBuilds](https://deltaforcebuilds.com) và [FPSMeta](https://fpsmeta.com) ([fpsmeta.gg](https://fpsmeta.gg/delta-force-meta)). Mọi yêu cầu gỡ bỏ dữ liệu xin gửi qua [GitHub Issues](https://github.com/TaiGiap/df-loadout-scanner/issues) hoặc [tai.gd361@gmail.com](mailto:tai.gd361@gmail.com).
- **Giấy phép:** Phát hành theo **MIT License** — xem [LICENSE](LICENSE) (`Copyright (c) 2026 Giáp Đức Tài`).
- **Miễn trừ trách nhiệm:** Công cụ độc lập hoạt động qua chụp ảnh màn hình, PaddleOCR v4 offline và Clipboard hệ thống; không chứa mã nguồn hay tài sản trí tuệ của *Delta Force* (Team Jade / TiMi Studio Group / Garena).
