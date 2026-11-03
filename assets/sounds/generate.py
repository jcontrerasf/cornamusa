# Synthesizes the notification sounds in public/sounds (no samples, so no licensing issues).
# Usage: python3 assets/sounds/generate.py public/sounds
import math, wave, struct, sys, random
SR = 44100
out = sys.argv[1]

def brass(freq, dur, t0, buf, vol=1.0, bend=0.0, vib=0.004, bright=1.0):
    """Brass-like tone: harmonics get brighter as the note swells (like a real horn)."""
    n0 = int(t0 * SR); n = int(dur * SR)
    phase = [0.0] * 12
    for i in range(n):
        t = i / SR
        a = min(1, t / 0.045)                      # soft attack
        r = min(1, (dur - t) / 0.12)               # gentle release
        env = (a * r) ** 1.3
        f = freq * (1 + bend * min(1, t / 0.08) - bend) * (1 + vib * math.sin(2 * math.pi * 5.2 * t) * min(1, t / 0.15))
        s = 0.0
        for h in range(1, 12):
            amp = (env ** (0.6 + h * 0.35 / bright)) / h ** 0.9
            phase[h] += 2 * math.pi * f * h / SR
            s += amp * math.sin(phase[h])
        if n0 + i < len(buf):
            buf[n0 + i] += s * vol

def drone(freq, dur, t0, buf, vol):
    n0 = int(t0 * SR); n = int(dur * SR); ph = [0.0] * 8
    for i in range(n):
        t = i / SR
        env = min(1, t / 0.08) * min(1, (dur - t) / 0.2)
        s = sum((1 / h ** 1.2) * math.sin(ph[h]) for h in range(1, 8))
        for h in range(1, 8): ph[h] += 2 * math.pi * freq * h / SR
        if n0 + i < len(buf): buf[n0 + i] += s * env * vol

def save(name, buf, peak=0.32):
    # tiny room reverb so it doesn't sound dry/harsh
    rev = buf[:]
    for d, g in ((0.031, 0.25), (0.047, 0.18), (0.071, 0.12), (0.113, 0.07)):
        k = int(d * SR)
        for i in range(k, len(rev)): rev[i] += buf[i - k] * g
    m = max(abs(x) for x in rev) or 1
    with wave.open(f"{out}/{name}.wav", "wb") as w:
        w.setnchannels(1); w.setsampwidth(2); w.setframerate(SR)
        w.writeframes(b"".join(struct.pack("<h", int(x / m * peak * 32767)) for x in rev))

# 1. Corneta suave: two rising notes (G4 -> C5), short and warm
b = [0.0] * int(0.95 * SR)
brass(392.0, 0.22, 0.00, b, 0.8, bend=0.02, bright=0.8)
brass(523.25, 0.50, 0.20, b, 1.0, bend=0.015, bright=0.8)
save("corneta-suave", b)

# 2. Corneta de cartero: one note with a little lip bend up, very short
b = [0.0] * int(0.7 * SR)
brass(466.16, 0.42, 0.0, b, 1.0, bend=0.04, bright=1.1)
save("corneta-cartero", b)

# 3. Gaita breve: soft drone + three quick notes (D5 E5 A5)
b = [0.0] * int(1.1 * SR)
drone(146.83, 1.0, 0.0, b, 0.22)
for f, t0, d in ((587.33, 0.05, 0.16), (659.25, 0.20, 0.16), (880.0, 0.35, 0.55)):
    brass(f, d, t0, b, 0.7, vib=0.006, bright=1.4)
save("gaita-breve", b, peak=0.28)
