import os
import math
from PIL import Image, ImageDraw, ImageFilter

def create_delta_icon():
    os.makedirs("assets", exist_ok=True)
    size = 512
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)

    # 1. Dark Rounded Background (#0F171E) with subtle border (#23303E)
    margin = 24
    corner_radius = 96
    bg_box = [margin, margin, size - margin, size - margin]
    draw.rounded_rectangle(bg_box, radius=corner_radius, fill=(15, 23, 30, 255), outline=(35, 48, 62, 255), width=8)

    # Center coordinates
    cx, cy = size // 2, size // 2

    # 2. Glowing Layer for Delta Triangle & Crosshairs
    glow_img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    glow_draw = ImageDraw.Draw(glow_img)

    # Triangle points (Delta Δ)
    # Centered triangle: top vertex at cy - 130, bottom vertices at cy + 110, cx +- 140
    tri_top = (cx, cy - 125)
    tri_left = (cx - 135, cy + 115)
    tri_right = (cx + 135, cy + 115)
    triangle = [tri_top, tri_right, tri_left]

    # Draw glow for triangle (multiple passes with thick soft strokes)
    for w, a in [(36, 40), (24, 70), (14, 120)]:
        glow_draw.polygon(triangle, outline=(0, 245, 160, a))
        glow_draw.line([tri_top, tri_right, tri_left, tri_top], fill=(0, 245, 160, a), width=w, joint="curve")

    # Outer tactical circular brackets
    radius = 185
    glow_draw.ellipse([cx - radius, cy - radius, cx + radius, cy + radius], outline=(0, 245, 160, 50), width=6)

    # Blur glow layer
    glow_blurred = glow_img.filter(ImageFilter.GaussianBlur(radius=10))
    img.alpha_composite(glow_blurred)

    # 3. Sharp Foreground Elements
    fg_draw = ImageDraw.Draw(img)

    # Tactical corner brackets
    b_len = 32
    b_margin = margin + 28
    fg_draw.line([(b_margin, b_margin), (b_margin + b_len, b_margin)], fill=(0, 245, 160, 180), width=5)
    fg_draw.line([(b_margin, b_margin), (b_margin, b_margin + b_len)], fill=(0, 245, 160, 180), width=5)

    fg_draw.line([(size - b_margin, b_margin), (size - b_margin - b_len, b_margin)], fill=(0, 245, 160, 180), width=5)
    fg_draw.line([(size - b_margin, b_margin), (size - b_margin, b_margin + b_len)], fill=(0, 245, 160, 180), width=5)

    fg_draw.line([(b_margin, size - b_margin), (b_margin + b_len, size - b_margin)], fill=(0, 245, 160, 180), width=5)
    fg_draw.line([(b_margin, size - b_margin), (b_margin, size - b_margin - b_len)], fill=(0, 245, 160, 180), width=5)

    fg_draw.line([(size - b_margin, size - b_margin), (size - b_margin - b_len, size - b_margin)], fill=(0, 245, 160, 180), width=5)
    fg_draw.line([(size - b_margin, size - b_margin), (size - b_margin, size - b_margin - b_len)], fill=(0, 245, 160, 180), width=5)

    # Crosshair cardinal tick marks
    tick_len = 36
    fg_draw.line([(cx, cy - radius - 10), (cx, cy - radius - 10 - tick_len)], fill=(0, 245, 160, 230), width=6)
    fg_draw.line([(cx, cy + radius + 10), (cx, cy + radius + 10 + tick_len)], fill=(0, 245, 160, 230), width=6)
    fg_draw.line([(cx - radius - 10, cy), (cx - radius - 10 - tick_len, cy)], fill=(0, 245, 160, 230), width=6)
    fg_draw.line([(cx + radius + 10, cy), (cx + radius + 10 + tick_len, cy)], fill=(0, 245, 160, 230), width=6)

    # Circular reticle with 4 gaps
    arc_box = [cx - radius, cy - radius, cx + radius, cy + radius]
    for start_deg in [20, 110, 200, 290]:
        fg_draw.arc(arc_box, start=start_deg, end=start_deg + 50, fill=(0, 245, 160, 160), width=5)

    # Sharp Delta Triangle Outline (#00F5A0 emerald)
    fg_draw.line([tri_top, tri_right, tri_left, tri_top], fill=(0, 245, 160, 255), width=12, joint="curve")
    # Inner high-light stroke
    fg_draw.line([tri_top, tri_right, tri_left, tri_top], fill=(160, 255, 220, 255), width=4, joint="curve")

    # Center tactical reticle pip
    pip_radius = 8
    fg_draw.ellipse([cx - pip_radius, cy + 15 - pip_radius, cx + pip_radius, cy + 15 + pip_radius], fill=(0, 245, 160, 255))
    fg_draw.ellipse([cx - 3, cy + 15 - 3, cx + 3, cy + 15 + 3], fill=(255, 255, 255, 255))

    # Inner mini inverted triangle or chevron
    chev_top = (cx, cy + 50)
    chev_l = (cx - 40, cy - 20)
    chev_r = (cx + 40, cy - 20)
    fg_draw.line([chev_l, chev_top, chev_r], fill=(0, 245, 160, 180), width=6)

    # Save master PNG
    img.save("assets/icon.png", format="PNG")

    # Save ICO with all standard Windows sizes
    sizes = [(16, 16), (32, 32), (48, 48), (64, 64), (256, 256)]
    img.save("assets/icon.ico", format="ICO", sizes=sizes)

    # Save raw RGBA files for direct embedding in Rust ViewportBuilder
    for s in [32, 64]:
        resized = img.resize((s, s), Image.Resampling.LANCZOS)
        rgba_bytes = resized.tobytes("raw", "RGBA")
        with open(f"assets/icon_{s}.rgba", "wb") as f:
            f.write(rgba_bytes)

    print(f"[✓] Đã tạo thành công icon Delta Force tại 'assets/icon.ico' ({len(sizes)} kích thước).")
    print(f"[✓] Đã lưu 'assets/icon.png', 'assets/icon_32.rgba', 'assets/icon_64.rgba'.")

if __name__ == "__main__":
    create_delta_icon()
