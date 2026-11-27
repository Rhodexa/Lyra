#!/usr/bin/env python3
"""Make a "tape" for the faceplate mock (docs/design/faceplate.html).

Renders a score with the real engine, then writes what the faceplate's
telemetry would carry, as a script the page can load from file://:

    docs/design/tapes/NAME.js    window.TAPES.NAME = {notes, frames, ...}
    docs/design/tapes/NAME.ogg   the sound, so the mock plays in sync

usage: scripts/tape.py [--local] scores/prelude.txt [more...] [-- octavia options]

--local writes to docs/design/tapes/local/ (git-ignored) and lists what's
there in local/index.js, which the page loads when it exists. Use it for
MIDI files: they may be copyrighted and are never committed
(scripts/tape-midis.sh does them all).
"""
import json, re, subprocess, sys, tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "docs/design/tapes"
BIN = ROOT / "../../target/release/octavia"  # Lyra's shared target/
STEP = 4  # curves are 250 frames/s; keep every 4th = 62.5 frames/s

NOTE = re.compile(
    r"^\s*(?P<start>[\d.]+)s\s+(?P<len>[\d.]+)s\s+(?P<note>[A-G]#?-?\d)\s+"
    r"(?:\+(?P<note2>[A-G]#?-?\d)\s+)?(?P<dir>[↓↑])(?P<how>\w+)\s+"
    r"(?P<string>IV|III|II|I)\s+(?P<open>open\s+)?(?P<hand>\d+)\s+"
    r"(?P<load>\d+)\s+(?P<calm>\d+)(?:\s+shift\s+(?P<shift>\d+))?")
ROMAN = {"IV": 0, "III": 1, "II": 2, "I": 3}  # 0 = lowest string, as in the curves


def main():
    global OUT
    args = sys.argv[1:]
    extra = []
    if "--" in args:
        i = args.index("--")
        args, extra = args[:i], args[i + 1:]
    local = "--local" in args
    if local:
        args.remove("--local")
        OUT = OUT / "local"
    subprocess.run(["cargo", "build", "--release", "-q", "-p", "octavia-render"], cwd=ROOT, check=True)
    OUT.mkdir(parents=True, exist_ok=True)
    for score in args:
        tape(Path(score), extra)
    if local:
        names = sorted(p.stem for p in OUT.glob("*.js") if p.stem != "index")
        (OUT / "index.js").write_text("window.LOCAL_TAPES = " + json.dumps(names) + ";\n")


def tape(score, extra):
    name = score.stem
    with tempfile.TemporaryDirectory() as tmp:
        log = subprocess.run([BIN, score, "--explain", "-o", tmp, *extra],
                             cwd=ROOT, check=True, capture_output=True, text=True).stdout
        notes = []
        for line in log.splitlines():
            m = NOTE.match(line)
            if not m:
                continue
            notes.append({
                "t": float(m["start"]), "len": float(m["len"]), "note": m["note"],
                "note2": m["note2"], "dir": "down" if m["dir"] == "↓" else "up",
                "how": m["how"], "string": ROMAN[m["string"]], "open": bool(m["open"]),
                "hand": int(m["hand"]), "load": int(m["load"]), "calm": int(m["calm"]),
                "shift": int(m["shift"]) if m["shift"] else 0,
            })
        rows = (Path(tmp) / f"{name}.curves.csv").read_text().splitlines()
        # events: the bow leaving a string that rings on (a finger lifting leaves the open
        # string ringing), plucks (also in the notes)
        rings = [{"t": float(c[1]), "pitch": float(c[2]), "level": float(c[3]),
                  "string": int(c[4]), "open": c[5] == "1", "bright": float(c[6])}
                 for c in (r.split(",") for r in rows) if c[0] == "ring"]
        # finger taps: a finger landing or lifting knocks the fingerboard
        taps = [{"t": float(c[1]), "strength": float(c[2])}
                for c in (r.split(",") for r in rows) if c[0] == "tap"]
        rows = [r for r in rows if r[0].isdigit() or r.startswith(("pitch", "-"))]
        cols = rows[0].split(",")
        frames = {c: [] for c in cols}
        for row in rows[1::STEP]:
            for c, v in zip(cols, row.split(",")):
                frames[c].append(round(float(v), 3))
        instrument = "violin" if "violin" in extra else "cello"
        data = {"name": name, "fps": 250 / STEP, "instrument": instrument,
                "options": " ".join(extra), "notes": notes, "rings": rings, "taps": taps, "frames": frames}
        (OUT / f"{name}.js").write_text(
            f"// made by scripts/tape.py from {score.as_posix()} {' '.join(extra)}\n"
            f"(window.TAPES = window.TAPES || {{}})[{json.dumps(name)}] = "
            + json.dumps(data, separators=(",", ":"), ensure_ascii=False) + ";\n")
        subprocess.run(["ffmpeg", "-loglevel", "error", "-y", "-i", Path(tmp) / f"{name}.wav",
                        "-c:a", "libopus", "-b:a", "64k", OUT / f"{name}.ogg"], check=True)
    print(f"{name}: {len(notes)} notes, {len(frames['pitch'])} frames")


if __name__ == "__main__":
    main()
