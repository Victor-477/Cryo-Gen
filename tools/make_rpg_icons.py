"""Draws the item, skill and talent icons of Ashen Keep (96x96 PNG), all by code.

    python3 tools/make_rpg_icons.py   ->  crates/cryo-editor/template/ashen_icons/*.png
"""
import math, os
from PIL import Image, ImageDraw, ImageFilter

OUT = os.path.join(os.path.dirname(__file__), "..", "crates", "cryo-editor", "template", "ashen_icons")
S = 96
os.makedirs(OUT, exist_ok=True)


def base(c1, c2):
    """A framed tile with a vertical gradient."""
    im = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    for y in range(S):
        t = y / (S - 1)
        c = tuple(int(c1[i] * (1 - t) + c2[i] * t) for i in range(3)) + (255,)
        d.line([(0, y), (S, y)], fill=c)
    mask = Image.new("L", (S, S), 0)
    ImageDraw.Draw(mask).rounded_rectangle([1, 1, S - 2, S - 2], 14, fill=255)
    im.putalpha(mask)
    d = ImageDraw.Draw(im)
    d.rounded_rectangle([1, 1, S - 2, S - 2], 14, outline=(20, 16, 12, 255), width=3)
    d.rounded_rectangle([5, 5, S - 6, S - 6], 11, outline=(255, 230, 170, 60), width=1)
    return im


def glow(im, shape_fn, color, radius=6):
    layer = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    shape_fn(ImageDraw.Draw(layer), color)
    blurred = layer.filter(ImageFilter.GaussianBlur(radius))
    im.alpha_composite(blurred)
    im.alpha_composite(layer)


def save(im, name):
    im.save(os.path.join(OUT, name + ".png"))


def sword(d, blade=(215, 220, 230), guard=(170, 130, 60), grip=(90, 55, 30), big=False):
    w = 7 if big else 5
    d.polygon([(70, 18), (78, 18), (78, 26), (34, 70), (26, 62)], fill=blade, outline=(60, 60, 70))
    d.line([(74, 22), (30, 66)], fill=(255, 255, 255), width=1)
    d.line([(20, 56), (40, 76)], fill=guard, width=w)
    d.line([(28, 68), (16, 80)], fill=grip, width=6)
    d.ellipse([10, 78, 18, 86], fill=guard)


