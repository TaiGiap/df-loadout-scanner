# DF Loadout Scanner

Công cụ tự động nhận diện vũ khí thời gian thực qua Windows OCR và đồng bộ mã trang bị (loadout code) cho game Delta Force (Garena & Level Infinite / Steam).

*Real-time weapon OCR scanner and loadout clipboard synchronizer for Delta Force.*

---

## Tải Về Nhanh / Quick Download

Người dùng thông thường chỉ cần tải bản đóng gói sẵn tại mục GitHub Releases:

- **Phiên bản mới nhất (Latest Release):** [DF-Loadout-Scanner Releases](https://github.com/TaiGiap/df-loadout-scanner/releases/latest)
- **Tệp cài đặt:** `DF-Loadout-Scanner-v1.4.2-Windows-x64.zip` (Đã đóng gói trọn gói file thực thi `.exe`, tài liệu hướng dẫn và giấy phép).

> **Tác giả / Developed By:** Anh Bảy Chọ / Tài Giáp  
> **Phiên bản / Version:** `v1.4.2` (Header Layout Fix, Enhanced Search & Vietnamese Aliases)

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

1. **Quét vũ khí thời gian thực & Bộ xử lý ảnh OCR nâng cao (Anti-18+ Banner & Fallback ROI):**
   - **Tăng cường độ nét ảnh 2.5x & Lọc tương phản:** Phóng to ảnh 2.5x kết hợp thuật toán phân tách chữ trắng (giữ lại pixel sáng chữ tên súng, triệt tiêu hoàn toàn nền xám mờ của bảng cảnh báo 18+ về màu đen).
   - **Cơ chế quét dự phòng (Fallback ROI):** Nếu vùng giữa bị che khuất hoặc không nhận diện được, ứng dụng tự động dò tìm vị trí thẻ súng đang được chọn ở cột danh sách bên trái (dựa trên viền trắng nổi bật), cắt lấy dòng tiêu đề và tự động loại bỏ tên skin để nhận diện chính xác 100%.
   - **Bộ lọc từ rác bảng cảnh báo 18+:** Loại bỏ triệt để các cụm từ như *"Chơi quá 180 phút..."* và các ký tự rác nhỏ lẻ.
   - **Sửa 14 lỗi nhận diện súng:** Khắc phục triệt để các trường hợp đọc nhầm font game (`G3`, `P90`, `AR-57`, `PSG-1`, `M82`, `M7`, `MK4`, `R93`, `QBZ95-1`, `SG552`, `CI-19`, `SR-3M`, `VECTOR`, `725 Double-Barrel`).
   - Đổi phím tắt quét tức thì ngay trên thanh điều khiển với nút `[⌨]`, hỗ trợ đầy đủ `F1`–`F12`, phím điều hướng, phím số `Numpad`, phím đặc biệt và **hai nút chuột phụ bên hông** (`Mouse 4` / `Mouse 5`).

2. **Tối ưu giảm tải CPU/GPU khi cửa sổ chạy nền:**
   - Ứng dụng tự động phát hiện trạng thái cửa sổ (`focused` / `minimized`). Khi người dùng đang trong trận đấu, toàn bộ việc vẽ lại giao diện được tạm dừng, giảm đáng kể tải CPU/GPU và chỉ kích hoạt vẽ khi có thao tác quét súng hoặc mở lại cửa sổ.

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
1. **Download:** Grab `DF-Loadout-Scanner-v1.4.2-Windows-x64.zip` from [GitHub Releases](https://github.com/TaiGiap/df-loadout-scanner/releases/latest) and extract it.
2. **Launch:** Run `DF-Loadout-Scanner-Windows-x64.exe`. If the Windows SmartScreen prompt appears, click **More info** $\rightarrow$ **Run anyway** (requires admin rights for global hotkey capture and stable fullscreen screenshot).
3. **Use in Game:** Set game display to **Borderless Windowed** and enable Windows **Clipboard History (`Win + V`)**. In gunsmith menu, press hotkey (default `F9` or customizable to `Mouse 4/5` / `F1-F12`). Hit `Ctrl + V` to paste the top build, or `Win + V` to browse alternate builds.

### Key Highlights
- **Zero-Overlap Header & Enhanced Filter Search:** Relocated search bar to the filter row directly preceding the Weapon dropdown with quick-clear `[×]` button, completely resolving Header button overlap. Real-time auto-selection and dropdown filtering support case-insensitive, dash-less (`"ar57"`, `"mini14"`), and Vietnamese aliases (`"hai nong"` -> `725 DOUBLE-BARREL`, `"cung"` -> `COMPOUND BOW`).
- **Dedicated Favorites Tab & ESC Quick Exit (`favorites.json`):** Dedicated `[★ FAVORITES (N)]` tab displaying all pinned builds and custom loadouts for all weapons with quick jump button (`[🔍 View Gun]`) to weapon page, fast exit via `ESC` key or `[← BACK (ESC)]` banner button, independent storage (`favorites.json`), and instant custom code modal (`[+ ADD CUSTOM]`).
- **OCR Confidence Thresholding (Anti-Miscopy Protection):** Tiered recognition prevents unwanted clipboard overwrites: high confidence (`>= 90%`) auto-syncs, suspicious matches (`75%-89%`) require explicit confirmation via `[✔ CONFIRM & COPY]`, and low confidence (`< 75%`) triggers rejection.
- **Semantic Data Validator & Clean Code Guarantee:** Deep cross-validation ensures every weapon code belongs 100% to the matching weapon, removing corrupted/mismatched codes and rogue newlines.
- **Source-Normalized Ranking:** Dynamically balances popularity scores across multiple sources, allowing high-quality builds from DFBuilds and FPSMeta to be fairly interleaved with DFTools without getting pushed to the bottom.
- **2.5x Upscaling & White-Text Contrast Thresholding:** Eliminates blurry text and nullifies the Vietnamese 18+ health warning gray overlay box by isolating pure white text before OCR processing.
- **Smart Left-Column Fallback ROI:** Automatically detects the highlighted weapon card on the left list when the center screen region is obstructed or unreadable, strips custom skin names, and extracts the clean weapon name.
- **14 Specific Weapon OCR Fixes & Anti-Junk Filtering:** Resolves misreadings for `G3`, `P90`, `AR-57`, `PSG-1`, `M82`, `M7`, `MK4`, `R93`, `QBZ95-1`, `SG552`, `CI-19`, `SR-3M`, `VECTOR`, and `725 Double-Barrel`, with strict length difference checks protecting short weapon names.
- **Zero Background Overhead:** Window focus/minimize detection ensures zero CPU/GPU consumption while in game, waking up only upon scan trigger.
- **Auto Release Check:** Built-in update notifier surfaces a clickable badge in Header when a new release is available.
- **Custom Hotkey:** Dedicated `[⌨]` rebind button supporting `F1`–`F12`, `Numpad`, and side mouse buttons (`Mouse 4` / `Mouse 5`).
- **Smart Auto-Pick & Clipboard Sync:** Automatically feeds the loadout with highest copy count to `Ctrl + V`, populating alternate builds to `Win + V`.
- **Multi-Source Database:** Over 2,100 verified builds aggregated from `DFTools`, `DFBuilds`, and `FPSMeta`.
- **Technical Safety & Anti-Cheat Compatibility:** Designed to minimize game interference: no game memory access, DLL injection, process modification, or automated gameplay input. Compatibility with anti-cheat systems is not officially guaranteed.
- **Support / Dynamic VietQR:** Built-in modal supporting instant donation presets (1K, 5K, 10K VND or custom) via auto-generated Techcombank VietQR and one-click account copying.

---

## Nguồn Dữ Liệu, Ghi Nhận & Chính Sách Gỡ Bỏ / Data Attribution & Takedown

### 1. Nguồn Dữ Liệu Tham Khảo Cộng Đồng (Community Attribution)
Cơ sở dữ liệu cấu hình trang bị được tổng hợp miễn phí phục vụ cộng đồng người chơi tra cứu thuận tiện từ 3 cổng thông tin trực tuyến:
- **DeltaForceTools:** [deltaforcetools.gg](https://deltaforcetools.gg/vi/weapon-builds)
- **DeltaForceBuilds:** [deltaforcebuilds.com](https://deltaforcebuilds.com)
- **FPSMeta:** [fpsmeta.com](https://fpsmeta.com) ([fpsmeta.gg](https://fpsmeta.gg/delta-force-meta))

Phần mềm được phát hành miễn phí 100% cho cộng đồng (Freeware) — mọi khoản ủng hộ (trà tắc/cà phê) từ người dùng đều hoàn toàn tự nguyện và không khóa bất kỳ tính năng nào. Ứng dụng hoạt động hoàn toàn độc lập, không có mối liên kết hay tài trợ từ các trang web nêu trên cũng như nhà phát hành trò chơi Delta Force (Team Jade / TiMi Studio Group / Garena).

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
| **v1.4.2 (Mới nhất)** | Sửa triệt để lỗi giao diện chồng lấn trên Header: Chuyển ô `Tìm: [Tên súng...]` xuống thanh `BỘ LỌC:` (ngay trước ComboBox `Vũ khí:`) kèm nút nhỏ `[×]` xóa nhanh; Tối ưu hóa thuật toán tìm kiếm vũ khí không phân biệt hoa/thường, tự động bỏ qua dấu gạch ngang (`ar57`, `mini14`), hỗ trợ toàn diện bảng bí danh tiếng Việt (`hai nong` $\rightarrow$ `725 DOUBLE-BARREL`, `cung` $\rightarrow$ `COMPOUND BOW`...); Tự động chọn khẩu súng khớp nhất đầu tiên ngay khi gõ để tải cấu hình tức thì và lọc gọn danh sách trong dropdown `Vũ khí:`. |
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

Phần mềm được phát hành theo giấy phép **Proprietary Freeware License - All Rights Reserved** (Copyright © 2026 Anh Bảy Chọ / Tài Giáp).

- **Quyền sử dụng:** Người dùng được phép tải về và sử dụng miễn phí bản thực thi nguyên trạng cho mục đích cá nhân, phi thương mại.
- **Nghiêm cấm:** Mọi hành vi sao chép mã nguồn, dịch ngược, giải mã, can thiệp file thực thi, sửa đổi/xóa tên tác giả hoặc thương mại hóa dưới bất kỳ hình thức nào khi chưa có sự đồng ý bằng văn bản từ tác giả.

### Tuyên Bố Miễn Trừ Trách Nhiệm (Disclaimer)
- Công cụ độc lập hoạt động hoàn toàn qua Windows OCR API chuẩn, không chứa bất kỳ tài sản trí tuệ, hình ảnh hay mã nguồn nào của trò chơi Delta Force (Team Jade / TiMi Studio Group / Garena).
- *An independent utility operating strictly via standard Windows OCR APIs, containing no intellectual property, assets, images, or source code from Delta Force (Team Jade / TiMi Studio Group / Garena).*
