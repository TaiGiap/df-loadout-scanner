import json
import os
import re
import sys
import time
from playwright.sync_api import sync_playwright

BASE_DFTOOLS_URL = "https://deltaforcetools.gg/vi/weapon-builds"
BASE_DFBUILDS_URL = "https://deltaforcebuilds.com"
BASE_FPSMETA_URL = "https://fpsmeta.gg/delta-force-meta"
OUTPUT_FILE = "builds.json"

WEAPON_TYPE_MAP = {
    # Assault Rifle / Battle Rifle
    "M4A1": "Assault Rifle", "AK-12": "Assault Rifle", "AKM": "Assault Rifle", "AKS-74": "Assault Rifle",
    "AS VAL": "Assault Rifle", "ASH-12": "Assault Rifle", "AUG": "Assault Rifle", "CAR-15": "Assault Rifle",
    "CI-19": "Assault Rifle", "G3": "Assault Rifle", "K416": "Assault Rifle", "K437": "Assault Rifle",
    "KC-17": "Assault Rifle", "KC17": "Assault Rifle", "M16A4": "Assault Rifle", "MCX LT": "Assault Rifle",
    "MK47": "Assault Rifle", "PTR-32": "Assault Rifle", "QBZ95-1": "Assault Rifle", "QBZ95-Z": "Assault Rifle",
    "RM277": "Assault Rifle", "SCAR-H": "Assault Rifle", "SG552": "Assault Rifle", "SR-3M": "Assault Rifle",
    "M7": "Assault Rifle", "AN-94": "Assault Rifle",
    # SMG
    "MP5": "SMG", "MP7": "SMG", "P90": "SMG", "SMG-45": "SMG", "UZI": "SMG", "VECTOR": "SMG",
    "VITYAZ": "SMG", "QCQ171": "SMG", "BIZON": "SMG", "AR-57": "SMG", "AKS-74U": "SMG", "SR9": "SMG",
    "MK4": "SMG",
    # LMG
    "M249": "LMG", "M250": "LMG", "PKM": "LMG", "QJB-201": "LMG", "QJB201": "LMG",
    # Marksman Rifle
    "M14": "Marksman Rifle", "MINI-14": "Marksman Rifle", "PSG-1": "Marksman Rifle",
    "SKS": "Marksman Rifle", "SR-25": "Marksman Rifle", "SVCH": "Marksman Rifle", "VSS": "Marksman Rifle",
    # Sniper Rifle
    "AWM": "Sniper Rifle", "M700": "Sniper Rifle", "M82": "Sniper Rifle", "R93": "Sniper Rifle",
    "SV-98": "Sniper Rifle", "SVD": "Sniper Rifle",
    # Shotgun
    "725 DOUBLE-BARREL": "Shotgun", "725": "Shotgun", "FS-12": "Shotgun",
    "M1014": "Shotgun", "M870": "Shotgun", "S12K": "Shotgun",
    # Pistol
    ".357": "Pistol", "93R": "Pistol", "DESERT EAGLE": "Pistol", "G-17": "Pistol",
    "G18": "Pistol", "M1911": "Pistol", "QSZ92G": "Pistol",
    # Special
    "COMPOUND BOW": "Special", "LEVER-ACTION": "Special", "LEVER-ACTION RIFLE": "Special",
    "MARLIN LEVER-ACTION": "Special",
}

def clean_text(text: str) -> str:
    if not text:
        return ""
    return re.sub(r"\s+", " ", text).strip()

AVATAR_OR_NON_BUILD_IMAGE_TOKENS = (
    "avatar",
    "user",
    "author",
    "profile",
    "member",
    "icon",
    "logo",
    "emoji",
    "badge",
    "placeholder",
    "sprite",
    "blank",
    "favicon",
    ".svg",
    ".avif",
)


def is_non_build_image_hint(text: str) -> bool:
    lower = (text or "").lower()
    return any(tok in lower for tok in AVATAR_OR_NON_BUILD_IMAGE_TOKENS)


def unwrap_next_image_url(u: str) -> str:
    if "/_next/image" not in u:
        return u
    try:
        from urllib.parse import urlparse, parse_qs, unquote
        parsed = urlparse(u)
        qs = parse_qs(parsed.query)
        inner = qs.get("url", [""])[0]
        if inner:
            inner = unquote(inner).strip()
            if inner.startswith("https://") or inner.startswith("http://"):
                return inner
    except Exception:
        pass
    return u


def normalize_preview_image_url(raw_url: str, base_url: str = "") -> str:
    if not raw_url:
        return ""
    u = raw_url.strip()
    lower = u.lower()
    if not u or lower.startswith("data:") or lower.startswith("blob:"):
        return ""
    if u.startswith("//"):
        u = "https:" + u
    elif u.startswith("/") and base_url:
        u = base_url.rstrip("/") + u
    u = unwrap_next_image_url(u)
    if is_non_build_image_hint(u.lower()):
        return ""
    if not u.startswith("https://"):
        return ""
    return u

def parse_shorthand_number(text: str) -> int:
    if not text:
        return 0
    m = re.search(r'(\d+(?:[.,]\d+)?)\s*([kKmM])?', text)
    if not m:
        return 0
    num_part = m.group(1)
    mult = m.group(2)
    if mult:
        num_part = num_part.replace(',', '.')
        try:
            val = float(num_part)
            if mult.upper() == 'K':
                val *= 1000
            elif mult.upper() == 'M':
                val *= 1000000
            return int(round(val))
        except ValueError:
            return 0
    else:
        if ',' in num_part:
            num_part = num_part.replace(',', '')
        elif '.' in num_part:
            parts = num_part.split('.')
            if len(parts) == 2 and len(parts[1]) == 3:
                num_part = ''.join(parts)
            else:
                num_part = parts[0]
        try:
            return int(num_part)
        except ValueError:
            return 0

def extract_copy_count(card_text: str) -> int:
    if not card_text:
        return 0
    for line in card_text.splitlines():
        line = line.strip()
        if any(kw in line.lower() for kw in ['đặc vụ', 'operator', 'sao chép', 'copi', 'copies']):
            cnt = parse_shorthand_number(line)
            if cnt > 0:
                return cnt
    return 0