def icons():
    # --- items
    im = base((90, 20, 24), (40, 8, 12))
    glow(im, lambda d, c: d.ellipse([28, 40, 68, 80], fill=c), (230, 50, 60), 7)
    d = ImageDraw.Draw(im)
    d.rectangle([42, 22, 54, 42], fill=(200, 210, 220))
    d.rectangle([39, 18, 57, 24], fill=(120, 80, 40))
    d.ellipse([28, 40, 68, 80], outline=(240, 220, 220), width=3)
    d.ellipse([36, 48, 46, 56], fill=(255, 200, 200))
    save(im, "potion")

    im = base((50, 30, 20), (20, 12, 8))
    glow(im, lambda d, c: d.polygon([(48, 16), (68, 46), (48, 82), (28, 46)], fill=c), (255, 120, 40), 8)
    d = ImageDraw.Draw(im)
    d.polygon([(48, 16), (68, 46), (48, 82), (28, 46)], outline=(255, 220, 160), width=2)
    d.line([(48, 16), (48, 82)], fill=(255, 200, 120), width=1)
    save(im, "ember_shard")

    im = base((70, 60, 50), (30, 26, 20))
    sword(ImageDraw.Draw(im), blade=(150, 120, 90))
    save(im, "rusted_sword")

    im = base((40, 60, 90), (14, 20, 34))
    sword(ImageDraw.Draw(im))
    save(im, "knight_blade")

    im = base((90, 30, 10), (30, 8, 4))
    glow(im, lambda d, c: d.polygon([(76, 14), (84, 22), (40, 74), (24, 58)], fill=c), (255, 110, 30), 6)
    d = ImageDraw.Draw(im)
    d.polygon([(76, 14), (84, 22), (40, 74), (24, 58)], fill=(60, 50, 50), outline=(255, 160, 80))
    d.polygon([(58, 26), (82, 20), (70, 46)], fill=(80, 70, 70), outline=(255, 160, 80))
    d.line([(26, 66), (12, 82)], fill=(60, 30, 20), width=7)
    save(im, "warden_cleaver")

    im = base((80, 70, 50), (40, 32, 22))
    d = ImageDraw.Draw(im)
    d.polygon([(30, 22), (66, 22), (78, 36), (72, 80), (24, 80), (18, 36)], fill=(170, 145, 105), outline=(60, 40, 20), width=2)
    for y in range(30, 80, 8):
        d.line([(24, y), (72, y)], fill=(130, 105, 70), width=2)
    d.polygon([(38, 22), (58, 22), (48, 34)], fill=(60, 40, 20))
    save(im, "padded_gambeson")

    im = base((60, 66, 76), (20, 22, 28))
    d = ImageDraw.Draw(im)
    d.polygon([(30, 20), (66, 20), (80, 36), (74, 82), (22, 82), (16, 36)], fill=(150, 158, 170), outline=(40, 44, 50), width=2)
    for y in range(26, 82, 6):
        for x in range(22, 76, 6):
            d.arc([x, y, x + 6, y + 6], 0, 180, fill=(100, 106, 118))
    d.polygon([(38, 20), (58, 20), (48, 32)], fill=(40, 44, 50))
    save(im, "iron_hauberk")

    im = base((70, 30, 10), (24, 10, 4))
    d = ImageDraw.Draw(im)
    d.ellipse([22, 30, 74, 82], outline=(220, 170, 60), width=8)
    glow(im, lambda d, c: d.polygon([(48, 14), (60, 30), (48, 42), (36, 30)], fill=c), (255, 110, 40), 6)
    save(im, "ember_ring")

    im = base((30, 50, 70), (10, 18, 28))
    d = ImageDraw.Draw(im)
    d.line([(30, 14), (48, 40), (66, 14)], fill=(200, 180, 120), width=3)
    glow(im, lambda d, c: d.ellipse([32, 38, 64, 70], fill=c), (110, 180, 255), 6)
    d = ImageDraw.Draw(im)
    d.ellipse([32, 38, 64, 70], outline=(230, 210, 150), width=4)
    save(im, "ward_amulet")

    # --- skills and talents
    def swirl(d, c):
        for k in range(3):
            a0 = k * 120
            pts = []
            for i in range(30):
                t = i / 29
                a = math.radians(a0 + t * 200)
                r = 8 + t * 30
                pts.append((48 + math.cos(a) * r, 48 + math.sin(a) * r))
            d.line(pts, fill=c, width=5)

    im = base((80, 40, 20), (30, 12, 6))
    glow(im, swirl, (255, 200, 120), 5)
    save(im, "whirlwind")

    im = base((50, 60, 80), (16, 20, 30))
    d = ImageDraw.Draw(im)
    d.polygon([(48, 14), (78, 26), (72, 60), (48, 84), (24, 60), (18, 26)], fill=(110, 120, 140), outline=(230, 230, 240), width=3)
    d.line([(48, 18), (48, 80)], fill=(200, 160, 60), width=4)
    d.line([(22, 40), (74, 40)], fill=(200, 160, 60), width=4)
    for a in range(-40, 50, 20):
        r = math.radians(a)
        d.line([(80 + math.cos(r) * 4, 48 + math.sin(r) * 18), (92, 48 + math.sin(r) * 26)], fill=(255, 240, 200), width=2)
    save(im, "shield_bash")

    im = base((90, 50, 10), (30, 14, 4))
    d = ImageDraw.Draw(im)
    for r, a in [(14, 255), (24, 170), (34, 100)]:
        d.arc([48 - r, 48 - r, 48 + r, 48 + r], -60, 60, fill=(255, 220, 140, a), width=4)
        d.arc([48 - r, 48 - r, 48 + r, 48 + r], 120, 240, fill=(255, 220, 140, a), width=4)
    d.ellipse([40, 40, 56, 56], fill=(255, 240, 200))
    save(im, "war_cry")

    def burst(d, c):
        for i in range(12):
            a = math.radians(i * 30)
            r = 36 if i % 2 == 0 else 24
            d.polygon([(48, 48), (48 + math.cos(a - 0.15) * 12, 48 + math.sin(a - 0.15) * 12), (48 + math.cos(a) * r, 48 + math.sin(a) * r), (48 + math.cos(a + 0.15) * 12, 48 + math.sin(a + 0.15) * 12)], fill=c)
        d.ellipse([36, 36, 60, 60], fill=(255, 240, 180))

    im = base((100, 30, 6), (30, 6, 2))
    glow(im, burst, (255, 130, 30), 6)
    save(im, "ember_burst")

    im = base((20, 70, 40), (6, 26, 14))
    d = ImageDraw.Draw(im)
    glow(im, lambda d, c: (d.rectangle([40, 22, 56, 74], fill=c), d.rectangle([22, 40, 74, 56], fill=c)), (120, 255, 150), 7)
    save(im, "second_wind")

    im = base((70, 70, 80), (24, 24, 30))
    d = ImageDraw.Draw(im)
    d.polygon([(48, 12), (56, 60), (48, 70), (40, 60)], fill=(220, 225, 235), outline=(80, 80, 90))
    d.line([(30, 64), (66, 64)], fill=(190, 150, 70), width=5)
    d.line([(48, 66), (48, 84)], fill=(90, 55, 30), width=6)
    save(im, "keen_edge")

    im = base((80, 20, 20), (26, 6, 6))
    d = ImageDraw.Draw(im)
    d.polygon([(20, 70), (70, 20), (80, 30), (30, 80)], fill=(190, 190, 200))
    d.polygon([(56, 16), (84, 14), (82, 42)], fill=(160, 40, 40))
    save(im, "executioner")

    im = base((70, 50, 30), (26, 18, 10))
    d = ImageDraw.Draw(im)
    d.ellipse([20, 20, 76, 76], fill=(150, 110, 70), outline=(230, 200, 150), width=4)
    d.arc([30, 30, 66, 66], 200, 340, fill=(230, 200, 150), width=4)
    save(im, "thick_hide")

    im = base((40, 50, 70), (12, 16, 26))
    d = ImageDraw.Draw(im)
    d.rectangle([28, 22, 68, 74], fill=(120, 128, 145), outline=(230, 230, 240), width=3)
    d.rectangle([40, 34, 56, 62], fill=(70, 76, 90))
    save(im, "iron_will")

    def flame(d, c):
        d.polygon([(48, 12), (66, 44), (70, 64), (58, 82), (38, 82), (26, 64), (32, 44), (42, 56)], fill=c)

    im = base((90, 30, 6), (26, 6, 2))
    glow(im, flame, (255, 150, 40), 6)
    d = ImageDraw.Draw(im)
    d.polygon([(48, 44), (58, 64), (48, 78), (38, 64)], fill=(255, 230, 150))
    save(im, "kindled")

    im = base((70, 10, 14), (20, 2, 4))
    glow(im, lambda d, c: d.polygon([(48, 16), (66, 50), (60, 72), (48, 80), (36, 72), (30, 50)], fill=c), (220, 30, 40), 6)
    save(im, "ashen_blood")


icons()
print("icons written to", os.path.normpath(OUT))
