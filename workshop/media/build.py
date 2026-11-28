#!/usr/bin/env python3
"""Build Steam Workshop cover and showcase images from clean raw assets.

Outputs generated artifacts into `target/workshop-media/` so only clean source
assets and this script are tracked in Git.
"""

from __future__ import annotations

from pathlib import Path
import tomllib
from PIL import Image, ImageDraw, ImageFilter, ImageFont

ROOT = Path(__file__).resolve().parents[2]
RAW = ROOT / "workshop" / "media" / "raw"
FONTS = ROOT / "fonts"
OUT = ROOT / "target" / "workshop-media"


def load_workspace_release() -> tuple[str, str | None]:
    data = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
    version = str(data["workspace"]["package"]["version"])
    codename = data.get("workspace", {}).get("metadata", {}).get("harmony", {}).get("codename")
    if isinstance(codename, str) and codename.strip():
        return version, codename.strip()
    return version, None


def load_workshop_ids() -> tuple[str, str, str]:
    data = tomllib.loads((ROOT / "workshop" / "item.toml").read_text(encoding="utf-8"))
    public_id = int(data.get("published_file_id", 0))
    beta_id = int(data.get("beta_published_file_id", 0))
    active = public_id if public_id != 0 else beta_id
    if active == 0:
        raise ValueError("workshop/item.toml has neither published_file_id nor beta_published_file_id set")
    featured = data.get("featured_mods", {})
    ct_id = int(featured.get("convenience_tools_id", 3838535796))
    rc_id = int(featured.get("regional_combat_id", 3837793510))
    return str(active), str(ct_id), str(rc_id)


VERSION, CODENAME = load_workspace_release()
ACTIVE_WORKSHOP_ID, CONVENIENCE_TOOLS_ID, REGIONAL_COMBAT_ID = load_workshop_ids()

# Palette aligned with the teal-gold parchment cover and Harmony's dark UI.
BG_TOP = (13, 21, 32)
BG_BOT = (21, 33, 48)
CARD_BG = (25, 38, 54)
CARD_BG_ALT = (20, 31, 45)
CARD_BORDER = (52, 74, 98)
TEAL = (68, 190, 174)
TEAL_DARK = (24, 72, 68)
GOLD = (218, 179, 102)
GOLD_SOFT = (182, 149, 86)
TEXT_PRIMARY = (244, 238, 227)
TEXT_MUTED = (158, 177, 196)
MUTED_BAR = (86, 102, 120)
BEFORE_TAG = (178, 86, 78)


def load_font(candidates: list[Path | str], size: int) -> ImageFont.FreeTypeFont:
    win_fonts = Path("C:/Windows/Fonts")
    for item in candidates:
        p = Path(item)
        if not p.is_absolute():
            p = win_fonts / p
        if p.is_file():
            return ImageFont.truetype(str(p), size)
    raise FileNotFoundError(f"None of these fonts exist: {candidates}")


def font_serif(size: int, bold: bool = False, italic: bool = False) -> ImageFont.FreeTypeFont:
    if bold and italic:
        names = ["georgiaz.ttf", "georgiab.ttf", "timesbi.ttf"]
    elif bold:
        names = ["georgiab.ttf", "timesbd.ttf", "segoeuib.ttf"]
    elif italic:
        names = ["georgiai.ttf", "timesi.ttf", "segoeuii.ttf"]
    else:
        names = ["georgia.ttf", "times.ttf", "segoeui.ttf"]
    return load_font(names, size)


def font_sans(size: int, weight: str = "regular") -> ImageFont.FreeTypeFont:
    if weight == "bold":
        names = ["segoeuib.ttf", "arialbd.ttf"]
    elif weight == "semibold":
        names = ["seguisb.ttf", "segoeuib.ttf", "arialbd.ttf"]
    else:
        names = ["segoeui.ttf", "arial.ttf"]
    return load_font(names, size)


def font_mono(size: int, bold: bool = False) -> ImageFont.FreeTypeFont:
    names = ["consolab.ttf", "consola.ttf"] if bold else ["consola.ttf", "lucon.ttf"]
    return load_font(names, size)


def make_gradient_canvas(w: int, h: int) -> Image.Image:
    """Smooth vertical dark-slate gradient with subtle teal/gold corner glow."""
    img = Image.new("RGB", (w, h), BG_TOP)
    px = img.load()
    for y in range(h):
        t = y / max(1, h - 1)
        r = int(BG_TOP[0] * (1 - t) + BG_BOT[0] * t)
        g = int(BG_TOP[1] * (1 - t) + BG_BOT[1] * t)
        b = int(BG_TOP[2] * (1 - t) + BG_BOT[2] * t)
        for x in range(w):
            px[x, y] = (r, g, b)

    overlay = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    od = ImageDraw.Draw(overlay)
    # Soft ambient top-right teal halo and bottom-left gold halo
    od.ellipse((w - 700, -320, w + 260, 560), fill=(68, 190, 174, 24))
    od.ellipse((-300, h - 520, 620, h + 300), fill=(218, 179, 102, 18))
    overlay = overlay.filter(ImageFilter.GaussianBlur(radius=140))
    return Image.alpha_composite(img.convert("RGBA"), overlay).convert("RGB")


def draw_header(draw: ImageDraw.ImageDraw, title: str, subtitle: str, badge: str = "HARMONY  ·  EU4 1.37.5") -> None:
    # Subtle top accent bar
    draw.rectangle((0, 0, 1920, 6), fill=TEAL)
    draw.rectangle((0, 0, 620, 6), fill=GOLD)

    # Small uppercase kicker badge on the right
    bf = font_sans(19, "semibold")
    bw = draw.textlength(badge, font=bf)
    bx = 1920 - 80 - int(bw) - 36
    draw.rounded_rectangle((bx, 46, 1920 - 80, 86), radius=20, fill=(24, 42, 58), outline=CARD_BORDER, width=2)
    draw.text((bx + 18, 54), badge, font=bf, fill=GOLD)

    # Main title & subtitle
    draw.text((80, 38), title, font=font_serif(46, bold=True), fill=TEXT_PRIMARY)
    draw.text((80, 100), subtitle, font=font_sans(24), fill=TEXT_MUTED)
    draw.line((80, 144, 1840, 144), fill=(44, 62, 84), width=2)