def match_canonical_weapon(raw_name: str, code: str = "") -> str:
    cleaned = clean_text(raw_name).replace("\xa0", " ").upper()
    cleaned = cleaned.replace("QBZ-95-1", "QBZ95-1").replace("QBZ 95-1", "QBZ95-1")
    cleaned = cleaned.replace("AS-VAL", "AS VAL").replace("ASVAL", "AS VAL")
    cleaned = cleaned.replace("SR3M", "SR-3M").replace("SR 3M", "SR-3M")
    cleaned = cleaned.replace("MK-47", "MK47").replace("MK 47", "MK47")
    cleaned = cleaned.replace("MINI14", "MINI-14").replace("MINI 14", "MINI-14")
    cleaned = cleaned.replace("SR25", "SR-25").replace("SR 25", "SR-25")
    cleaned = cleaned.replace("SV98", "SV-98").replace("SV 98", "SV-98")
    cleaned = cleaned.replace("AR57", "AR-57").replace("AR 57", "AR-57")
    cleaned = cleaned.replace("CAR15", "CAR-15").replace("CAR 15", "CAR-15")
    cleaned = cleaned.replace("AK12", "AK-12").replace("AK 12", "AK-12")
    cleaned = cleaned.replace("AN94", "AN-94").replace("AN 94", "AN-94")
    cleaned = cleaned.replace("M16-A4", "M16A4").replace("M16 A4", "M16A4")

    stop_words = [
        "FUSIL DE ASALTO", "FUSIL DE TIRADOR", "STURMGEWEHR", "FUSIL",
        "COMPACT ASSAULT RIFLE", "ASSAULT RIFLE", "ASSULT RIFLE",
        "SUBMACHINE GUN", "SNIPER RIFLE", "MARKSMAN RIFLE",
        "BATTLE RIFLE", "GENERAL MACHINE GUN", "LIGHT MACHINE GUN",
        "MACHINE GUN", "SHOTGUN", "PISTOL",
    ]
    for stop in stop_words:
        cleaned = cleaned.replace(stop, "").strip()
    cleaned = cleaned.strip(":- ")

    if cleaned in WEAPON_TYPE_MAP:
        return cleaned

    for tok in cleaned.split():
        if tok in WEAPON_TYPE_MAP:
            return tok

    if code:
        code_upper = code.upper()
        sorted_keys = sorted(WEAPON_TYPE_MAP.keys(), key=len, reverse=True)
        for k in sorted_keys:
            if k in code_upper:
                return k

    return cleaned if cleaned else "UNKNOWN"

def get_weapon_type(weapon_name: str, raw_code: str = "") -> str:
    w = weapon_name.upper().strip()
    if w in WEAPON_TYPE_MAP:
        return WEAPON_TYPE_MAP[w]
    for k, v in WEAPON_TYPE_MAP.items():
        if w.startswith(k) or k.startswith(w):
            return v
    code_lower = raw_code.lower()
    if "submachine" in code_lower or "smg" in code_lower:
        return "SMG"
    if "sniper" in code_lower or "tirador" in code_lower:
        return "Sniper Rifle"
    if "marksman" in code_lower:
        return "Marksman Rifle"
    if "machine gun" in code_lower or "lmg" in code_lower:
        return "LMG"
    if "shotgun" in code_lower:
        return "Shotgun"
    if "pistol" in code_lower:
        return "Pistol"
    if "assault" in code_lower or "battle rifle" in code_lower or "fusil" in code_lower:
        return "Assault Rifle"
    return "Special"

def parse_mode_from_text_or_code(text: str, code: str = "") -> str:
    combined = (text + " " + code).lower()
    if any(k in combined for k in ["warfare", "krieg", "conflicto b", "detonation", "chiến trường"]):
        return "Warfare"
    if any(k in combined for k in ["operation", "operaci", "extrac", "hazard", "sinh tồn"]):
        return "Operations"
    return "Warfare"

def parse_server_from_text(text: str, tags: list = None, default: str = "Infinity") -> str:
    combined = (text + " " + " ".join(tags or [])).lower()
    if "garena" in combined:
        return "Garena"
    if "global" in combined or "infinity" in combined:
        return "Infinity"
    return default


# ==========================================
# SOURCE 1: DeltaForceTools.gg
# ==========================================
def extract_dftools_cards_from_page(page, server_label: str, mode_label: str):
    results = []
    try:
        page.wait_for_selector("[data-track-click='copies']", timeout=12000)
    except Exception:
        return results

    copy_buttons = page.locator("[data-track-click='copies']").all()
    for btn in copy_buttons:
        try:
            track_prop_raw = btn.get_attribute("data-track-prop")
            loadout_code = ""
            weapon_name = ""

            if track_prop_raw:
                try:
                    prop_json = json.loads(track_prop_raw)
                    loadout_code = prop_json.get("change_weapon_code", "")
                    weapon_name = prop_json.get("weapon_name", "")
                except Exception:
                    pass

            card_el = btn.locator("xpath=./ancestor::div[contains(@class, 'flex') and contains(@class, 'cursor-pointer') and contains(@class, 'flex-col')][last()]")
            if card_el.count() == 0:
                card_el = btn.locator("xpath=./ancestor::div[contains(@class, 'bg-full-cover')][last()]")

            if not weapon_name and card_el.count() > 0:
                name_el = card_el.locator(".font-extrabold.text-white").first
                if name_el.count() > 0:
                    weapon_name = name_el.inner_text()

            if not loadout_code and card_el.count() > 0:
                code_el = card_el.locator(".truncate.text-center.text-xs").first
                if code_el.count() > 0:
                    loadout_code = clean_text(code_el.inner_text())

            if not loadout_code:
                continue

            weapon_name_clean = match_canonical_weapon(weapon_name, loadout_code)

            tags = ["DFTools"]
            copy_count = 0
            image_url = ""
            if card_el.count() > 0:
                card_text = card_el.inner_text()
                copy_count = extract_copy_count(card_text)
                img_candidates = card_el.evaluate("""(card) => {
                    const imgs = Array.from(card.querySelectorAll('img'));
                    return imgs.map(im => {
                        const rect = im.getBoundingClientRect ? im.getBoundingClientRect() : { width: 0, height: 0 };
                        return {
                            src: im.getAttribute('src') || im.getAttribute('data-src') || '',
                            alt: im.getAttribute('alt') || '',
                            cls: im.getAttribute('class') || '',
                            w: im.naturalWidth || parseInt(im.getAttribute('width') || '0', 10) || Math.round(rect.width || 0),
                            h: im.naturalHeight || parseInt(im.getAttribute('height') || '0', 10) || Math.round(rect.height || 0),
                        };
                    });
                }""")
                best_score = -1
                for im in img_candidates:
                    norm_src = normalize_preview_image_url(im.get("src", ""), "https://deltaforcetools.gg")
                    if not norm_src:
                        continue
                    if is_non_build_image_hint(im.get("alt", "")) or is_non_build_image_hint(im.get("cls", "")):
                        continue
                    w = int(im.get("w") or 0)
                    h = int(im.get("h") or 0)
                    if w > 0 and h > 0:
                        if w <= 72 and h <= 72:
                            continue
                        if w == h and w <= 128:
                            continue
                    score = (w * h) + (50000 if w > h * 1.15 else 0)
                    if score > best_score:
                        best_score = score
                        image_url = norm_src
                tag_nodes = card_el.locator("span.inline-block, .font-bold").all()
                for t in tag_nodes:
                    txt = clean_text(t.inner_text())
                    if (
                        txt
                        and txt not in [
                            "Lượt sao chép", "Đặc Vụ sao chép",
                            "Chiến Trường Toàn Diện", "Chiến Dịch Sinh Tồn",
                            "Havoc Warfare", "Hazard Operations",
                        ]
                        and not txt.isdigit()
                        and not txt.startswith("bởi ")
                        and not txt.startswith("Được ")
                        and txt.upper() != weapon_name_clean
                    ):
                        tags.append(txt)

            classification = ", ".join(tags) if tags else "Tiêu chuẩn"
            weapon_type = get_weapon_type(weapon_name_clean, loadout_code)

            item = {
                "server": server_label,
                "mode": mode_label,
                "weapon_type": weapon_type,
                "weapon": weapon_name_clean,
                "classification": classification,
                "tags": tags,
                "copy_count": copy_count,
                "loadout_code": loadout_code
            }
            if image_url:
                item["image_url"] = image_url
            results.append(item)
        except Exception:
            pass

    return results

