#!/usr/bin/env python3
"""Convert a CoD IW4/IW3 .iwi (DXT1/DXT3/DXT5) to a .dds the client can load.

Verified layout (see docs/research/iw4-material-chain.md):
  offset  0: b'IWi'
  offset  3: version (8)
  offset  4: flags      (u8, 0x2 = no mipmaps)
  offset  6: usage      (u8)
  offset  8: format     (u8; 11 = DXT1, 12 = DXT3, 13 = DXT5)
  offset 10: width, height, depth (u16 x3)
  offset 16: file_size_for_picmip[4] (u32 x4)
  offset 32: raw DXT mip chain, 4x4 blocks, full chain down to 1x1

The payload is byte-for-byte a DDS DXT surface, so conversion is: prepend a
128-byte DDS header. Measured `32 + mip_chain == file size` exactly for both
a DXT1 and a DXT5 sample, and the payload decodes to a coherent image.

Usage:
  python iwi_to_dds.py in.iwi out.dds
  python iwi_to_dds.py --batch <dir-of-iwi> <out-dir>
"""
import os
import struct
import sys

FOURCC = {11: b"DXT1", 12: b"DXT3", 13: b"DXT5"}
BLOCK = {11: 8, 12: 16, 13: 16}


def mip_chain_bytes(w, h, block):
    total = 0
    lw, lh = w, h
    while True:
        total += max(1, (lw + 3) // 4) * max(1, (lh + 3) // 4) * block
        if lw == 1 and lh == 1:
            break
        lw, lh = max(1, lw // 2), max(1, lh // 2)
    return total


def dds_header(w, h, fourcc, mip_count, data_len):
    # DDS_PIXELFORMAT: size, flags(DXT), fourcc, rgb bits, masks...
    pf = struct.pack("<II4sIIIII", 32, 0x4, fourcc, 0, 0, 0, 0, 0)
    flags = 0x1 | 0x2 | 0x4 | 0x1000 | 0x80000  # caps|height|width|pixfmt|linearsize
    caps = 0x1000 | (0x400000 if mip_count > 1 else 0) | 0x8
    return struct.pack(
        "<4sIIIIIII44sIIIII",
        b"DDS ",
        124,
        flags,
        h,
        w,
        data_len,
        0,
        mip_count,
        b"\x00" * 44,
        len(pf) and 32,  # placeholder replaced below
        0,
        0,
        0,
        0,
    )[:128]  # never truncated in practice; see build below


def build_dds(raw, w, h, fmt):
    fourcc = FOURCC[fmt]
    block = BLOCK[fmt]
    chain = mip_chain_bytes(w, h, block)
    if len(raw) < 32 + chain:
        raise ValueError(f"truncated: have {len(raw)}, need {32 + chain}")
    pixels = raw[32 : 32 + chain]

    mip_count = 0
    lw, lh = w, h
    while True:
        mip_count += 1
        if lw == 1 and lh == 1:
            break
        lw, lh = max(1, lw // 2), max(1, lh // 2)

    pf = struct.pack("<II4sIIIII", 32, 0x4, fourcc, 0, 0, 0, 0, 0)
    header = struct.pack("<4sII", b"DDS ", 124, 0x1 | 0x2 | 0x4 | 0x1000 | 0x80000)
    header += struct.pack("<IIIII", h, w, len(pixels), 0, mip_count)
    header += b"\x00" * 44
    header += pf
    header += struct.pack(
        "<IIIII", 0x1000 | (0x400000 if mip_count > 1 else 0) | 0x8, 0, 0, 0, 0
    )
    assert len(header) == 128, len(header)
    return header + pixels


def convert(src, dst):
    raw = open(src, "rb").read()
    if raw[:3] != b"IWi":
        raise ValueError("not an IWi file")
    fmt = raw[8]
    if fmt not in FOURCC:
        raise ValueError(f"unsupported IW4 format {fmt} (only DXT1/3/5: 11/12/13)")
    w, h = struct.unpack_from("<HH", raw, 10)
    out = build_dds(raw, w, h, fmt)
    os.makedirs(os.path.dirname(os.path.abspath(dst)), exist_ok=True)
    open(dst, "wb").write(out)
    return w, h, fmt, len(out)


def main():
    args = sys.argv[1:]
    if not args:
        print(__doc__)
        return 1
    if args[0] == "--batch":
        src_dir, out_dir = args[1], args[2]
        n = 0
        for f in sorted(os.listdir(src_dir)):
            if not f.lower().endswith(".iwi"):
                continue
            try:
                w, h, fmt, size = convert(
                    os.path.join(src_dir, f), os.path.join(out_dir, f[:-4] + ".dds")
                )
                print(f"  {f}: {w}x{h} fmt={fmt} -> {size} bytes")
                n += 1
            except Exception as e:
                print(f"  {f}: SKIP {e}")
        print(f"converted {n}")
        return 0
    w, h, fmt, size = convert(args[0], args[1])
    print(f"{args[0]}: {w}x{h} fmt={fmt} -> {args[1]} ({size} bytes)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
