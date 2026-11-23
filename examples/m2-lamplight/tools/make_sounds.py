"""Writes Lamplight's synthesized sounds and night theme (our own work, CC0-1.0).

python3 -I examples/m2-lamplight/tools/make_sounds.py examples/m2-lamplight/assets

48 kHz mono 16-bit WAVs in sounds/:
- sizzle.wav (0.6 s): crackling noise that fades, a wisp burning in lamplight.
- pop.wav (0.35 s): a falling tone and a puff of noise, a wisp bursting.
- whoosh.wav (0.45 s): a swelling, brightening breath, a lamp catching.
- rumble.wav (2.0 s, loops seamlessly): low rolling noise, an oil wagon on cobbles.
- click.wav (0.04 s): a short tick for UI buttons.
- chime.wav (2.5 s): bell partials (1, 2.76, 5.40, 8.93 x 523.25 Hz) ringing out, layered with
  the struck bell at dawn.
music/night.ogg: the night theme, 36 s (a 4 s intro, then a 32 s loop from frame 192000): a
drone on A2 and E3 under a slow A-minor arpeggio. Written as a WAV, then encoded by oggenc
(vorbis-tools) with a fixed serial and no comments.

Samples use only + and * (a polynomial sine, multiplied decays and a seeded xorshift), so every
machine writes the same PCM; the Ogg bytes also depend on the libvorbis version.
"""
import os
import shutil
import struct
import subprocess
import sys
import tempfile
import wave

RATE = 48000
TAU = 6.283185307179586
PI = 3.141592653589793


def sin(x):
    """sin(x) from range reduction and an odd Taylor polynomial (error < 1e-7 on [-pi/2, pi/2])."""
    x = x - TAU * int(x / TAU)
    if x > PI:
        x -= TAU
    elif x < -PI:
        x += TAU
    if x > PI / 2:
        x = PI - x
    elif x < -PI / 2:
        x = -PI - x
    x2 = x * x
    return x * (1 - x2 / 6 * (1 - x2 / 20 * (1 - x2 / 42 * (1 - x2 / 72 * (1 - x2 / 110)))))


class Noise:
    """Seeded white noise in [-1, 1)."""

    def __init__(self, seed):
        self.state = seed & 0xFFFFFFFF or 1

    def __call__(self):
        s = self.state
        s ^= (s << 13) & 0xFFFFFFFF
        s ^= s >> 17
        s ^= (s << 5) & 0xFFFFFFFF
        self.state = s
        return s / 2147483648.0 - 1.0


def lowpass(samples, k):
    """One-pole low-pass: y += k (x − y)."""
    y, out = 0.0, []
    for x in samples:
        y += k * (x - y)
        out.append(y)
    return out


def seconds(s):
    return int(RATE * s)


def sizzle():
    noise, n = Noise(11), seconds(0.6)
    out, env = [], 1.0
    hiss = lowpass([noise() for _ in range(n)], 0.6)
    for i in range(n):
        crackle = 1.0 if noise() > 0.995 else 0.0
        out.append(env * (0.35 * hiss[i] + 0.6 * crackle * noise()))
        env *= 0.99992
    return fade(out, 0.0, 0.1)


def pop():
    noise, n = Noise(23), seconds(0.35)
    out, env, phase, freq = [], 1.0, 0.0, 420.0
    for _ in range(n):
        phase += TAU * freq / RATE
        out.append(env * (0.7 * sin(phase) + 0.25 * noise()))
        freq *= 0.99993
        env *= 0.99975
    return fade(out, 0.002, 0.03)


def whoosh():
    noise, n = Noise(37), seconds(0.45)
    raw = [noise() for _ in range(n)]
    out = []
    y = 0.0
    for i, x in enumerate(raw):
        t = i / n
        k = 0.02 + 0.3 * t
        y += k * (x - y)
        out.append(0.8 * y * (t * 4 if t < 0.25 else 1.0 - (t - 0.25) / 0.75))
    return out