def get_dftools_total_pages(page, url: str) -> int:
    try:
        page.goto(url, wait_until="domcontentloaded", timeout=45000)
        page.wait_for_selector("nav[aria-label='pagination']", timeout=5000)
        page_links = page.locator("a[data-slot='pagination-link']").all()
        page_nums = []
        for link in page_links:
            txt = link.inner_text().strip()
            if txt.isdigit():
                page_nums.append(int(txt))
        if page_nums:
            return max(page_nums)
    except Exception:
        pass
    return 1

def scrape_dftools(page, seen_codes: set) -> list:
    print("\n" + "="*50)
    print("[1/3] NGUỒN 1: DeltaForceTools.gg")
    print("="*50)
    builds = []
    combos = [
        {"server_param": "Garena", "mode_param": "Havoc Warfare", "server_label": "Garena", "mode_label": "Warfare"},
        {"server_param": "Garena", "mode_param": "Hazard Operations", "server_label": "Garena", "mode_label": "Operations"},
        {"server_param": "Infinity", "mode_param": "Havoc Warfare", "server_label": "Infinity", "mode_label": "Warfare"},
        {"server_param": "Infinity", "mode_param": "Hazard Operations", "server_label": "Infinity", "mode_label": "Operations"},
    ]

    for idx, combo in enumerate(combos, start=1):
        s_param = combo["server_param"]
        m_param = combo["mode_param"].replace(" ", "+")
        s_label = combo["server_label"]
        m_label = combo["mode_label"]

        first_url = f"{BASE_DFTOOLS_URL}?mode={m_param}&server_region={s_param}&page=1"
        print(f"\n  [{idx}/4] Thu thập: Server={s_label} | Mode={m_label}...")
        total_pages = get_dftools_total_pages(page, first_url)
        print(f"       -> Số trang: {total_pages}")

        combo_new = 0
        for p in range(1, total_pages + 1):
            if p > 1:
                page_url = f"{BASE_DFTOOLS_URL}?mode={m_param}&server_region={s_param}&page={p}"
                try:
                    page.goto(page_url, wait_until="domcontentloaded", timeout=45000)
                except Exception as e:
                    print(f"       [!] Lỗi khi tải trang {p}: {e}")
                    continue

            cards = extract_dftools_cards_from_page(page, s_label, m_label)
            page_new = 0
            for item in cards:
                code_key = item["loadout_code"].strip()
                if code_key and code_key not in seen_codes:
                    seen_codes.add(code_key)
                    builds.append(item)
                    page_new += 1
                    combo_new += 1

            print(f"       Trang {p:02d}/{total_pages:02d}: +{page_new} mới (Tổ hợp: {combo_new} | Tổng nguồn: {len(builds)})")

    print(f"\n[✓] DeltaForceTools.gg hoàn tất: thu thập được {len(builds)} cấu hình.")
    return builds


