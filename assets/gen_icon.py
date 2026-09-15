"""Generate a LogScope app icon: dark rounded panel with colored log lines."""
from PIL import Image, ImageDraw

S = 1024  # master size

def roundrect(d, box, r, fill):
    d.rounded_rectangle(box, radius=r, fill=fill)

img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
d = ImageDraw.Draw(img)

# --- background: dark navy rounded square, subtle vertical gradient ---
bg_top = (24, 28, 44)
bg_bot = (16, 18, 30)
# draw gradient via horizontal bands
for y in range(S):
    t = y / S
    c = tuple(int(bg_top[i] + (bg_bot[i] - bg_top[i]) * t) for i in range(3))
    d.line([(0, y), (S, y)], fill=c + (255,))
# rounded mask for background
mask = Image.new("L", (S, S), 0)
md = ImageDraw.Draw(mask)
md.rounded_rectangle([0, 0, S, S], radius=int(S*0.22), fill=255)
# apply mask
img.putalpha(Image.composite(img.getchannel("A"), Image.new("L",(S,S),0), mask))
# re-draw: simpler approach - draw background already rounded on a fresh layer
img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
d = ImageDraw.Draw(img)
def grad_rect(d, box, r, ctop, cbot):
    # build a gradient rounded rectangle
    x0, y0, x1, y1 = box
    layer = Image.new("RGBA", (S, S), (0,0,0,0))
    ld = ImageDraw.Draw(layer)
    for y in range(y0, y1):
        t = (y - y0) / max(1, (y1 - y0))
        c = tuple(int(ctop[i] + (cbot[i] - ctop[i]) * t) for i in range(3))
        ld.line([(x0, y), (x1, y)], fill=c + (255,))
    m = Image.new("L", (S, S), 0)
    ImageDraw.Draw(m).rounded_rectangle(box, radius=r, fill=255)
    layer.putalpha(Image.composite(layer.getchannel("A"), Image.new("L",(S,S),0), m) )
    img.alpha_composite(layer)

grad_rect(d, (0, 0, S, S), int(S*0.20), (28, 32, 52), (14, 16, 28))

# --- inner panel (the "log document"): slightly lighter, rounded ---
pad = int(S * 0.14)
panel = (pad, pad, S - pad, S - pad)
grad_rect(d, panel, int(S*0.10), (40, 46, 72), (30, 34, 56))

d = ImageDraw.Draw(img)
# --- log lines with severity dots ---
# line geometry
lx0 = int(S * 0.28)
lx1 = int(S * 0.74)
dot_r = int(S * 0.045)
dot_x = int(S * 0.20)

rows = [
    (0.30, (98, 190, 110)),   # green  = INFO
    (0.40, (240, 190, 90)),   # amber  = WARN
    (0.50, (235, 105, 105)),  # red    = ERROR
    (0.60, (98, 190, 110)),   # green
    (0.70, (150, 165, 200)),  # slate  = DEBUG
]
line_h = int(S * 0.045)
for (fy, color) in rows:
    cy = int(S * fy)
    d.ellipse([dot_x - dot_r, cy - dot_r, dot_x + dot_r, cy + dot_r], fill=color + (255,))
    # line with rounded end
    d.rounded_rectangle([lx0, cy - line_h//2, lx1, cy + line_h//2], radius=line_h//2, fill=(150, 160, 190, 220))

# --- top "title bar" hint: three dots like a window ---
ty = int(S * 0.20)
for i, (cc, tx) in enumerate([((235,105,105),0.22), ((240,190,90),0.26), ((98,190,110),0.30)]):
    d.ellipse([int(S*tx)-dot_r*2//3, ty-dot_r*2//3, int(S*tx)+dot_r*2//3, ty+dot_r*2//3], fill=cc + (255,))

img.save("assets/logscope_icon.png")  # master
# multi-size ICO
sizes = [16, 20, 24, 32, 40, 48, 64, 128, 256]
img.save("assets/logscope_icon.ico", format="ICO", sizes=[(s, s) for s in sizes])
print("saved", img.size)