def draw_pill(
    draw: ImageDraw.ImageDraw,
    x: int,
    y: int,
    text: str,
    bg: tuple[int, int, int],
    fg: tuple[int, int, int] = TEXT_PRIMARY,
    outline: tuple[int, int, int] | None = None,
    size: int = 20,
    pad_x: int = 18,
    pad_y: int = 8,
) -> tuple[int, int]:
    f = font_sans(size, "semibold")
    bbox = draw.textbbox((0, 0), text, font=f)
    tw, th = bbox[2] - bbox[0], bbox[3] - bbox[1]
    w, h = tw + pad_x * 2, th + pad_y * 2 + 2
    draw.rounded_rectangle((x, y, x + w, y + h), radius=h // 2, fill=bg, outline=outline, width=2 if outline else 0)
    draw.text((x + pad_x, y + pad_y - bbox[1]), text, font=f, fill=fg)
    return w, h


def wrap_text(draw: ImageDraw.ImageDraw, text: str, f: ImageFont.FreeTypeFont, max_w: int) -> list[str]:
    words = text.split()
    lines: list[str] = []
    cur = ""
    for word in words:
        trial = f"{cur} {word}".strip()
        if draw.textlength(trial, font=f) <= max_w:
            cur = trial
        else:
            if cur:
                lines.append(cur)
            cur = word
    if cur:
        lines.append(cur)
    return lines


def paste_rounded(base: Image.Image, patch: Image.Image, box: tuple[int, int], radius: int = 14) -> None:
    mask = Image.new("L", patch.size, 0)
    ImageDraw.Draw(mask).rounded_rectangle((0, 0, patch.width, patch.height), radius=radius, fill=255)
    base.paste(patch, box, mask)


def draw_check(d: ImageDraw.ImageDraw, x: int, y: int, size: int, color: tuple[int, int, int], width: int = 3) -> None:
    """Draw a crisp vector checkmark (avoids missing-glyph boxes in fonts lacking U+2713)."""
    p1 = (x, y + int(size * 0.55))
    p2 = (x + int(size * 0.38), y + int(size * 0.90))
    p3 = (x + size, y + int(size * 0.15))
    d.line([p1, p2, p3], fill=color, width=width, joint="curve")


def build_thumbnail() -> Path:
    """Compose the public Workshop cover from the clean text-free watercolor base."""
    return _compose_english_thumbnail(OUT / "thumbnail.png", beta=False)


def build_thumbnail_beta() -> Path:
    """Compose the beta Workshop cover with a clear BETA badge from the text-free base."""
    return _compose_english_thumbnail(OUT / "thumbnail-beta.png", beta=True)


def _compose_english_thumbnail(out: Path, *, beta: bool) -> Path:
    base = Image.open(RAW / "thumbnail-base.png").convert("RGBA")
    w, h = base.size  # 1254x1254 native artwork; compose at native resolution then downsample to 1024x1024
    s = w / 1254.0

    # Garamond wordmark + soft warm halo matching the original Harmony branding
    title_font = load_font(["GARA.TTF", "pala.ttf", "georgia.ttf"], int(230 * s))
    sub_font = load_font(["GARAIT.TTF", "palai.ttf", "georgiai.ttf"], int(52 * s))

    def draw_wordmark(d: ImageDraw.ImageDraw, title_fill: tuple[int, int, int, int], sub_fill: tuple[int, int, int, int]) -> None:
        tb = d.textbbox((0, 0), "Harmony", font=title_font)
        tx = w / 2 - (tb[2] - tb[0]) / 2 - tb[0]
        ty = 236 * s - tb[1]
        d.text((tx, ty), "Harmony", font=title_font, fill=title_fill)

        sub = "for Europa Universalis IV 1.37.5"
        sb = d.textbbox((0, 0), sub, font=sub_font)
        sx = w / 2 - (sb[2] - sb[0]) / 2 - sb[0]
        sy = 454 * s - sb[1]
        d.text((sx, sy), sub, font=sub_font, fill=sub_fill)

    halo = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    draw_wordmark(ImageDraw.Draw(halo), (255, 250, 235, 210), (255, 250, 235, 200))
    halo = halo.filter(ImageFilter.GaussianBlur(radius=10))
    composed = Image.alpha_composite(base, halo)

    layer = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    d = ImageDraw.Draw(layer)
    draw_wordmark(d, (24, 84, 80, 255), (42, 96, 90, 245))

    if beta:
        bf = font_sans(int(36 * s), "bold")
        btxt = "BETA CHANNEL"
        bb = d.textbbox((0, 0), btxt, font=bf)
        bw, bh = bb[2] - bb[0], bb[3] - bb[1]
        bpw, bph = bw + int(60 * s), int(62 * s)
        bx0, by0 = (w - bpw) // 2, int(112 * s)
        d.rounded_rectangle((bx0, by0, bx0 + bpw, by0 + bph), radius=bph // 2, fill=(168, 92, 24, 238), outline=(238, 198, 122, 240), width=max(3, int(3 * s)))
        d.text((bx0 + (bpw - bw) // 2, by0 + (bph - bh) // 2 - bb[1] - 1), btxt, font=bf, fill=(255, 248, 236, 255))

    # Crisp gold divider lines flanking a small diamond
    cx, dy = int(w / 2), int(536 * s)
    d.line((cx - int(290 * s), dy, cx - int(22 * s), dy), fill=(198, 162, 92, 210), width=max(2, int(2.5 * s)))
    d.line((cx + int(22 * s), dy, cx + int(290 * s), dy), fill=(198, 162, 92, 210), width=max(2, int(2.5 * s)))
    r = int(9 * s)
    d.polygon([(cx, dy - r), (cx + r, dy), (cx, dy + r), (cx - r, dy)], fill=(198, 162, 92, 235))

    # High-contrast dark-teal rounded pill for the 3 headline hooks (readable even at 200px)
    hook = "HD CJK Fonts   ·   No-Restart   ·   Faster Ticks"
    hf = font_sans(int(38 * s), "semibold")
    hb = d.textbbox((0, 0), hook, font=hf)
    hw, hh = hb[2] - hb[0], hb[3] - hb[1]
    pill_w = hw + int(82 * s)
    pill_h = int(84 * s)
    px0 = (w - pill_w) // 2
    py0 = int(572 * s)
    d.rounded_rectangle(
        (px0, py0, px0 + pill_w, py0 + pill_h),
        radius=pill_h // 2,
        fill=(21, 66, 63, 234),
        outline=(206, 171, 102, 232),
        width=max(3, int(3.5 * s)),
    )
    d.text((px0 + (pill_w - hw) // 2, py0 + (pill_h - hh) // 2 - hb[1] - 1), hook, font=hf, fill=(249, 244, 233, 255))

    composed = Image.alpha_composite(composed, layer).convert("RGB").resize((1024, 1024), Image.Resampling.LANCZOS)
    # Indexed-PNG compression keeps the 1024x1024 cover well under Steam's 1 MiB cap.
    quantized = composed.quantize(colors=256, method=Image.Quantize.MEDIANCUT, dither=Image.Dither.FLOYDSTEINBERG)
    quantized.save(out, optimize=True)
    assert out.stat().st_size < 1024 * 1024, f"{out.name} exceeds 1 MiB: {out.stat().st_size}"
    return out


def build_thumbnail_zh() -> Path:
    """Compose the Simplified Chinese promotional cover ("和鸣") from the text-free base."""
    base = Image.open(RAW / "thumbnail-base.png").convert("RGBA")
    w, h = base.size
    s = w / 1254.0

    zh_title_font = load_font(["STFANGSO.TTF", "simfang.ttf", "SourceHanSerifCN-Light.otf", FONTS / "simp_chinese_default_body.ttf"], int(224 * s))
    en_sub_font = load_font(["GARA.TTF", "pala.ttf", "georgia.ttf"], int(58 * s))
    zh_sub_font = load_font(["STFANGSO.TTF", "simfang.ttf", FONTS / "simp_chinese_default_body.ttf"], int(42 * s))

    def draw_wordmark(d: ImageDraw.ImageDraw, title_fill: tuple[int, int, int, int], sub_fill: tuple[int, int, int, int]) -> None:
        title = "和 鸣"
        tb = d.textbbox((0, 0), title, font=zh_title_font)
        tx = w / 2 - (tb[2] - tb[0]) / 2 - tb[0]
        ty = 178 * s - tb[1]
        d.text((tx, ty), title, font=zh_title_font, fill=title_fill)

        en = "Harmony"
        eb = d.textbbox((0, 0), en, font=en_sub_font)
        ex = w / 2 - (eb[2] - eb[0]) / 2 - eb[0]
        ey = 406 * s - eb[1]
        d.text((ex, ey), en, font=en_sub_font, fill=sub_fill)

        sub = "欧陆风云 IV 1.37.5 底层增强与扩展框架"
        sb = d.textbbox((0, 0), sub, font=zh_sub_font)
        sx = w / 2 - (sb[2] - sb[0]) / 2 - sb[0]
        sy = 476 * s - sb[1]
        d.text((sx, sy), sub, font=zh_sub_font, fill=sub_fill)

    halo = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    draw_wordmark(ImageDraw.Draw(halo), (255, 250, 235, 215), (255, 250, 235, 200))
    halo = halo.filter(ImageFilter.GaussianBlur(radius=10))
    composed = Image.alpha_composite(base, halo)

    layer = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    d = ImageDraw.Draw(layer)
    draw_wordmark(d, (24, 84, 80, 255), (42, 96, 90, 245))

    cx, dy = int(w / 2), int(544 * s)
    d.line((cx - int(290 * s), dy, cx - int(22 * s), dy), fill=(198, 162, 92, 210), width=max(2, int(2.5 * s)))
    d.line((cx + int(22 * s), dy, cx + int(290 * s), dy), fill=(198, 162, 92, 210), width=max(2, int(2.5 * s)))
    r = int(9 * s)
    d.polygon([(cx, dy - r), (cx + r, dy), (cx, dy + r), (cx - r, dy)], fill=(198, 162, 92, 235))

    hook = "高清字体   ·   免重启回主菜单   ·   性能优化"
    hf = load_font([FONTS / "simp_chinese_default_heading.ttf", "msyhbd.ttc"], int(34 * s))
    hb = d.textbbox((0, 0), hook, font=hf)
    hw, hh = hb[2] - hb[0], hb[3] - hb[1]
    pill_w = hw + int(80 * s)
    pill_h = int(84 * s)
    px0 = (w - pill_w) // 2
    py0 = int(576 * s)
    d.rounded_rectangle(
        (px0, py0, px0 + pill_w, py0 + pill_h),
        radius=pill_h // 2,
        fill=(21, 66, 63, 234),
        outline=(206, 171, 102, 232),
        width=max(3, int(3.5 * s)),
    )
    d.text((px0 + (pill_w - hw) // 2, py0 + (pill_h - hh) // 2 - hb[1] - 1), hook, font=hf, fill=(249, 244, 233, 255))

    composed = Image.alpha_composite(composed, layer).convert("RGB").resize((1024, 1024), Image.Resampling.LANCZOS)
    out = OUT / "thumbnail-zh.png"
    quantized = composed.quantize(colors=256, method=Image.Quantize.MEDIANCUT, dither=Image.Dither.FLOYDSTEINBERG)
    quantized.save(out, optimize=True)
    assert out.stat().st_size < 1024 * 1024, f"thumbnail-zh.png exceeds 1 MiB: {out.stat().st_size}"
    return out


def build_banner_zh() -> Path:
    """Compose the Simplified Chinese 1600px README banner ("和鸣 · Harmony") from banner-base.png."""
    im = Image.open(RAW / "banner-base.png").convert("RGBA")
    target_w = 1600
    im = im.resize((target_w, round(im.height * target_w / im.width)), Image.Resampling.LANCZOS)
    w, h = im.size

    zh_font = load_font(["STFANGSO.TTF", "simfang.ttf", "SourceHanSerifCN-Light.otf", FONTS / "simp_chinese_default_body.ttf"], 172)
    en_font = load_font(["GARA.TTF", "pala.ttf", "georgia.ttf"], 62)
    sub_font = load_font(["STFANGSO.TTF", "simfang.ttf", FONTS / "simp_chinese_default_body.ttf"], 36)
    cx = int(w * 0.382)

    def draw_text(d: ImageDraw.ImageDraw, main_fill: tuple[int, int, int, int], sub_fill: tuple[int, int, int, int]) -> None:
        zh = "和 鸣"
        zb = d.textbbox((0, 0), zh, font=zh_font)
        zw = zb[2] - zb[0]
        zx = cx - zw / 2 - zb[0]
        zy = int(h * 0.08) - zb[1]
        d.text((zx, zy), zh, font=zh_font, fill=main_fill)

        en = "Harmony"
        eb = d.textbbox((0, 0), en, font=en_font)
        ex = cx - (eb[2] - eb[0]) / 2 - eb[0]
        ey = int(h * 0.40) - eb[1]
        d.text((ex, ey), en, font=en_font, fill=sub_fill)

        sub = "欧陆风云 IV 1.37.5"
        sb = d.textbbox((0, 0), sub, font=sub_font)
        sx = cx - (sb[2] - sb[0]) / 2 - sb[0]
        sy = int(h * 0.56) - sb[1]
        d.text((sx, sy), sub, font=sub_font, fill=sub_fill)

    halo = Image.new("RGBA", im.size, (0, 0, 0, 0))
    draw_text(ImageDraw.Draw(halo), (255, 250, 235, 210), (255, 250, 235, 200))
    im.alpha_composite(halo.filter(ImageFilter.GaussianBlur(radius=9)))
    draw_text(ImageDraw.Draw(im), (24, 84, 80, 255), (56, 102, 96, 255))

    out = OUT / "banner.zh-CN.jpg"
    q = 90
    while True:
        im.convert("RGB").save(out, quality=q, optimize=True, progressive=True)
        if out.stat().st_size < 500_000 or q <= 65:
            break
        q -= 5
    return out


def build_01_cjk_fonts() -> Path:
    """1920x1080 real Before/After 1:1 native-pixel comparison + bundled CJK defaults strip."""
    img = make_gradient_canvas(1920, 1080)
    d = ImageDraw.Draw(img)
    draw_header(
        d,
        "CRISP NATIVE FONTS & MAP OUTLINES",
        "Native UTF-8 rendering with sharp system fonts & free bundled SC / JP / KR defaults — zero blurry bitmap mods",
    )

    before = Image.open(RAW / "capture-map-before-zh.png").convert("RGB")
    after = Image.open(RAW / "capture-map-harmony-zh.png").convert("RGB")

    # Raw captures are 1600x1120 centered on the Ming event popup + Liaodong/Bohai/Korea map names.
    # Take an identical 800x540 native-pixel region showing the full event popup + dark-background map names.
    crop_box = (220, 175, 1020, 715)
    left_crop = before.crop(crop_box)
    right_crop = after.crop(crop_box)

    frame_w, frame_h = 860, 580
    top_y = 176
    lx, rx = 80, 980

    for x0, crop, label, tag_bg, border in [
        (lx, left_crop, "BEFORE  ·  Legacy Bitmap Fonts", BEFORE_TAG, (118, 70, 68)),
        (rx, right_crop, "AFTER  ·  Harmony Native + Outlined Map", TEAL_DARK, TEAL),
    ]:
        d.rounded_rectangle((x0 - 4, top_y - 4, x0 + frame_w + 4, top_y + frame_h + 4), radius=18, fill=CARD_BG, outline=border, width=3)
        paste_rounded(img, crop.resize((frame_w, frame_h), Image.Resampling.LANCZOS), (x0, top_y), radius=14)
        # Top-left pill tag with dark scrim behind it
        draw_pill(d, x0 + 18, top_y + 18, label, bg=tag_bg, fg=TEXT_PRIMARY, outline=border, size=20, pad_x=16, pad_y=7)

    # Highlight arrow badge centered between left & right frames
    mid_x, mid_y = 960, top_y + frame_h // 2
    d.ellipse((mid_x - 32, mid_y - 32, mid_x + 32, mid_y + 32), fill=GOLD, outline=BG_TOP, width=4)
    af = font_sans(32, "bold")
    ab = d.textbbox((0, 0), "→", font=af)
    d.text((mid_x - (ab[2] - ab[0]) // 2, mid_y - (ab[3] - ab[1]) // 2 - ab[1] - 2), "→", font=af, fill=BG_TOP)

    # Bottom 3-column strip showcasing the real bundled free CJK fonts rendered directly from fonts/
    strip_y = 796
    strip_h = 228
    col_w = 560
    gap = 40
    samples = [
        (
            "SIMPLIFIED CHINESE",
            "Built-in Default Fonts",
            FONTS / "simp_chinese_default_body.ttf",
            FONTS / "simp_chinese_default_map.ttf",
            "大明 · 奥斯曼 · 法兰西",
            "江山如画，盛世长明。1444年11月11日",
        ),
        (
            "JAPANESE",
            "Source Han Serif JP (SIL OFL 1.1)",
            FONTS / "japanese_default_body.otf",
            FONTS / "japanese_default_heading.otf",
            "日本 · オスマン · フランス",
            "帝国の繁栄と均衡の時代。1444年11月11日",
        ),
        (
            "KOREAN",
            "Godo M / Godo B (Free License)",
            FONTS / "korean_default_body.ttf",
            FONTS / "korean_default_heading.ttf",
            "조선 · 오스만 · 프랑스",
            "제국의 번영과 균형의 시대. 1444년 11월 11일",
        ),
    ]
    for i, (lang_tag, font_note, body_fp, head_fp, map_line, body_line) in enumerate(samples):
        cx = 80 + i * (col_w + gap)
        d.rounded_rectangle((cx, strip_y, cx + col_w, strip_y + strip_h), radius=16, fill=CARD_BG, outline=CARD_BORDER, width=2)
        d.rectangle((cx + 24, strip_y + 22, cx + 28, strip_y + 52), fill=TEAL if i > 0 else GOLD)
        d.text((cx + 42, strip_y + 18), lang_tag, font=font_sans(18, "bold"), fill=GOLD if i == 0 else TEAL)
        d.text((cx + 42, strip_y + 42), font_note, font=font_sans(17), fill=TEXT_MUTED)
        d.line((cx + 24, strip_y + 78, cx + col_w - 24, strip_y + 78), fill=(42, 60, 80), width=1)
        f_head = load_font([head_fp], 32)
        f_body = load_font([body_fp], 23)
        d.text((cx + 28, strip_y + 98), map_line, font=f_head, fill=TEXT_PRIMARY)
        d.text((cx + 28, strip_y + 156), body_line, font=f_body, fill=(210, 222, 234))

    out = OUT / "01-cjk-fonts.png"
    img.save(out, optimize=True)
    return out


def build_02_settings_ui() -> Path:
    """1920x1080 real Harmony Settings window capture + key UI callouts."""
    img = make_gradient_canvas(1920, 1080)
    d = ImageDraw.Draw(img)
    draw_header(
        d,
        "MODERN IN-GAME SETTINGS & LIVE PREVIEW",
        "Self-drawn framework UI with real-time font preview, per-page reset, and transparent mod permissions",
    )

    shot = Image.open(RAW / "capture-settings-en.png").convert("RGB")
    # Place crisp capture on the left (1080x840 fits nicely in 1080x840 -> scale gently to 1054x820)
    sw, sh = 1054, 820
    sx, sy = 80, 184
    d.rounded_rectangle((sx - 4, sy - 4, sx + sw + 4, sy + sh + 4), radius=20, fill=CARD_BG, outline=GOLD_SOFT, width=3)
    paste_rounded(img, shot.resize((sw, sh), Image.Resampling.LANCZOS), (sx, sy), radius=16)
    draw_pill(d, sx + 22, sy + 20, "LIVE CAPTURE  ·  Harmony Settings", bg=(20, 34, 48), fg=GOLD, outline=GOLD_SOFT, size=18, pad_x=14, pad_y=6)

    # Right column: 4 clean highlight cards
    rx = 1176
    rw = 664
    card_h = 186
    gap = 25
    cards = [
        ("Live Font & Weight Preview", "Pick any Windows font or built-in CJK default for body, headings, map names, and console — see instant live samples before saving.", TEAL),
        ("Change Settings Mid-Campaign", "Adjust game speeds, FPS cap, map zoom, ironman auto-save, and audio loudness right from the in-game Esc menu.", GOLD),
        ("Transparent Mod Permissions", "Inspect every Harmony mod in the Mods window to see whether it requests real clock, local files, or network access.", TEAL),
        ("Opt-In Remote & Private Diagnostics", "Remote control stays off by default under Advanced. One-click troubleshooting zip redacts username paths and slims crash dumps.", GOLD),
    ]
    for i, (heading, body, accent) in enumerate(cards):
        cy = sy + i * (card_h + gap)
        d.rounded_rectangle((rx, cy, rx + rw, cy + card_h), radius=16, fill=CARD_BG, outline=CARD_BORDER, width=2)
        d.rounded_rectangle((rx, cy + 20, rx + 7, cy + card_h - 20), radius=3, fill=accent)
        d.text((rx + 30, cy + 22), heading, font=font_sans(25, "bold"), fill=TEXT_PRIMARY)
        bf = font_sans(20)
        lines = wrap_text(d, body, bf, rw - 56)
        ly = cy + 66
        for line in lines[:3]:
            d.text((rx + 30, ly), line, font=bf, fill=TEXT_MUTED)
            ly += 31

    out = OUT / "02-settings-ui.png"
    img.save(out, optimize=True)
    return out


def build_03_performance() -> Path:
    """1920x1080 measured benchmark comparison chart from docs/performance.md."""
    img = make_gradient_canvas(1920, 1080)
    d = ImageDraw.Draw(img)
    draw_header(
        d,
        "MEASURED PERFORMANCE GAINS",
        "Shorter save freezes, smoother speed-5 frames, faster large-mod startup — with vanilla game rules intact",
    )

    # Left column: 3 horizontal Before/After bar cards
    lx, lw = 80, 1180
    top_y = 186
    card_h = 228
    gap = 28

    metrics = [
        (
            "Ironman Local Save Pause (55 MB save)",
            "Multi-threaded province & country serialization cuts the freeze to ~1/3",
            "Vanilla EU4",
            "~1.20 s",
            1.0,
            "Harmony",
            "~0.35 s",
            0.29,
            "-70%",
            "shorter freeze",
            GOLD,
        ),
        (
            "Speed-5 Frame Submission Stutter (P99)",
            "Tighter presentation lock scopes keep map panning responsive at speed 5",
            "Before",
            "44.1 ms",
            1.0,
            "Harmony",
            "13.2 ms",
            0.30,
            "-70%",
            "lower P99 lag",
            TEAL,
        ),
        (
            "Large-Mod Startup Time (333 loc replace files)",
            "Fast localisation & parameterized script passes cut startup wait by ~1/3",
            "Vanilla EU4",
            "31.1 s",
            1.0,
            "Harmony",
            "20.2 s",
            0.65,
            "-35%",
            "faster startup",
            TEAL,
        ),
    ]

    for i, (title, desc, b_lbl, b_val, b_ratio, a_lbl, a_val, a_ratio, stat, stat_sub, accent) in enumerate(metrics):
        cy = top_y + i * (card_h + gap)
        d.rounded_rectangle((lx, cy, lx + lw, cy + card_h), radius=18, fill=CARD_BG, outline=CARD_BORDER, width=2)
        d.text((lx + 32, cy + 22), title, font=font_sans(26, "bold"), fill=TEXT_PRIMARY)
        d.text((lx + 32, cy + 58), desc, font=font_sans(19), fill=TEXT_MUTED)

        bar_x = lx + 178
        bar_max_w = 650
        bar_h = 30

        # Before bar
        y1 = cy + 108
        d.text((lx + 32, y1 + 3), b_lbl, font=font_sans(20, "semibold"), fill=TEXT_MUTED)
        d.rounded_rectangle((bar_x, y1, bar_x + int(bar_max_w * b_ratio), y1 + bar_h), radius=8, fill=MUTED_BAR)
        d.text((bar_x + int(bar_max_w * b_ratio) + 16, y1 + 2), b_val, font=font_sans(21, "semibold"), fill=TEXT_MUTED)

        # After bar
        y2 = cy + 158
        d.text((lx + 32, y2 + 3), a_lbl, font=font_sans(20, "bold"), fill=accent)
        d.rounded_rectangle((bar_x, y2, bar_x + int(bar_max_w * a_ratio), y2 + bar_h), radius=8, fill=accent)
        d.text((bar_x + int(bar_max_w * a_ratio) + 16, y2 + 2), a_val, font=font_sans(22, "bold"), fill=TEXT_PRIMARY)

        # Right-side callout badge
        sx0 = lx + lw - 216
        d.rounded_rectangle((sx0, cy + 36, lx + lw - 28, cy + card_h - 36), radius=14, fill=CARD_BG_ALT, outline=accent, width=2)
        sf = font_serif(52, bold=True)
        sb = d.textbbox((0, 0), stat, font=sf)
        d.text((sx0 + (188 - (sb[2] - sb[0])) // 2, cy + 62), stat, font=sf, fill=accent)
        ssf = font_sans(18, "semibold")
        ssb = d.textbbox((0, 0), stat_sub, font=ssf)
        d.text((sx0 + (188 - (ssb[2] - ssb[0])) // 2, cy + 138), stat_sub, font=ssf, fill=TEXT_MUTED)

    # Right hero column: No-Restart Return + Memory savings
    rx, rw = 1300, 540
    # Card 1: No-Restart Return
    d.rounded_rectangle((rx, top_y, rx + rw, top_y + 420), radius=18, fill=(22, 46, 56), outline=GOLD_SOFT, width=3)
    draw_pill(d, rx + 32, top_y + 30, "INSTANT MAIN MENU", bg=TEAL_DARK, fg=GOLD, outline=GOLD_SOFT, size=18)
    d.text((rx + 32, top_y + 92), "No More Full", font=font_serif(44, bold=True), fill=TEXT_PRIMARY)
    d.text((rx + 32, top_y + 146), "Game Restarts", font=font_serif(44, bold=True), fill=GOLD)
    d.line((rx + 32, top_y + 216, rx + rw - 32, top_y + 216), fill=(62, 92, 108), width=2)
    d.text((rx + 32, top_y + 240), "Vanilla EU4", font=font_sans(20, "semibold"), fill=TEXT_MUTED)
    d.text((rx + 32, top_y + 268), "Closes & relaunches eu4.exe (~20–30 s)", font=font_sans(22), fill=(205, 145, 138))
    d.text((rx + 32, top_y + 322), "With Harmony", font=font_sans(20, "bold"), fill=TEAL)
    d.text((rx + 32, top_y + 350), "In-process reset in ~2 seconds", font=font_sans(25, "bold"), fill=TEXT_PRIMARY)

    # Card 2: Memory & Rule integrity
    m_y = top_y + 448
    m_h = 292
    d.rounded_rectangle((rx, m_y, rx + rw, m_y + m_h), radius=18, fill=CARD_BG, outline=CARD_BORDER, width=2)
    d.text((rx + 32, m_y + 24), "AND LESS MEMORY OVERHEAD", font=font_sans(19, "bold"), fill=TEAL)
    d.text((rx + 32, m_y + 56), "~410 MB", font=font_serif(50, bold=True), fill=TEXT_PRIMARY)
    mf = font_sans(20)
    for idx, mline in enumerate(wrap_text(d, "less RAM with CJK system fonts vs. legacy bitmap font atlases.", mf, rw - 64)):
        d.text((rx + 32, m_y + 118 + idx * 28), mline, font=mf, fill=TEXT_MUTED)
    d.line((rx + 32, m_y + 188, rx + rw - 32, m_y + 188), fill=(48, 66, 86), width=1)
    draw_check(d, rx + 34, m_y + 210, 18, GOLD, width=3)
    d.text((rx + 64, m_y + 206), "100% vanilla simulation answers preserved", font=font_sans(20, "semibold"), fill=GOLD)
    draw_check(d, rx + 34, m_y + 246, 18, TEAL, width=3)
    d.text((rx + 64, m_y + 242), "Dedicated drawing thread + smart FPS cap", font=font_sans(20, "semibold"), fill=TEXT_PRIMARY)

    # Rigorous benchmark footnote
    foot = (
        "Measured on Steam Windows EU4 1.37.5 (Ryzen 7 9700X; 55 MB Ironman save & 333-file large-mod benchmarks in docs/performance.md). "
        "Actual gains vary by hardware and playset."
    )
    d.text((80, 996), foot, font=font_sans(17), fill=(124, 142, 162))

    out = OUT / "03-performance.png"
    img.save(out, optimize=True)
    return out


def build_04_qol_features() -> Path:
    """1920x1080 2x2 QoL grid + Featured Harmony Mods ecosystem strip."""
    img = make_gradient_canvas(1920, 1080)
    d = ImageDraw.Draw(img)
    draw_header(
        d,
        "QUALITY OF LIFE & MOD ECOSYSTEM",
        "Engine fixes, audio & camera polish, achievement options, and Lua-powered mods beyond Paradox scripts",
    )

    # Top 2x2 QoL grid
    gx, gy = 80, 184
    cw, ch = 860, 224
    gap_x, gap_y = 40, 28
    qol = [
        (
            "01",
            "Return Without Restart & Ironman Loading",
            "Leave a campaign back to the main menu in seconds without restarting EU4. Load another save directly inside Ironman, with optional automatic backup save before switching.",
            TEAL,
        ),
        (
            "02",
            "Engine Bug & Memory Leak Repairs",
            "Fixes the duplicate heir-name overflow, frees session memory leaked across world resets, and prevents stale reformation-center crashes — keeping vanilla balance untouched.",
            GOLD,
        ),
        (
            "03",
            "Balanced Audio, Custom Speeds & Smooth Zoom",
            "Play every music track at consistent loudness, enjoy natural quadratic volume sliders, customize game speeds 1–5, cap menu FPS, and zoom the map smoothly to your cursor.",
            GOLD,
        ),
        (
            "04",
            "Optional Relaxed Achievement Rules",
            "Toggle achievements outside Ironman or with any mod right from Harmony settings. Your existing translation mods and saves work as they are.",
            TEAL,
        ),
    ]
    for idx, (num, title, desc, accent) in enumerate(qol):
        col, row = idx % 2, idx // 2
        x0 = gx + col * (cw + gap_x)
        y0 = gy + row * (ch + gap_y)
        d.rounded_rectangle((x0, y0, x0 + cw, y0 + ch), radius=18, fill=CARD_BG, outline=CARD_BORDER, width=2)
        # Number circle badge
        d.rounded_rectangle((x0 + 28, y0 + 28, x0 + 88, y0 + 88), radius=14, fill=CARD_BG_ALT, outline=accent, width=2)
        nf = font_serif(28, bold=True)
        nb = d.textbbox((0, 0), num, font=nf)
        d.text((x0 + 58 - (nb[2] - nb[0]) // 2, y0 + 58 - (nb[3] - nb[1]) // 2 - nb[1] - 1), num, font=nf, fill=accent)

        d.text((x0 + 112, y0 + 36), title, font=font_sans(26, "bold"), fill=TEXT_PRIMARY)
        bf = font_sans(21)
        lines = wrap_text(d, desc, bf, cw - 144)
        ly = y0 + 84
        for line in lines[:4]:
            d.text((x0 + 112, ly), line, font=bf, fill=TEXT_MUTED)
            ly += 32

    # Bottom Ecosystem showcase strip
    eco_y = 694
    eco_h = 318
    d.rounded_rectangle((80, eco_y, 1840, eco_y + eco_h), radius=20, fill=(19, 34, 48), outline=GOLD_SOFT, width=2)
    draw_pill(d, 112, eco_y + 26, "BUILT ON HARMONY  ·  FEATURED MODS", bg=TEAL_DARK, fg=GOLD, outline=GOLD_SOFT, size=18)
    d.text((510, eco_y + 32), "Sandboxed Lua extensions & native-style panels — subscribe alongside Harmony", font=font_sans(21), fill=TEXT_MUTED)

    mods = [
        (
            "Convenience Tools",
            f"Workshop ID {CONVENIENCE_TOOLS_ID}",
            "One-click country-wide Make States, Abandon States, and Raze in a native EU4 panel — strictly obeying vanilla governing capacity, trade company, and raze rules.",
            TEAL,
        ),
        (
            "Regional Combat",
            f"Workshop ID {REGIONAL_COMBAT_ID}",
            "Assign an army an operating radius: automatically pursues visible enemies inside its zone, retakes lost provinces, and returns to base when idle.",
            GOLD,
        ),
    ]
    mw = 824
    mh = 192
    for i, (mname, mid, mdesc, accent) in enumerate(mods):
        mx = 112 + i * (mw + 48)
        my = eco_y + 94
        d.rounded_rectangle((mx, my, mx + mw, my + mh), radius=16, fill=CARD_BG, outline=CARD_BORDER, width=2)
        d.rounded_rectangle((mx, my + 24, mx + 8, my + mh - 24), radius=4, fill=accent)
        d.text((mx + 30, my + 24), mname, font=font_serif(30, bold=True), fill=TEXT_PRIMARY)
        d.text((mx + 30 + int(d.textlength(mname, font=font_serif(30, bold=True))) + 18, my + 34), mid, font=font_mono(18), fill=accent)
        bf = font_sans(20)
        lines = wrap_text(d, mdesc, bf, mw - 60)
        ly = my + 78
        for line in lines[:3]:
            d.text((mx + 30, ly), line, font=bf, fill=TEXT_MUTED)
            ly += 31

    out = OUT / "04-qol-features.png"
    img.save(out, optimize=True)
    return out


def build_05_install_guide() -> Path:
    """1920x1080 clean 3-step visual install diagram (no personal filesystem paths)."""
    img = make_gradient_canvas(1920, 1080)
    d = ImageDraw.Draw(img)
    draw_header(
        d,
        "INSTALL ONCE IN 30 SECONDS",
        "Subscribing alone does not activate Harmony — copy version.dll once; Steam handles all future updates",
    )

    top_y = 190
    col_h = 684

    # Step 1 (left, narrow)
    s1x, s1w = 80, 420
    d.rounded_rectangle((s1x, top_y, s1x + s1w, top_y + col_h), radius=20, fill=CARD_BG, outline=CARD_BORDER, width=2)
    draw_pill(d, s1x + 28, top_y + 28, "STEP 1", bg=GOLD, fg=BG_TOP, size=20, pad_x=18, pad_y=6)
    d.text((s1x + 28, top_y + 88), "Subscribe on", font=font_serif(34, bold=True), fill=TEXT_PRIMARY)
    d.text((s1x + 28, top_y + 132), "Steam Workshop", font=font_serif(34, bold=True), fill=GOLD)
    for i, line in enumerate(wrap_text(d, "Click Subscribe on this page and let Steam finish downloading Harmony.", font_sans(22), s1w - 56)):
        d.text((s1x + 28, top_y + 202 + i * 34), line, font=font_sans(22), fill=TEXT_MUTED)
    # Visual mock Subscribe button
    bx0, by0 = s1x + 48, top_y + 390
    d.rounded_rectangle((bx0, by0, bx0 + 324, by0 + 78), radius=12, fill=(92, 138, 26), outline=(146, 206, 62), width=3)
    bf = font_sans(28, "bold")
    bb = d.textbbox((0, 0), "Subscribed", font=bf)
    tw = bb[2] - bb[0]
    total_w = 24 + 14 + tw
    sx_btn = bx0 + (324 - total_w) // 2
    draw_check(d, sx_btn, by0 + 27, 24, (255, 255, 255), width=4)
    d.text((sx_btn + 38, by0 + 22), "Subscribed", font=bf, fill=(255, 255, 255))
    d.text((s1x + 28, top_y + 528), "Workshop ID:", font=font_sans(19, "semibold"), fill=TEXT_MUTED)
    d.text((s1x + 28, top_y + 558), ACTIVE_WORKSHOP_ID, font=font_mono(26, bold=True), fill=TEAL)

    # Step 2 (center, wide — hero visual)
    s2x, s2w = 536, 868
    d.rounded_rectangle((s2x, top_y, s2x + s2w, top_y + col_h), radius=20, fill=CARD_BG, outline=TEAL, width=3)
    draw_pill(d, s2x + 28, top_y + 28, "STEP 2  ·  ONLY ONCE", bg=TEAL, fg=BG_TOP, size=20, pad_x=18, pad_y=6)
    d.text((s2x + 28, top_y + 88), "Copy version.dll next to eu4.exe", font=font_serif(34, bold=True), fill=TEXT_PRIMARY)

    # Two mock Explorer windows side-by-side with clear copy arrow
    win_y = top_y + 158
    win_h = 310
    win_w = 356
    w1x = s2x + 32
    w2x = s2x + s2w - 32 - win_w

    # Left Explorer: Workshop folder
    d.rounded_rectangle((w1x, win_y, w1x + win_w, win_y + win_h), radius=12, fill=CARD_BG_ALT, outline=CARD_BORDER, width=2)
    d.rounded_rectangle((w1x, win_y, w1x + win_w, win_y + 44), radius=12, fill=(32, 48, 68))
    d.rectangle((w1x, win_y + 32, w1x + win_w, win_y + 44), fill=(32, 48, 68))
    d.text((w1x + 16, win_y + 11), "Harmony Workshop Folder", font=font_sans(18, "semibold"), fill=GOLD)
    d.text((w1x + 16, win_y + 56), f"...\\workshop\\content\\236850\\{ACTIVE_WORKSHOP_ID}", font=font_mono(14), fill=TEXT_MUTED)
    for j, (fname, hl) in enumerate([("eu4-harmony\\", False), ("descriptor.mod", False), ("version.dll", True), ("thumbnail.png", False)]):
        fy = win_y + 96 + j * 48
        if hl:
            d.rounded_rectangle((w1x + 14, fy - 6, w1x + win_w - 14, fy + 34), radius=8, fill=TEAL_DARK, outline=TEAL, width=2)
        d.text((w1x + 26, fy + 2), fname, font=font_mono(20, bold=hl), fill=TEXT_PRIMARY if hl else TEXT_MUTED)

    # Arrow between folders
    ax0, ax1, ay = w1x + win_w + 14, w2x - 14, win_y + 204
    d.line((ax0, ay, ax1 - 14, ay), fill=TEAL, width=6)
    d.polygon([(ax1, ay), (ax1 - 22, ay - 14), (ax1 - 22, ay + 14)], fill=TEAL)
    d.text((ax0 + 6, ay - 36), "COPY", font=font_sans(18, "bold"), fill=TEAL)

    # Right Explorer: EU4 Game folder
    d.rounded_rectangle((w2x, win_y, w2x + win_w, win_y + win_h), radius=12, fill=CARD_BG_ALT, outline=CARD_BORDER, width=2)
    d.rounded_rectangle((w2x, win_y, w2x + win_w, win_y + 44), radius=12, fill=(32, 48, 68))
    d.rectangle((w2x, win_y + 32, w2x + win_w, win_y + 44), fill=(32, 48, 68))
    d.text((w2x + 16, win_y + 11), "EU4 Game Folder", font=font_sans(18, "semibold"), fill=GOLD)
    d.text((w2x + 16, win_y + 56), "...\\common\\Europa Universalis IV\\", font=font_mono(14), fill=TEXT_MUTED)
    for j, (fname, hl) in enumerate([("dlc\\", False), ("eu4.exe", False), ("version.dll   ← paste here", True), ("steam_api64.dll", False)]):
        fy = win_y + 96 + j * 48
        if hl:
            d.rounded_rectangle((w2x + 14, fy - 6, w2x + win_w - 14, fy + 34), radius=8, fill=TEAL_DARK, outline=TEAL, width=2)
        d.text((w2x + 26, fy + 2), fname, font=font_mono(19, bold=hl), fill=TEXT_PRIMARY if hl else TEXT_MUTED)

    # Option A PowerShell hint bar below the mock folders
    ps_y = win_y + win_h + 28
    d.rounded_rectangle((s2x + 32, ps_y, s2x + s2w - 32, top_y + col_h - 28), radius=12, fill=(18, 29, 42), outline=GOLD_SOFT, width=2)
    d.text((s2x + 52, ps_y + 18), "OR FASTEST OPTION (1-LINE POWERSHELL)", font=font_sans(18, "bold"), fill=GOLD)
    d.text((s2x + 52, ps_y + 50), "Copy & paste the local one-line command in the description below.", font=font_sans(21), fill=TEXT_PRIMARY)
    d.text((s2x + 52, ps_y + 84), "Auto-finds Steam libraries, backs up old version.dll to .bak, downloads nothing.", font=font_sans(19), fill=TEXT_MUTED)

    # Step 3 (right, narrow)
    s3x, s3w = 1440, 400
    d.rounded_rectangle((s3x, top_y, s3x + s3w, top_y + col_h), radius=20, fill=CARD_BG, outline=CARD_BORDER, width=2)
    draw_pill(d, s3x + 28, top_y + 28, "STEP 3", bg=GOLD, fg=BG_TOP, size=20, pad_x=18, pad_y=6)
    d.text((s3x + 28, top_y + 88), "Launch EU4", font=font_serif(34, bold=True), fill=TEXT_PRIMARY)
    d.text((s3x + 28, top_y + 132), "As Usual", font=font_serif(34, bold=True), fill=TEAL)
    step3_notes = [
        "• No need to add Harmony to your playset",
        "• Enable mods built on Harmony normally",
        "• Look for the corner bar at bottom-right:",
    ]
    ly = top_y + 206
    for note in step3_notes:
        for line in wrap_text(d, note, font_sans(21), s3w - 56):
            d.text((s3x + 28, ly), line, font=font_sans(21), fill=TEXT_MUTED)
            ly += 31
        ly += 10

    # Mock bottom-right Harmony corner bar
    cb_y = top_y + 426
    d.rounded_rectangle((s3x + 24, cb_y, s3x + s3w - 24, cb_y + 58), radius=10, fill=(17, 25, 36), outline=GOLD_SOFT, width=2)
    ver_label = f'Harmony v{VERSION} "{CODENAME}"' if CODENAME else f"Harmony v{VERSION}"
    cb_txt = f"{ver_label}  |  Mods  |  Settings"
    cb_size = 19
    cbf = font_sans(cb_size, "semibold")
    cbb = d.textbbox((0, 0), cb_txt, font=cbf)
    while (cbb[2] - cbb[0]) > (s3w - 68) and cb_size > 13:
        cb_size -= 1
        cbf = font_sans(cb_size, "semibold")
        cbb = d.textbbox((0, 0), cb_txt, font=cbf)
    d.text((s3x + (s3w - (cbb[2] - cbb[0])) // 2, cb_y + (58 - (cbb[3] - cbb[1])) // 2 - cbb[1] - 1), cb_txt, font=cbf, fill=TEXT_PRIMARY)

    d.text((s3x + 28, top_y + 528), "Future Updates:", font=font_sans(19, "semibold"), fill=GOLD)
    for i, line in enumerate(wrap_text(d, "Steam updates Harmony automatically; newer version.dll syncs itself on launch.", font_sans(19), s3w - 56)):
        d.text((s3x + 28, top_y + 558 + i * 28), line, font=font_sans(19), fill=TEXT_MUTED)

    # Bottom trust banner
    trust_y = 918
    d.rounded_rectangle((80, trust_y, 1840, 1004), radius=16, fill=(20, 34, 48), outline=CARD_BORDER, width=2)
    trust = "100% Open Source (MIT)   ·   Steam Windows EU4 1.37.5   ·   Single Player   ·   Vanilla Rules Unchanged   ·   Unsubscribe Anytime"
    tf = font_sans(22, "semibold")
    tb = d.textbbox((0, 0), trust, font=tf)
    d.text(((1920 - (tb[2] - tb[0])) // 2, trust_y + 28), trust, font=tf, fill=TEXT_PRIMARY)

    out = OUT / "05-install-guide.png"
    img.save(out, optimize=True)
    return out


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    builders = [
        build_thumbnail,
        build_thumbnail_beta,
        build_thumbnail_zh,
        build_banner_zh,
        build_01_cjk_fonts,
        build_02_settings_ui,
        build_03_performance,
        build_04_qol_features,
        build_05_install_guide,
    ]
    for fn in builders:
        path = fn()
        size_kb = path.stat().st_size / 1024.0
        print(f"Built {path.relative_to(ROOT)} ({size_kb:.1f} KB)")


if __name__ == "__main__":
    main()