# ==========================================
# SOURCE 2: DeltaForceBuilds.com
# ==========================================
def scrape_dfbuilds(page, seen_codes: set) -> list:
    print("\n" + "="*50)
    print("[2/3] NGUỒN 2: DeltaForceBuilds.com")
    print("="*50)
    builds = []

    # 1. Thu thập trang /latest
    print("\n  [2.1] Thu thập trang /latest (Latest Builds)...")
    try:
        page.goto(f"{BASE_DFBUILDS_URL}/latest", wait_until="domcontentloaded", timeout=30000)
        page.wait_for_timeout(1500)
        latest_cards = page.evaluate("""() => {
            const results = [];
            const codeBlocks = document.querySelectorAll('.feed-code-block');
            for (const block of codeBlocks) {
                const code = block.getAttribute('data-code') || (block.querySelector('code') ? block.querySelector('code').innerText.trim() : '');
                if (!code) continue;

                const card = block.closest('.feed-card') || block.parentElement;
                let gunName = '';
                let desc = '';
                let tags = [];
                let imageUrl = '';

                if (card) {
                    const gunBadge = card.querySelector('.feed-gun-badge');
                    if (gunBadge) gunName = gunBadge.innerText.trim();
                    const descEl = card.querySelector('.feed-desc');
                    if (descEl) desc = descEl.innerText.trim();
                    const tagEls = card.querySelectorAll('.feed-mode-tag, .tag');
                    for (const t of tagEls) {
                        const txt = t.innerText.trim();
                        if (txt) tags.push(txt);
                    }
                    const imgs = Array.from(card.querySelectorAll('img'));
                    let bestScore = -1;
                    const badHint = /avatar|user|author|profile|member|icon|logo|emoji|badge|placeholder|sprite|\\.svg/i;
                    for (const im of imgs) {
                        const src = im.getAttribute('src') || im.getAttribute('data-src') || '';
                        const alt = im.getAttribute('alt') || '';
                        const cls = im.getAttribute('class') || '';
                        if (!src || src.startsWith('data:') || src.startsWith('blob:')) continue;
                        if (badHint.test(src) || badHint.test(alt) || badHint.test(cls)) continue;
                        const rect = im.getBoundingClientRect ? im.getBoundingClientRect() : { width: 0, height: 0 };
                        const w = im.naturalWidth || parseInt(im.getAttribute('width') || '0', 10) || Math.round(rect.width || 0);
                        const h = im.naturalHeight || parseInt(im.getAttribute('height') || '0', 10) || Math.round(rect.height || 0);
                        if (w > 0 && h > 0) {
                            if (w <= 72 && h <= 72) continue;
                            if (w === h && w <= 128) continue;
                        }
                        const score = (w * h) + (w > h * 1.15 ? 50000 : 0);
                        if (score > bestScore) {
                            bestScore = score;
                            imageUrl = src;
                        }
                    }
                }
                results.push({ code, gun: gunName, desc, tags, imageUrl });
            }
            return results;
        }""")

        for c in latest_cards:
            code = c["code"].strip()
            if not code or code in seen_codes:
                continue
            seen_codes.add(code)

            weapon = match_canonical_weapon(c["gun"], code)
            w_type = get_weapon_type(weapon, code)
            mode = parse_mode_from_text_or_code(" ".join(c["tags"]), code)
            server = parse_server_from_text(" ".join(c["tags"]), c["tags"], default="Infinity")
            tags = ["DFBuilds"] + [t for t in c["tags"] if t not in ["DFBuilds"]]
            desc = c["desc"] if c["desc"] else f"Cộng đồng {weapon}"
            image_url = normalize_preview_image_url(c.get("imageUrl", ""), BASE_DFBUILDS_URL)

            item = {
                "server": server,
                "mode": mode,
                "weapon_type": w_type,
                "weapon": weapon,
                "classification": desc,
                "tags": tags,
                "copy_count": 0,
                "loadout_code": code
            }
            if image_url:
                item["image_url"] = image_url
            builds.append(item)
        print(f"       -> Đã lấy {len(builds)} cấu hình từ /latest")
    except Exception as e:
        print(f"       [!] Lỗi khi lấy /latest: {e}")

    # 2. Lấy danh sách các khẩu súng trên trang chủ /browse guns
    print("\n  [2.2] Quét danh mục súng và lấy build từ từng trang vũ khí...")
    try:
        page.goto(BASE_DFBUILDS_URL, wait_until="domcontentloaded", timeout=30000)
        page.wait_for_timeout(1000)
        gun_links = page.evaluate("""() => {
            const links = document.querySelectorAll("a[href*='/gun/']");
            return Array.from(links).map(l => ({
                href: l.getAttribute('href'),
                text: l.innerText.trim()
            }));
        }""")
        print(f"       -> Tìm thấy {len(gun_links)} vũ khí trên DeltaForceBuilds. Bắt đầu duyệt...")

        for idx, g in enumerate(gun_links, start=1):
            gun_url = f"{BASE_DFBUILDS_URL}{g['href']}"
            raw_gun_name = g['text'].split('\n')[1] if '\n' in g['text'] else g['text']
            try:
                page.goto(gun_url, wait_until="domcontentloaded", timeout=15000)
                page.wait_for_timeout(300)
                gun_cards = page.evaluate("""() => {
                    const results = [];
                    const codeBlocks = document.querySelectorAll('.feed-code-block');
                    for (const block of codeBlocks) {
                        const code = block.getAttribute('data-code') || (block.querySelector('code') ? block.querySelector('code').innerText.trim() : '');
                        if (!code) continue;

                        const card = block.closest('.feed-card') || block.parentElement;
                        let desc = '';
                        let tags = [];
                        let imageUrl = '';

                        if (card) {
                            const descEl = card.querySelector('.feed-desc');
                            if (descEl) desc = descEl.innerText.trim();
                            const tagEls = card.querySelectorAll('.feed-mode-tag, .tag');
                            for (const t of tagEls) {
                                const txt = t.innerText.trim();
                                if (txt) tags.push(txt);
                            }
                            const imgs = card.querySelectorAll('img');
                            for (const im of imgs) {
                                const src = im.getAttribute('src') || im.getAttribute('data-src') || '';
                                if (src && !src.startsWith('data:')) {
                                    imageUrl = src;
                                    break;
                                }
                            }
                        }
                        results.push({ code, desc, tags, imageUrl });
                    }
                    return results;
                }""")

                new_count = 0
                for c in gun_cards:
                    code = c["code"].strip()
                    if not code or code in seen_codes:
                        continue
                    seen_codes.add(code)

                    weapon = match_canonical_weapon(raw_gun_name, code)
                    w_type = get_weapon_type(weapon, code)
                    mode = parse_mode_from_text_or_code(" ".join(c["tags"]), code)
                    server = parse_server_from_text(" ".join(c["tags"]), c["tags"], default="Infinity")
                    tags = ["DFBuilds"] + [t for t in c["tags"] if t not in ["DFBuilds"]]
                    desc = c["desc"] if c["desc"] else f"Cộng đồng {weapon}"
                    image_url = normalize_preview_image_url(c.get("imageUrl", ""), BASE_DFBUILDS_URL)

                    item = {
                        "server": server,
                        "mode": mode,
                        "weapon_type": w_type,
                        "weapon": weapon,
                        "classification": desc,
                        "tags": tags,
                        "copy_count": 0,
                        "loadout_code": code
                    }
                    if image_url:
                        item["image_url"] = image_url
                    builds.append(item)
                    new_count += 1

                if idx % 10 == 0 or idx == len(gun_links):
                    print(f"       [{idx:02d}/{len(gun_links):02d}] {weapon:12}: +{new_count:2d} mới (Tổng DFBuilds: {len(builds)})")
            except Exception as e:
                print(f"       [{idx:02d}/{len(gun_links):02d}] [!] Lỗi tại {g['href']}: {e}")
                continue

    except Exception as e:
        print(f"       [!] Lỗi khi quét danh mục vũ khí DFBuilds: {e}")

    print(f"\n[✓] DeltaForceBuilds.com hoàn tất: thu thập được {len(builds)} cấu hình.")
    return builds


