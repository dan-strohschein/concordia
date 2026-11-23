#!/usr/bin/env bash
# Generates tests/fixtures/audio/tone-440.ogg (M2 2.7a): 1 s of a 440 Hz sine at amplitude 0.5
# (-6.0206 dBFS), mono, 48 kHz, written as 16-bit PCM by Python's stdlib, then encoded by oggenc
# (vorbis-tools). Run from the repository root; needs python3 and oggenc.
set -euo pipefail
OUT="tests/fixtures/audio"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
python3 -I - "$TMP/tone-440.wav" <<'PY'
import math, struct, sys, wave
rate, n = 48000, 48000
with wave.open(sys.argv[1], "wb") as w:
    w.setnchannels(1); w.setsampwidth(2); w.setframerate(rate)
    w.writeframes(b"".join(struct.pack("<h", round(16384 * math.sin(2 * math.pi * 440 * i / rate))) for i in range(n)))
PY
mkdir -p "$OUT"
oggenc --quiet --quality 6 --serial 1 --discard-comments -o "$OUT/tone-440.ogg" "$TMP/tone-440.wav"
echo "wrote $OUT/tone-440.ogg"
