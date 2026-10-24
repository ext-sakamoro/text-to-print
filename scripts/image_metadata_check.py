#!/usr/bin/env python3
"""追跡している PNG / JPEG が撮影・表示の環境の情報を持たないことの検査

- PNG: 許す chunk は IHDR / PLTE / tRNS / sRGB / gAMA / cHRM / IDAT / IEND だけ それ以外
  (iCCP の表示の profile、eXIf、iTXt / tEXt / zTXt の XMP や文字、tIME、cICP、私的な chunk)
  があれば fail 色は sRGB に変換して sRGB chunk で示す
- JPEG: 許す marker は SOI / APP0 (縮小画像の無い 14 byte の JFIF) / APP14 (12 byte の Adobe) / DQT / SOFn / DHT / DAC / DRI / SOS / RSTn /
  EOI だけ 全ての scan を通って EOI まで読み、それ以外の marker (APP1 の Exif / XMP、APP2 の ICC、他の
  APPn、COM) と EOI の後の bytes を fail にする
- PNG / JPEG 以外の画像 (gif / webp / svg / bmp / tiff / heic / avif / ico) は読めないので fail
- 読めない形 (signature が無い、chunk / segment が途中で切れる、EOI が無い) も fail
- 検査した画像が 0 件なら fail (検査の空振りを green にしない)

usage: image_metadata_check.py [--root DIR]
試験は scripts/test_image_metadata_check.py
"""

from __future__ import annotations

import argparse
import struct
import subprocess
import sys
from pathlib import Path

PNG_SIG = b"\x89PNG\r\n\x1a\n"
PNG_ALLOWED = {b"IHDR", b"PLTE", b"tRNS", b"sRGB", b"gAMA", b"cHRM", b"IDAT", b"IEND"}
# JPEG で許す marker (それ以外は fail): SOI / EOI、APP0 は JFIF、APP14 は Adobe の色変換の印だけ
JPEG_SOF = {0xC0, 0xC1, 0xC2, 0xC3, 0xC5, 0xC6, 0xC7, 0xC9, 0xCA, 0xCB, 0xCD, 0xCE, 0xCF}
JPEG_TABLES = {0xC4: "DHT", 0xCC: "DAC", 0xDB: "DQT", 0xDD: "DRI"}
IMAGE_SUFFIXES = (".png", ".jpg", ".jpeg")
# 読めない画像の形式: 追跡していれば fail (黙って検査から外さない)
OTHER_IMAGE_SUFFIXES = (".gif", ".webp", ".svg", ".bmp", ".tif", ".tiff", ".heic", ".heif", ".avif", ".ico")


def png_problems(data: bytes) -> list[str]:
    if not data.startswith(PNG_SIG):
        return ["PNG の signature が無い"]
    out: list[str] = []
    i = len(PNG_SIG)
    while i < len(data):
        if i + 8 > len(data):
            return out + ["chunk の header が途中で切れている"]
        n = struct.unpack(">I", data[i:i + 4])[0]
        kind = data[i + 4:i + 8]
        if i + 12 + n > len(data):
            return out + [f"chunk {kind!r} が途中で切れている"]
        if kind not in PNG_ALLOWED:
            out.append(f"chunk {kind.decode('latin-1')}")
        i += 12 + n
    return out


def jpeg_problems(data: bytes) -> list[str]:
    if not data.startswith(b"\xff\xd8"):
        return ["JPEG の signature が無い"]
    out: list[str] = []
    i = 2
    while True:
        if i + 2 > len(data):
            return out + ["EOI まで読めない"]
        if data[i] != 0xFF:
            return out + ["segment の marker が読めない"]
        marker = data[i + 1]
        if marker == 0xFF:  # fill byte
            i += 1
            continue
        if marker == 0xD9:  # EOI
            if i + 2 != len(data):
                out.append(f"EOI の後に {len(data) - i - 2} byte")
            return out
        if 0xD0 <= marker <= 0xD7:  # RSTn (scan の外に現れても長さは無い)
            i += 2
            continue
        if i + 4 > len(data):
            return out + ["segment の長さが読めない"]
        length = struct.unpack(">H", data[i + 2:i + 4])[0]
        body = data[i + 4:i + 2 + length]
        if length < 2 or i + 2 + length > len(data):
            return out + [f"segment 0x{marker:02X} が途中で切れている"]
        if marker == 0xE0:
            # JFIF: "JFIF\0" + 版 2 + 単位 1 + 密度 4 + 縮小画像の幅 1 + 高さ 1 = 14 byte、縮小画像は持たない
            if not (len(body) == 14 and body.startswith(b"JFIF\x00") and body[12] == 0 and body[13] == 0):
                out.append("APP0 (縮小画像の無い 14 byte の JFIF でない)")
        elif marker == 0xEE:
            # Adobe: "Adobe" + 版 2 + flags0 2 + flags1 2 + 色変換 1 = 12 byte
            if not (len(body) == 12 and body.startswith(b"Adobe")):
                out.append("APP14 (12 byte の Adobe でない)")
        elif marker == 0xDA:  # SOS: header の後は画素の data、次の marker (RSTn と 0xFF00 を除く) まで飛ばす
            i += 2 + length
            while i + 1 < len(data) and not (data[i] == 0xFF and data[i + 1] not in (0x00, *range(0xD0, 0xD8))):
                i += 1
            continue
        elif marker not in JPEG_SOF and marker not in JPEG_TABLES:
            out.append(f"marker 0x{marker:02X}")
        i += 2 + length


def problems(path: str, data: bytes) -> list[str]:
    name = path.lower()
    if name.endswith(".png"):
        return png_problems(data)
    if name.endswith((".jpg", ".jpeg")):
        return jpeg_problems(data)
    return ["読めない画像の形式 (PNG か JPEG にする)"]


def tracked_images(root: Path) -> list[str]:
    files = subprocess.run(["git", "-C", str(root), "ls-files"], capture_output=True, text=True, check=True)
    return [f for f in files.stdout.splitlines() if f.lower().endswith(IMAGE_SUFFIXES + OTHER_IMAGE_SUFFIXES)]


def main() -> int:
    # Windows の console の既定 encoding では日本語の出力が化けるので UTF-8 に固定する
    for stream in (sys.stdout, sys.stderr):
        stream.reconfigure(encoding="utf-8")
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", default=".")
    root = Path(ap.parse_args().root)
    images = tracked_images(root)
    bad = 0
    for f in images:
        for p in problems(f, (root / f).read_bytes()):
            print(f"{f}: {p}")
            bad += 1
    if not images:
        print("image_metadata_check: 検査した画像が 0 件", file=sys.stderr)
        return 1
    print(f"image_metadata_check: {len(images)} image(s), {bad} problem(s)")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
