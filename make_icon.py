#!/usr/bin/env python3
# Рисует иконку (щит + снежинка, холодная тема) через Pillow и собирает .ico
import math
from PIL import Image, ImageDraw

S = 1024  # рендерим крупно, потом уменьшаем (антиалиасинг)

def lerp(a, b, t):
    return tuple(int(a[i] + (b[i] - a[i]) * t) for i in range(3))

def vgrad(size, top, bot):
    w, h = size
    g = Image.new("RGB", (w, h))
    px = g.load()
    for y in range(h):
        c = lerp(top, bot, y / max(h - 1, 1))
        for x in range(w):
            px[x, y] = c
    return g

img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
d = ImageDraw.Draw(img)

# --- фон: скруглённый квадрат с градиентом ---
bg = vgrad((S, S), (12, 46, 78), (6, 20, 34))
mask = Image.new("L", (S, S), 0)
ImageDraw.Draw(mask).rounded_rectangle([0, 0, S - 1, S - 1], radius=int(S * 0.205), fill=255)
img.paste(bg, (0, 0), mask)
d = ImageDraw.Draw(img)

sc = S / 256.0  # масштаб из координат 256

# --- декоративные снежинки на фоне (больше снега) ---
snow = [
    (40,48,3.2),(70,34,2),(104,40,1.6),(150,36,1.8),(190,44,2.4),(218,60,3),
    (232,92,2),(236,124,1.8),(230,158,2.4),(214,188,3.2),(188,214,2),
    (150,224,1.7),(108,222,2.2),(70,214,1.6),(44,192,3),(28,160,2),
    (22,124,2.4),(28,92,1.8),(58,70,1.5),(200,206,1.6),(36,74,1.6),
    (126,28,1.6),(242,142,1.5),(16,140,1.6),
]
for (cx, cy, r) in snow:
    d.ellipse([ (cx-r)*sc,(cy-r)*sc,(cx+r)*sc,(cy+r)*sc ], fill=(175,222,255,95))

# --- щит (внешний): остроугольный, прямые грани, строго симметричный ---
# правая половина сверху вниз до острия (прямыми отрезками)
right = [(128, 36), (206, 62), (206, 120), (128, 224)]
# левая половина — зеркало правой относительно оси x=128
left = [(256 - x, y) for (x, y) in reversed(right)]
shield_pts = right + left[1:-1]           # без дублей острия и вершины
poly = [(x * sc, y * sc) for (x, y) in shield_pts]

smask = Image.new("L", (S, S), 0)
ImageDraw.Draw(smask).polygon(poly, fill=255)
sgrad = vgrad((S, S), (92, 198, 255), (27, 95, 174))
img.paste(sgrad, (0, 0), smask)
d = ImageDraw.Draw(img)
# обводка щита
d.line(poly + [poly[0]], fill=(191, 232, 255, 255), width=int(4*sc), joint="curve")

# --- внутренний щит (темнее) ---
inner = [(x,y) for (x,y) in shield_pts]
def shrink(pts, cx=128, cy=130, k=0.80):
    return [(cx+(x-cx)*k, cy+(y-cy)*k) for (x,y) in pts]
ipoly = [(x*sc,y*sc) for (x,y) in shrink(shield_pts)]
imask = Image.new("L", (S, S), 0)
ImageDraw.Draw(imask).polygon(ipoly, fill=255)
igrad = vgrad((S, S), (13, 51, 87), (10, 39, 68))
img.paste(igrad, (0, 0), imask)
d = ImageDraw.Draw(img)

# --- снежинка в центре (идеально симметрична через поворот одного луча) ---
cx, cy = 128*sc, 120*sc
R = 52*sc
col = (223, 242, 255, 255)
lw = int(7*sc)
branch_lw = int(5*sc)

# один луч (вверх) с симметричными ветками — на отдельном прозрачном слое
arm = Image.new("RGBA", (S, S), (0, 0, 0, 0))
ad = ImageDraw.Draw(arm)
tip = (cx, cy - R)
ad.line([cx, cy, tip[0], tip[1]], fill=col, width=lw)
for frac in (0.52, 0.80):
    bx, by = cx, cy - R * frac
    blen = 16 * sc * (1.0 - (frac - 0.52) * 0.6)  # верхние ветки чуть короче
    for da in (40, -40):
        aa = math.radians(-90 + da)  # относительно направления «вверх»
        ad.line([bx, by, bx + math.cos(aa) * blen, by + math.sin(aa) * blen],
                fill=col, width=branch_lw)

# разворачиваем луч 6 раз вокруг центра
for k in range(6):
    rot = arm.rotate(60 * k, resample=Image.BICUBIC, center=(cx, cy))
    img.alpha_composite(rot)
d = ImageDraw.Draw(img)

# аккуратный центральный шарик поверх
d.ellipse([cx-9*sc, cy-9*sc, cx+9*sc, cy+9*sc], fill=col)

# --- уменьшаем до 256 с антиалиасингом ---
icon256 = img.resize((256, 256), Image.LANCZOS)
icon256.save("icon_preview.png")

# --- многоразмерный .ico ---
sizes = [16, 24, 32, 48, 64, 128, 256]
icon256.save("app.ico", sizes=[(s, s) for s in sizes])
print("готово: icon_preview.png, app.ico")