# ==========================================
# SOURCE 3: FPSMeta.gg
# ==========================================
def scrape_fpsmeta(page, seen_codes: set) -> list:
    print("\n" + "="*50)
    print("[3/3] NGUỒN 3: FPSMeta.gg (Meta Builds Theo Mùa)")
    print("="*50)
    builds = []

    try:
        page.goto(BASE_FPSMETA_URL, wait_until="domcontentloaded", timeout=30000)
        page.wait_for_timeout(2000)

        raw_text = page.inner_text("body")
        lines = [clean_text(l) for l in raw_text.split("\n") if clean_text(l)]

        for i, line in enumerate(lines):
            code = ""
            tier = ""
            raw_gun = ""

            if line == "CODE:" and i + 1 < len(lines):
                code = lines[i+1].strip()
                raw_gun = lines[i-2] if i >= 2 else ""
                tier = lines[i-3] if i >= 3 else ""
            elif line == "Build Code" and i + 1 < len(lines):
                code = lines[i+1].strip()
                raw_gun = lines[i-5] if i >= 5 else ""
                tier = "Absolute Meta"

            if code and "-" in code and any(c.isdigit() for c in code):
                if code in seen_codes:
                    continue
                seen_codes.add(code)

                weapon = match_canonical_weapon(raw_gun, code)
                w_type = get_weapon_type(weapon, code)
                mode = parse_mode_from_text_or_code(tier + " " + raw_gun, code)
                tags = ["FPSMeta", "Meta"]
                if "ABSOLUTE" in tier.upper():
                    tags.append("Absolute Meta")
                elif "BUDGET" in tier.upper():
                    tags.append("Budget")
                elif "EXPENSIVE" in tier.upper():
                    tags.append("Top Tier")

                classification = "Meta (" + ", ".join(tags[1:]) + ")"
                builds.append({
                    "server": "Infinity",
                    "mode": mode,
                    "weapon_type": w_type,
                    "weapon": weapon,
                    "classification": classification,
                    "tags": tags,
                    "copy_count": 0,
                    "loadout_code": code
                })

        print(f"\n[✓] FPSMeta.gg hoàn tất: thu thập được {len(builds)} cấu hình Meta.")
    except Exception as e:
        print(f"\n[!] Lỗi khi cào dữ liệu FPSMeta: {e}")

    return builds


# ==========================================
# SEMANTIC DATA VALIDATOR
# ==========================================
VALIDATOR_MODE_KEYWORDS = [
    # English
    "warfare", "operations", "detonation", "extraction",
    # Chinese Traditional & Simplified
    "全面戰場", "全面战场", "烽火地帶", "烽火地带", "大战场", "大戰場", "全战场", "全戰場", "撤離", "撤离",
    # Russian
    "сражения", "операции",
    # Spanish / Portuguese
    "conflicto bélico", "conflicto belico", "operaciones", "operación: extracción", "operacion: extraccion",
    "operación", "operacion", "conquista", "zona de risco",
    # German
    "krieg", "einsätze", "einsatze",
    # French
    "guerre", "opérations",
    # Turkish
    "savaş", "savas", "operasyonlar", "operasyon",
    # Arabic
    "الحرب", "العمليات",
    # Japanese / Korean
    "オペレーションズ", "全面戦争", "전면전", "작전",
    # Vietnamese
    "chiến trường toàn diện", "chien truong toan dien",
    "chiến trường", "chien truong", "chiến dịch", "chien dich", "sinh tồn", "sinh ton",
    # Typo
    "0perations",
]

VALIDATOR_TRANSLATIONS = {
    # Russian Cyrillic names
    "ВСС": "VSS",
    "АК-12": "AK-12",
    "АКМ": "AKM",
    "АШ-12": "ASH-12",
    "АШ12": "ASH-12",
    "ВИТЯЗЬ": "VITYAZ",
    "АС ВАЛ": "AS VAL",
    "АС «ВАЛ»": "AS VAL",
    "СР-3M": "SR-3M",
    "СР-3М": "SR-3M",
    "СВД": "SVD",
    "СВ-98": "SV-98",
    "СВЧ": "SVCH",
    "ПКМ": "PKM",
    "МАУЗЕР": "M700",

    # Chinese names
    "騰龍": "CI-19",
    "腾龙": "CI-19",
    "勇士": "VITYAZ",
    "野牛": "BIZON",
    "旋风": "SR-3M",
    "旋風": "SR-3M",
    "巨浪": "AS VAL",
    "无畏": "ASH-12",
    "無畏": "ASH-12",

    # Korean names
    "데저트 이글": "DESERT EAGLE",

    # Synonyms / Variations
    "LEVER-ACTION RIFLE": "MARLIN LEVER-ACTION",
    "LEVER-ACTION": "MARLIN LEVER-ACTION",
    "LEVER ACTION RIFLE": "MARLIN LEVER-ACTION",
    "LEVER ACTION": "MARLIN LEVER-ACTION",
    "DOUBLE-BARREL": "725 DOUBLE-BARREL",
    "DOUBLE BARREL": "725 DOUBLE-BARREL",
    "725": "725 DOUBLE-BARREL",
    "G17": "G-17",
    "G-17": "G-17",
    "KC17": "KC-17",
    "KC-17": "KC-17",
    "QJB201": "QJB-201",
    "QJB-201": "QJB-201",
    "QSZ92G": "QSZ-92G",
    "QSZ-92G": "QSZ-92G",
    "COMPOUND-BOW": "COMPOUND BOW",
    "COMPOUND BOW": "COMPOUND BOW",
}

CANONICAL_WEAPONS_LIST = [
    "725 DOUBLE-BARREL", ".357", "93R", "AK-12", "AKM", "AKS-74U", "AKS-74", "AN-94", "AR-57",
    "AS VAL", "ASH-12", "AUG", "AWM", "BIZON", "CAR-15", "CI-19", "COMPOUND BOW", "DESERT EAGLE",
    "FS-12", "G-17", "G18", "G3", "K416", "K437", "KC-17", "M1014", "M14", "M16A4", "M1911",
    "M249", "M250", "M4A1", "M700", "M7", "M82", "M870", "MARLIN LEVER-ACTION", "MCX LT", "MDR",
    "MINI-14", "MK47", "MK4", "MP5", "MP7", "P90", "PKM", "PSG-1", "PTR-32", "QBZ95-1", "QBZ95-Z",
    "QCQ171", "QJB-201", "QSZ-92G", "R93", "RM277", "S12K", "SCAR-H", "SG552", "SKS", "SMG-45",
    "SR-25", "SR-3M", "SR9", "SV-98", "SVCH", "SVD", "THOMPSON", "UZI", "VECTOR", "VITYAZ", "VSS"
]
CANONICAL_WEAPONS_SORTED = sorted(CANONICAL_WEAPONS_LIST, key=len, reverse=True)

def canonical_weapon_key(w: str) -> str:
    if not w:
        return ""
    wu = w.upper().strip()
    return VALIDATOR_TRANSLATIONS.get(wu, wu)

def extract_weapon_from_loadout_code(code: str):
    code = code.strip()
    lines = [l.strip() for l in code.splitlines() if l.strip()]
    for l in reversed(lines):
        if re.search(r'-[A-Za-z0-9]{15,25}$', l):
            code = l
            break

    m_hash = re.search(r'-([A-Za-z0-9]{15,25})$', code)
    if not m_hash:
        return None, None, code

    prefix = code[:m_hash.start()]

    # Check mode keyword
    parts = prefix.split('-')
    if len(parts) >= 2:
        last_part = parts[-1].strip().lower()
        for mk in VALIDATOR_MODE_KEYWORDS:
            if mk == last_part or mk in last_part:
                return '-'.join(parts[:-1]).strip(), parts[-1].strip(), code

    for mk in sorted(VALIDATOR_MODE_KEYWORDS, key=len, reverse=True):
        pattern = re.compile(rf'-({re.escape(mk)})(?:-|$)', re.IGNORECASE)
        m = pattern.search(prefix)
        if m:
            return prefix[:m.start()].strip(), m.group(1).strip(), code

    return prefix, None, code