def rumble():
    """Two seconds of low noise that loops: the last 0.25 s cross-fades into the start."""
    noise, n, tail = Noise(41), seconds(2.0), seconds(0.25)
    raw = lowpass(lowpass([noise() for _ in range(n + tail)], 0.01), 0.02)
    peak = max(abs(v) for v in raw) or 1.0
    raw = [0.8 * v / peak for v in raw]
    out = raw[:n]
    for i in range(tail):
        w = i / tail
        out[i] = out[i] * w + raw[n + i] * (1 - w)
    return out


def click():
    n = seconds(0.04)
    out, env, phase = [], 1.0, 0.0
    for _ in range(n):
        phase += TAU * 2200.0 / RATE
        out.append(0.6 * env * sin(phase))
        env *= 0.9985
    return fade(out, 0.0005, 0.005)


def chime():
    n = seconds(2.5)
    partials = [(1.0, 0.45, 0.99993), (2.76, 0.3, 0.99988), (5.40, 0.15, 0.9998), (8.93, 0.08, 0.9997)]
    out = [0.0] * n
    for ratio, amp, decay in partials:
        phase, env, step = 0.0, amp, TAU * 523.25 * ratio / RATE
        for i in range(n):
            phase += step
            out[i] += env * sin(phase)
            env *= decay
    return fade(out, 0.002, 0.2)


def night():
    """A 4 s intro (the drone swelling in) and a 32 s loop of eight 4 s bars."""
    intro, loop = seconds(4.0), seconds(32.0)
    n = intro + loop
    out = [0.0] * n
    for freq, amp in ((110.0, 0.18), (164.81, 0.12)):
        phase, step = 0.0, TAU * freq / RATE
        for i in range(n):
            phase += step
            swell = min(1.0, i / intro)
            out[i] += amp * swell * (sin(phase) + 0.3 * sin(2 * phase))
    # A-minor arpeggio, one note per half second from the loop start; the pattern spans 32 s.
    notes = [220.0, 261.63, 329.63, 392.0, 329.63, 261.63, 246.94, 293.66]
    note_len = seconds(0.5)
    for k in range(loop // note_len):
        freq = notes[k % len(notes)] * (0.5 if (k // 16) % 2 else 1.0)
        start = intro + k * note_len
        phase, env, step = 0.0, 0.16, TAU * freq / RATE
        for i in range(start, min(n, start + seconds(1.5))):
            phase += step
            v = env * (sin(phase) + 0.2 * sin(3 * phase))
            # Notes ringing past the loop end wrap to its start, so the loop is seamless.
            out[i if i < n else i - loop] += v
            env *= 0.99994
    return out


def fade(samples, fade_in, fade_out):
    a, b, n = seconds(fade_in), seconds(fade_out), len(samples)
    for i in range(a):
        samples[i] *= i / a
    for i in range(b):
        samples[n - 1 - i] *= i / b
    return samples


def write_wav(path, samples):
    frames = b"".join(struct.pack("<h", max(-32767, min(32767, int(round(v * 32767))))) for v in samples)
    with wave.open(path, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes(frames)


def main():
    out = sys.argv[1]
    os.makedirs(os.path.join(out, "sounds"), exist_ok=True)
    os.makedirs(os.path.join(out, "music"), exist_ok=True)
    for name, make in (
        ("sizzle", sizzle),
        ("pop", pop),
        ("whoosh", whoosh),
        ("rumble", rumble),
        ("click", click),
        ("chime", chime),
    ):
        write_wav(os.path.join(out, "sounds", name + ".wav"), make())
    oggenc = shutil.which("oggenc")
    if oggenc is None:
        sys.exit("oggenc (vorbis-tools) is needed for music/night.ogg")
    with tempfile.TemporaryDirectory() as tmp:
        wav = os.path.join(tmp, "night.wav")
        write_wav(wav, night())
        target = os.path.join(out, "music", "night.ogg")
        subprocess.run(
            [oggenc, "--quiet", "--quality", "4", "--serial", "1", "--discard-comments", "-o", target, wav],
            check=True,
        )


if __name__ == "__main__":
    main()
