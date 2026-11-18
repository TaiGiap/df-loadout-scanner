# DF Loadout Scanner

Công cụ tự động nhận diện vũ khí thời gian thực qua Windows OCR và đồng bộ mã trang bị (loadout code) cho game Delta Force (Garena & Level Infinite / Steam).

*Real-time weapon OCR scanner and loadout clipboard synchronizer for Delta Force.*

---

## Tải Về Nhanh / Quick Download

Người dùng thông thường chỉ cần tải bản đóng gói sẵn tại mục GitHub Releases:

- **Phiên bản mới nhất (Latest Release):** [DF-Loadout-Scanner Releases](https://github.com/TaiGiap/df-loadout-scanner/releases/latest)
- **Tệp cài đặt:** `DF-Loadout-Scanner-Windows-x64.zip` (Đã đóng gói trọn gói file thực thi `.exe`, tài liệu hướng dẫn và giấy phép).

> **Tác giả / Developed By:** Anh Bảy Chọ / Tài Giáp  
> **Phiên bản / Version:** `v1.3.2` (Performance & OCR Accuracy Edition)

---

## Hướng Dẫn Cài Đặt & Sử Dụng (3 Bước)

### Bước 1: Tải về & Giải nén
- Tải gói file nén `DF-Loadout-Scanner-Windows-x64.zip` từ trang [Releases](https://github.com/TaiGiap/df-loadout-scanner/releases/latest).
- Giải nén tệp zip vào một thư mục bất kỳ trên máy tính.

### Bước 2: Khởi chạy ứng dụng
- Mở file `DF-Loadout-Scanner-Windows-x64.exe`.
- *Lưu ý:* Nếu Windows SmartScreen hiển thị hộp thoại màu xanh *"Windows protected your PC"*, hãy nhấn **More info** $\rightarrow$ chọn **Run anyway** để tiếp tục. Ứng dụng tự động yêu cầu quyền Administrator để Windows cho phép bắt phím tắt toàn cục khi cửa sổ game đang hoạt động.

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

1. **Quét vũ khí thời gian thực & Bộ lọc sửa lỗi OCR thông minh:**
   - Tự động nhận diện tên vũ khí chính xác qua Windows Native OCR (`Windows.Media.Ocr`) kết hợp thuật toán so khớp mờ Jaro-Winkler.
   - **Bộ lọc sửa lỗi đọc nhầm OCR:** Tự động chuẩn hóa các cặp ký tự dễ nhầm lẫn trên font game (`0` <-> `O`, `1` <-> `I`/`L`, `5` <-> `S`, `8` <-> `B`, loại bỏ khoảng trắng và dấu gạch nối thừa) giúp độ chính xác đạt mức tối đa.
   - Đổi phím tắt quét tức thì ngay trên thanh điều khiển với nút `[⌨]`, hỗ trợ đầy đủ `F1`–`F12`, phím điều hướng, phím số `Numpad`, phím đặc biệt và **hai nút chuột phụ bên hông** (`Mouse 4` / `Mouse 5`).

2. **Tối ưu hóa hiệu năng nền (Zero Background Overhead):**
   - Ứng dụng tự động phát hiện trạng thái cửa sổ (`focused` / `minimized`). Khi người dùng đang trong trận đấu, toàn bộ việc vẽ lại giao diện được tạm dừng, giảm tải CPU/GPU về mức 0% và chỉ kích hoạt vẽ khi có thao tác quét súng hoặc mở lại cửa sổ.

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

9. **An toàn 100% (Anti-Cheat Safe):**
   - Chỉ sử dụng Windows API chuẩn để chụp ảnh màn hình và ghi dữ liệu văn bản vào Clipboard.
   - Hoàn toàn không can thiệp bộ nhớ (RAM), không inject DLL, không chỉnh sửa file hay tiến trình game.

---

## English Summary

**DF Loadout Scanner** is a lightweight, real-time weapon recognition and loadout synchronizer for Delta Force (Garena & Level Infinite / Steam).

### Quick Start (3 Steps)
1. **Download:** Grab the latest `DF-Loadout-Scanner-Windows-x64.zip` from [GitHub Releases](https://github.com/TaiGiap/df-loadout-scanner/releases/latest) and extract it.
2. **Launch:** Run `DF-Loadout-Scanner-Windows-x64.exe`. If the Windows SmartScreen blue prompt appears, click **More info** $\rightarrow$ **Run anyway** (requires admin rights for global hotkey capture).
3. **Use in Game:** Set game display to **Borderless Windowed** and enable Windows **Clipboard History (`Win + V`)**. In game gunsmith menu, press hotkey (default `F9` or customizable to `Mouse 4/5` / `F1-F12`). Hit `Ctrl + V` to paste the most popular build directly into the game, or `Win + V` to browse alternate builds.

### Key Highlights
- **Real-time OCR with Homoglyph Correction:** Captures weapon title with native Windows OCR and fuzzy matching, automatically correcting confused character pairs (`0`/`O`, `1`/`I`/`L`, `5`/`S`, `8`/`B`).
- **Zero Background Overhead:** Window focus/minimize detection ensures zero CPU/GPU consumption while in game, waking up only upon scan trigger.
- **Auto Release Check:** Built-in update notifier surfaces a clickable badge in Header when a new release is available.
- **Custom Hotkey:** Dedicated `[⌨]` rebind button supporting `F1`–`F12`, `Numpad`, and side mouse buttons (`Mouse 4` / `Mouse 5`).
- **Smart Auto-Pick & Clipboard Sync:** Automatically feeds the loadout with highest copy count to `Ctrl + V`, populating alternate builds to `Win + V`.
- **Multi-Source Database:** Over 2,200 builds aggregated from `DFTools`, `DFBuilds`, and `FPSMeta`.
- **100% Anti-Cheat Compliant:** Pure Windows display/clipboard APIs — zero memory scanning, zero game injection.

---

## Nguồn Dữ Liệu, Ghi Nhận & Chính Sách Gỡ Bỏ / Data Attribution & Takedown

### 1. Nguồn Dữ Liệu Tham Khảo Cộng Đồng (Community Attribution)
Cơ sở dữ liệu cấu hình trang bị được tổng hợp nhằm mục đích tra cứu phi lợi nhuận cho cộng đồng người chơi từ 3 cổng thông tin trực tuyến:
- **DeltaForceTools:** [deltaforcetools.gg](https://deltaforcetools.gg/vi/weapon-builds)
- **DeltaForceBuilds:** [deltaforcebuilds.com](https://deltaforcebuilds.com)
- **FPSMeta:** [fpsmeta.com](https://fpsmeta.com) ([fpsmeta.gg](https://fpsmeta.gg/delta-force-meta))

Ứng dụng hoàn toàn độc lập, phi lợi nhuận, không có mối liên kết hay tài trợ từ các trang web nêu trên cũng như nhà phát hành trò chơi Delta Force (Team Jade / TiMi Studio Group / Garena).

### 2. Tiếp Nhận Khiếu Nại & Gỡ Bỏ Dữ Liệu (Notice & Takedown Policy)
Tác giả tôn trọng quyền sở hữu trí tuệ và mong muốn của các đơn vị quản trị nguồn dữ liệu:
- Mọi yêu cầu gỡ bỏ nguồn hoặc dữ liệu trích dẫn (Opt-out / Takedown Request) xin vui lòng gửi qua:
  - **GitHub Issues:** [Tạo Issue Yêu Cầu Gỡ Bỏ](https://github.com/TaiGiap/df-loadout-scanner/issues)
  - **Email Liên Hệ Trực Tiếp:** [tai.gd361@gmail.com](mailto:tai.gd361@gmail.com)
- Yêu cầu hợp lệ sẽ được xử lý và gỡ bỏ hoàn toàn khỏi hệ thống phân phối trong vòng **24–48 giờ**.

---

## Bản Quyền & Giấy Phép / License & Disclaimer

Phần mềm được phát hành theo giấy phép **Proprietary Freeware License - All Rights Reserved** (Copyright © 2026 Anh Bảy Chọ / Tài Giáp).

- **Quyền sử dụng:** Người dùng được phép tải về và sử dụng miễn phí bản thực thi nguyên trạng cho mục đích cá nhân, phi thương mại.
- **Nghiêm cấm:** Mọi hành vi sao chép mã nguồn, dịch ngược, giải mã, can thiệp file thực thi, sửa đổi/xóa tên tác giả hoặc thương mại hóa dưới bất kỳ hình thức nào khi chưa có sự đồng ý bằng văn bản từ tác giả.

### Tuyên Bố Miễn Trừ Trách Nhiệm (Disclaimer)
- Công cụ độc lập hoạt động hoàn toàn qua Windows OCR API chuẩn, không chứa bất kỳ tài sản trí tuệ, hình ảnh hay mã nguồn nào của trò chơi Delta Force (Team Jade / TiMi Studio Group / Garena).
- *An independent utility operating strictly via standard Windows OCR APIs, containing no intellectual property, assets, images, or source code from Delta Force (Team Jade / TiMi Studio Group / Garena).*