def detect_weapon_in_extracted_string(extracted_str: str) -> str:
    s_raw = extracted_str.strip().replace("\xa0", " ")
    s_upper = s_raw.upper()

    # 1. Check translations & synonyms
    for k, v in VALIDATOR_TRANSLATIONS.items():
        if k in s_upper or k in s_raw:
            return v

    # 2. Normalize common character variations
    s = s_upper
    s = s.replace("QBZ-95-1", "QBZ95-1").replace("QBZ 95-1", "QBZ95-1").replace("QBZ 95 1", "QBZ95-1")
    s = s.replace("QBZ-95-Z", "QBZ95-Z").replace("QBZ 95-Z", "QBZ95-Z")
    s = s.replace("AS-VAL", "AS VAL").replace("ASVAL", "AS VAL")
    s = s.replace("SR3M", "SR-3M").replace("SR 3M", "SR-3M")
    s = s.replace("MK-47", "MK47").replace("MK 47", "MK47")
    s = s.replace("MINI14", "MINI-14").replace("MINI 14", "MINI-14")
    s = s.replace("SR25", "SR-25").replace("SR 25", "SR-25")
    s = s.replace("SV98", "SV-98").replace("SV 98", "SV-98")
    s = s.replace("AR57", "AR-57").replace("AR 57", "AR-57")
    s = s.replace("CAR15", "CAR-15").replace("CAR 15", "CAR-15")
    s = s.replace("AK12", "AK-12").replace("AK 12", "AK-12")
    s = s.replace("AN94", "AN-94").replace("AN 94", "AN-94")
    s = s.replace("M16-A4", "M16A4").replace("M16 A4", "M16A4")
    s = s.replace("M82A1", "M82").replace("M82-A1", "M82")
    s = s.replace("SG-552", "SG552").replace("SG 552", "SG552")
    s = s.replace("QCQ-171", "QCQ171").replace("QCQ 171", "QCQ171")
    s = s.replace("QJB-201", "QJB-201").replace("QJB201", "QJB-201")
    s = s.replace("DOUBLE-BARREL", "725 DOUBLE-BARREL").replace("DOUBLE BARREL", "725 DOUBLE-BARREL")
    s = s.replace("COMPOUND-BOW", "COMPOUND BOW")
    s = s.replace("UZL", "UZI")
    s = s.replace("KC17", "KC-17").replace("KC 17", "KC-17")

    # Strip concatenated category suffixes (e.g. M4A1AssaultRifle)
    for cat in ["ASSAULTRIFLE", "SNIPERRIFLE", "LIGHTMACHINEGUN", "GENERALMACHINEGUN", "SUBMACHINEGUN", "MARKSMANRIFLE"]:
        if s.endswith(cat) and len(s) > len(cat):
            s = s[:-len(cat)].strip()

    # 3. Match weapons with boundary
    for w in CANONICAL_WEAPONS_SORTED:
        pattern = re.compile(rf'(?<![A-Za-z0-9]){re.escape(w)}(?![A-Za-z0-9])')
        if pattern.search(s):
            return w

    # 4. Try token-by-token
    tokens = re.split(r'[^A-Za-z0-9\-.]+', s)
    for t in tokens:
        for w in CANONICAL_WEAPONS_SORTED:
            if t == w:
                return w

    return None

def validate_builds(builds: list) -> list:
    """
    Nâng cấp bộ kiểm duyệt ngữ nghĩa (Semantic Data Validator):
      1. Làm sạch chuỗi tuyệt đối: strip() mọi trường, loại bỏ \r, \n thừa.
      2. loadout_code >= 18 ký tự, có ít nhất 2 dấu '-', kết thúc bằng chữ-số hợp lệ.
      3. Đối chiếu chéo tên súng thực tế bên trong loadout_code với trường weapon:
         Loại bỏ ngay lập tức nếu loadout_code thuộc về khẩu súng khác hẳn trường weapon hoặc là mã rác không có tên súng.
      4. copy_count luôn là số nguyên >= 0 (không bao giờ null).
      5. Tự động đồng bộ trường mode và tags nếu mâu thuẫn với nội dung loadout_code.
      6. Loại bỏ bản ghi trùng loadout_code (giữ bản ghi đầu tiên).
    """
    seen_codes: set = set()
    valid = []
    removed_mismatch = 0
    removed_invalid = 0
    removed_dup = 0

    print("\n" + "="*60)
    print("[Semantic Validator] Đang kiểm tra ngữ nghĩa và tính hợp lệ dữ liệu...")
    print("="*60)

    for b in builds:
        # 1. Làm sạch chuỗi tuyệt đối
        for k, v in list(b.items()):
            if isinstance(v, str):
                b[k] = v.strip()
            elif isinstance(v, list):
                b[k] = [x.strip() if isinstance(x, str) else x for x in v]

        code = b.get("loadout_code", "")
        # Nếu mã có nhiều dòng (ví dụ chú thích kèm theo mã), lấy dòng chứa mã hợp lệ
        lines = [l.strip() for l in code.splitlines() if l.strip()]
        for l in reversed(lines):
            if re.search(r'-[A-Za-z0-9]{15,25}$', l):
                code = l
                b["loadout_code"] = code
                break

        # 2. Kiểm tra cấu trúc mã
        if len(code) < 18 or code.count("-") < 2 or not re.search(r'[A-Za-z0-9]$', code):
            removed_invalid += 1
            continue

        # 3. Đối chiếu chéo tên súng trong loadout_code với trường weapon
        w_meta = canonical_weapon_key(b.get("weapon", ""))
        w_ext, m_ext, code_clean = extract_weapon_from_loadout_code(code)

        if not w_ext:
            removed_invalid += 1
            continue

        w_detected = detect_weapon_in_extracted_string(w_ext)
        if not w_detected:
            print(f"  [X] Loại bỏ mã súng rác (không có tên súng): weapon='{b.get('weapon')}', extracted='{w_ext}', code='{code}'")
            removed_invalid += 1
            continue

        w_detected_canon = canonical_weapon_key(w_detected)
        if w_detected_canon != w_meta:
            print(f"  [X] Loại bỏ bản ghi sai súng: Metadata='{b.get('weapon')}' vs Code='{w_detected_canon}' (Extracted: '{w_ext}', Code: '{code[:60]}...')")
            removed_mismatch += 1
            continue

        # Chuẩn hóa lại trường weapon cho nhất quán
        b["weapon"] = w_meta

        # 4. Đảm bảo copy_count là số nguyên >= 0
        try:
            b["copy_count"] = max(0, int(b.get("copy_count") or 0))
        except (ValueError, TypeError):
            b["copy_count"] = 0

        # 5. Đồng bộ chế độ mode
        code_lower = code.lower()
        warfare_kws = ["warfare", "krieg", "conflicto b", "detonation", "chiến trường", "conquista", "guerre", "savaş", "savas", "전면전", "全面"]
        operations_kws = ["operation", "operaci", "extrac", "hazard", "sinh tồn", "zona de risco", "0peration", "operasyon", "작전", "烽火"]
        is_warfare = any(k in code_lower for k in warfare_kws)
        is_operations = any(k in code_lower for k in operations_kws)

        if is_warfare and not is_operations and b.get("mode") == "Operations":
            b["mode"] = "Warfare"
        elif is_operations and not is_warfare and b.get("mode") == "Warfare":
            b["mode"] = "Operations"

        # 6. Loại bỏ trùng lặp theo loadout_code
        if code in seen_codes:
            removed_dup += 1
            continue
        seen_codes.add(code)
        valid.append(b)

    total_removed = removed_mismatch + removed_invalid + removed_dup
    total_input = len(valid) + total_removed
    print(f"\n[Validator Summary] Kết quả kiểm tra dữ liệu ngữ nghĩa:")
    print(f"  - Tổng đầu vào               : {total_input} bản build")
    print(f"  - Sai khác tên súng (loại bỏ): {removed_mismatch}")
    print(f"  - Mã rác/không hợp lệ        : {removed_invalid}")
    print(f"  - Trùng lặp (loại bỏ)        : {removed_dup}")
    print(f"  - Tổng bản ghi loại bỏ       : {total_removed}")
    print(f"  - Giữ lại hợp lệ 100%        : {len(valid)} bản build")
    return valid

