"""Letterbox the snapshot frames (rendered over DESKTOPFLY_SNAPSHOT_BG), then cut the four creature
clips into one captioned montage and encode it mobile-safe for the showcase."""
import os, subprocess, glob
import numpy as np
from PIL import Image

ROOT = os.path.dirname(os.path.abspath(__file__))
FRAMES = os.path.join(ROOT, "..", "..", "output", "showcase", "frames")
KEYED = os.path.join(ROOT, "..", "..", "output", "showcase", "letterboxed")
OUT = os.path.join(ROOT, "..", "..", "output", "showcase", "out")
os.makedirs(OUT, exist_ok=True)
BG = np.array([19, 21, 31], dtype=np.uint8)
FPS = 12
CLIPS = [
    ("drosophila", "Drosophila · 668 real neurons, ~19k FlyWire synapses"),
    ("salticid", "Jumping spider · the retreat"),
    ("araneus", "Orb weaver · 40 s into building its web"),
    ("koi", "Koi · the stone pond"),
]

def key(src, dst):
    # Frames are already composited over the card colour by the renderer
    # (DESKTOPFLY_SNAPSHOT_BG); just letterbox the 900x900 frame into 16:9.
    im = Image.open(src).convert("RGB").resize((720, 720), Image.LANCZOS)
    canvas = Image.new("RGB", (1280, 720), tuple(BG))
    canvas.paste(im, (280, 0))
    canvas.save(dst, quality=92)

parts = []
for cid, label in CLIPS:
    srcs = sorted(glob.glob(os.path.join(FRAMES, cid, "f*.png")))
    kd = os.path.join(KEYED, cid); os.makedirs(kd, exist_ok=True)
    for i, s in enumerate(srcs):
        d = os.path.join(kd, "f%04d.jpg" % i)
        if not os.path.exists(d):
            key(s, d)
    clip = os.path.join(OUT, cid + ".mp4")
    txt = label.replace("'", "’").replace(":", "\\:")
    subprocess.check_call([
        "ffmpeg", "-y", "-hide_banner", "-loglevel", "error",
        "-framerate", str(FPS), "-i", os.path.join(kd, "f%04d.jpg"),
        "-vf", f"fps=24,drawtext=text='{txt}':fontfile='C\\:/Windows/Fonts/segoeui.ttf':fontsize=30:fontcolor=white@0.92:x=40:y=h-70:box=1:boxcolor=black@0.35:boxborderw=12,fade=t=in:st=0:d=0.4,fade=t=out:st=9.6:d=0.4",
        "-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", "18", "-preset", "medium", clip,
    ])
    parts.append(clip)
    print("clip", cid, len(srcs), "frames")

lst = os.path.join(OUT, "list.txt")
with open(lst, "w") as f:
    for p in parts:
        f.write("file '%s'\n" % p.replace("\\", "/"))
tmp = os.path.join(OUT, "montage-tmp.mp4")
subprocess.check_call(["ffmpeg", "-y", "-hide_banner", "-loglevel", "error", "-f", "concat", "-safe", "0", "-i", lst, "-vf", "scale=in_range=pc:out_range=tv,format=yuv420p",
                       "-c:v", "libx264", "-pix_fmt", "yuv420p", "-profile:v", "high", "-level", "4.0", "-crf", "20",
                       "-preset", "medium", "-movflags", "+faststart", "-an", tmp])
final = os.path.join(OUT, "desktopfly-menagerie.mp4")
# Strip the edit list ffmpeg re-adds (the Android-Chrome freeze; see README).
subprocess.check_call(["ffmpeg", "-y", "-hide_banner", "-loglevel", "error", "-i", tmp, "-c", "copy", "-movflags", "+faststart",
                       "-use_editlist", "0", "-avoid_negative_ts", "make_zero", final])
subprocess.check_call(["ffmpeg", "-y", "-hide_banner", "-loglevel", "error", "-ss", "3", "-i", final, "-frames:v", "1", "-q:v", "3",
                       os.path.join(OUT, "poster-desktopfly-menagerie.jpg")])
print("final", final)
