#!/usr/bin/env python3
"""Probe the WaW fastfile container: decompress and scan for asset structures.

Read-only. Answers: (1) is the .ff a zlib stream we can read; (2) what does the
decompressed data look like; (3) can we find xmodel descriptors and their
version numbers, to decide whether vcod's v14 parsers could ever read them.
"""
import os, struct, sys, zlib, re

WAW = r"C:\Program Files (x86)\Steam\steamapps\common\Call of Duty World at War\zone\english"


def load_stream(name):
    path = os.path.join(WAW, name)
    raw = open(path, "rb").read()
    assert raw[:8] == b"IWffu100", raw[:8]
    d = zlib.decompressobj()
    out = d.decompress(raw[12:])
    return raw, out


def main():
    name = sys.argv[1] if len(sys.argv) > 1 else "common_mp.ff"
    raw, out = load_stream(name)
    print(f"{name}: {len(raw)} raw -> {len(out)} decompressed")
    print("first 64:", out[:64].hex())

    # scan for u16 == 14 (xmodel v14?) at 2-byte aligned offsets, where the next
    # 24 bytes look like plausible small bounds.
    hits = []
    for off in range(0, len(out) - 32, 2):
        v = struct.unpack_from("<H", out, off)[0]
        if v in (14, 15, 16, 17, 18, 20, 25):
            mins = struct.unpack_from("<3f", out, off + 2)
            maxs = struct.unpack_from("<3f", out, off + 14)
            def sane(a):
                return all(abs(x) < 5000 for x in a)
            if sane(mins) and sane(maxs) and all(
                abs(maxs[i] - mins[i]) < 4000 for i in range(3)
            ):
                hits.append((off, v, mins, maxs))
    print(f"\nplausible version-tagged model-ish headers: {len(hits)}")
    for off, v, mins, maxs in hits[:20]:
        print(f"  off={off:#x} v={v} mins={mins} maxs={maxs}")

    # Look for readable ASCII strings that look like asset names.
    strings = re.findall(rb"[ -~]{6,}", out)
    print(f"\nASCII strings >= 6: {len(strings)}")
    for pat in (b"viewmodel", b"xmodel", b"weapon_", b"trench", b"shotgun", b"g43", b"flame"):
        m = [s for s in strings if pat in s.lower()]
        print(f"  {pat!r}: {len(m)}  e.g. {m[:4]}")


if __name__ == "__main__":
    main()