def clean_existing_builds():
    if not os.path.exists(OUTPUT_FILE):
        print(f"[!] Không tìm thấy file {OUTPUT_FILE}")
        return
    print(f"[i] Đang đọc dữ liệu từ '{OUTPUT_FILE}' để làm sạch ngữ nghĩa...")
    with open(OUTPUT_FILE, "r", encoding="utf-8") as f:
        data = json.load(f)
    cleaned = validate_builds(data)
    with open(OUTPUT_FILE, "w", encoding="utf-8") as f:
        json.dump(cleaned, f, ensure_ascii=False, indent=2)
    print(f"\n[✓] Đã làm sạch và cập nhật thành công '{OUTPUT_FILE}' ({len(cleaned)} bản build hợp lệ).")


# ==========================================
# MAIN EXECUTION
# ==========================================
def run_fetch():
    is_headless = os.environ.get("CI") == "true" or os.environ.get("HEADLESS", "").lower() in ("true", "1")
    print("="*60)
    print("DF LOADOUT SCANNER - BỘ THU THẬP DỮ LIỆU ĐA NGUỒN")
    print(f"Trạng thái Playwright: Headless={is_headless}")
    print("="*60)

    seen_codes = set()
    all_builds = []
    source_stats = {
        "DFTools": 0,
        "DFBuilds": 0,
        "FPSMeta": 0
    }

    # Đọc dữ liệu cũ từ builds.json để dự phòng chống lỗi khi bị chặn hoặc đổi UI
    existing_builds_by_source = {
        "DFTools": [],
        "DFBuilds": [],
        "FPSMeta": []
    }
    if os.path.exists(OUTPUT_FILE):
        try:
            with open(OUTPUT_FILE, "r", encoding="utf-8") as f:
                prev_data = json.load(f)
                if isinstance(prev_data, list):
                    for b in prev_data:
                        # Chuẩn hóa copy_count đảm bảo luôn là số nguyên, không bao giờ null
                        if "copy_count" not in b or b["copy_count"] is None:
                            b["copy_count"] = 0
                        else:
                            try:
                                b["copy_count"] = int(b["copy_count"])
                            except Exception:
                                b["copy_count"] = 0

                        tags = b.get("tags", [])
                        classif = b.get("classification", "")
                        if "DFTools" in tags or "DFTools" in classif:
                            existing_builds_by_source["DFTools"].append(b)
                        elif "DFBuilds" in tags or "DFBuilds" in classif:
                            existing_builds_by_source["DFBuilds"].append(b)
                        elif "FPSMeta" in tags or "FPSMeta" in classif:
                            existing_builds_by_source["FPSMeta"].append(b)
            print(f"[i] Đã nạp dữ liệu cũ dự phòng từ '{OUTPUT_FILE}':")
            print(f"    - DFTools : {len(existing_builds_by_source['DFTools']):4d} cấu hình")
            print(f"    - DFBuilds: {len(existing_builds_by_source['DFBuilds']):4d} cấu hình")
            print(f"    - FPSMeta : {len(existing_builds_by_source['FPSMeta']):4d} cấu hình")
        except Exception as e:
            print(f"[!] Không thể đọc '{OUTPUT_FILE}' cũ làm dự phòng: {e}")

    t_start = time.time()

    with sync_playwright() as p:
        browser = p.chromium.launch(
            headless=is_headless,
            args=[
                "--disable-blink-features=AutomationControlled",
                "--start-maximized"
            ]
        )
        context = browser.new_context(
            no_viewport=True,
            user_agent="Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36"
        )
        page = context.new_page()

        # Tăng tốc độ duyệt web tối đa bằng cách chặn tài nguyên hình ảnh, media, font
        def intercept_route(route):
            if route.request.resource_type in ["image", "media", "font"]:
                route.abort()
            else:
                route.continue_()
        page.route("**/*", intercept_route)

        # 1. DeltaForceTools.gg
        try:
            dftools_builds = scrape_dftools(page, seen_codes)
            if not dftools_builds and existing_builds_by_source["DFTools"]:
                print(f"[!] CẢNH BÁO: DFTools trả về 0 bản build (có thể bị chặn IP hoặc đổi giao diện).")
                print(f"    -> Đang giữ lại {len(existing_builds_by_source['DFTools'])} bản build DFTools cũ.")
                for b in existing_builds_by_source["DFTools"]:
                    code = b["loadout_code"]
                    if code not in seen_codes:
                        seen_codes.add(code)
                        all_builds.append(b)
                source_stats["DFTools"] = len(existing_builds_by_source["DFTools"])
            else:
                all_builds.extend(dftools_builds)
                source_stats["DFTools"] = len(dftools_builds)
        except Exception as e:
            print(f"[!] Lỗi nghiêm trọng khi cào DFTools: {e}")
            if existing_builds_by_source["DFTools"]:
                print(f"    -> Đang giữ lại {len(existing_builds_by_source['DFTools'])} bản build DFTools cũ.")
                for b in existing_builds_by_source["DFTools"]:
                    code = b["loadout_code"]
                    if code not in seen_codes:
                        seen_codes.add(code)
                        all_builds.append(b)
                source_stats["DFTools"] = len(existing_builds_by_source["DFTools"])

        # 2. DeltaForceBuilds.com
        try:
            dfbuilds_builds = scrape_dfbuilds(page, seen_codes)
            if not dfbuilds_builds and existing_builds_by_source["DFBuilds"]:
                print(f"[!] CẢNH BÁO: DFBuilds trả về 0 bản build (có thể bị chặn IP hoặc đổi giao diện).")
                print(f"    -> Đang giữ lại {len(existing_builds_by_source['DFBuilds'])} bản build DFBuilds cũ.")
                for b in existing_builds_by_source["DFBuilds"]:
                    code = b["loadout_code"]
                    if code not in seen_codes:
                        seen_codes.add(code)
                        all_builds.append(b)
                source_stats["DFBuilds"] = len(existing_builds_by_source["DFBuilds"])
            else:
                all_builds.extend(dfbuilds_builds)
                source_stats["DFBuilds"] = len(dfbuilds_builds)
        except Exception as e:
            print(f"[!] Lỗi nghiêm trọng khi cào DFBuilds: {e}")
            if existing_builds_by_source["DFBuilds"]:
                print(f"    -> Đang giữ lại {len(existing_builds_by_source['DFBuilds'])} bản build DFBuilds cũ.")
                for b in existing_builds_by_source["DFBuilds"]:
                    code = b["loadout_code"]
                    if code not in seen_codes:
                        seen_codes.add(code)
                        all_builds.append(b)
                source_stats["DFBuilds"] = len(existing_builds_by_source["DFBuilds"])

        # 3. FPSMeta.gg
        try:
            fpsmeta_builds = scrape_fpsmeta(page, seen_codes)
            if not fpsmeta_builds and existing_builds_by_source["FPSMeta"]:
                print(f"[!] CẢNH BÁO: FPSMeta trả về 0 bản build (có thể bị chặn IP hoặc đổi giao diện).")
                print(f"    -> Đang giữ lại {len(existing_builds_by_source['FPSMeta'])} bản build FPSMeta cũ.")
                for b in existing_builds_by_source["FPSMeta"]:
                    code = b["loadout_code"]
                    if code not in seen_codes:
                        seen_codes.add(code)
                        all_builds.append(b)
                source_stats["FPSMeta"] = len(existing_builds_by_source["FPSMeta"])
            else:
                all_builds.extend(fpsmeta_builds)
                source_stats["FPSMeta"] = len(fpsmeta_builds)
        except Exception as e:
            print(f"[!] Lỗi nghiêm trọng khi cào FPSMeta: {e}")
            if existing_builds_by_source["FPSMeta"]:
                print(f"    -> Đang giữ lại {len(existing_builds_by_source['FPSMeta'])} bản build FPSMeta cũ.")
                for b in existing_builds_by_source["FPSMeta"]:
                    code = b["loadout_code"]
                    if code not in seen_codes:
                        seen_codes.add(code)
                        all_builds.append(b)
                source_stats["FPSMeta"] = len(existing_builds_by_source["FPSMeta"])

        browser.close()

    total_time = time.time() - t_start

    # Thống kê chi tiết
    by_server = {}
    by_mode = {}
    by_type = {}
    unique_weapons = set()

    for b in all_builds:
        by_server[b["server"]] = by_server.get(b["server"], 0) + 1
        by_mode[b["mode"]] = by_mode.get(b["mode"], 0) + 1
        by_type[b["weapon_type"]] = by_type.get(b["weapon_type"], 0) + 1
        unique_weapons.add(b["weapon"])

    print("\n" + "="*60)
    print("BÁO CÁO TỔNG KẾT THU THẬP & GỘP DỮ LIỆU ĐA NGUỒN")
    print(f"Thời gian thực hiện: {total_time:.1f} giây")
    print("="*60)
    print(f"Theo Nguồn Thu Thập:")
    print(f"  - DeltaForceTools.gg : {source_stats['DFTools']:4d} cấu hình")
    print(f"  - DeltaForceBuilds.com: {source_stats['DFBuilds']:4d} cấu hình")
    print(f"  - FPSMeta.gg          : {source_stats['FPSMeta']:4d} cấu hình")
    print(f"  -------------------------------------------")
    print(f"  => TỔNG CỘNG DUY NHẤT: {len(all_builds)} cấu hình (Đã lọc trùng 100% qua loadout_code)")
    print(f"  => Số loại vũ khí    : {len(unique_weapons)} vũ khí")

    print("\nTheo Khu Vực Máy Chủ (Server):")
    for s, c in sorted(by_server.items()):
        print(f"  - {s:10}: {c} cấu hình")

    print("\nTheo Chế Độ Chơi (Mode):")
    for m, c in sorted(by_mode.items()):
        print(f"  - {m:10}: {c} cấu hình")

    print("\nTheo Phân Loại Vũ Khí (Weapon Type):")
    for t, c in sorted(by_type.items(), key=lambda x: x[1], reverse=True):
        print(f"  - {t:15}: {c} cấu hình")

    # Lọc và kiểm tra tính hợp lệ của dữ liệu trước khi ghi
    all_builds = validate_builds(all_builds)

    # Chốt chặn bảo vệ an toàn dữ liệu: Nếu thu thập được ít hơn 500 cấu hình, hủy ghi đè file
    MIN_REQUIRED_BUILDS = 500
    if len(all_builds) < MIN_REQUIRED_BUILDS:
        print(f"\n[!] CẢNH BÁO BẢO VỆ DỮ LIỆU: Chỉ thu thập được {len(all_builds)} cấu hình (< {MIN_REQUIRED_BUILDS})!")
        print("[!] Đã hủy ghi đè file builds.json để bảo vệ dữ liệu hiện có trên GitHub.")
        sys.exit(1)

    # Lưu file builds.json
    with open(OUTPUT_FILE, "w", encoding="utf-8") as f:
        json.dump(all_builds, f, ensure_ascii=False, indent=2)

    print(f"\n[✓] Đã xuất thành công toàn bộ {len(all_builds)} bản build vào '{OUTPUT_FILE}'.")

if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1].lower() in ("--clean", "--clean-only", "--validate", "--validate-only", "clean", "validate"):
        clean_existing_builds()
    else:
        run_fetch()
