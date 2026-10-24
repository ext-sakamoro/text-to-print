#!/usr/bin/env python3
"""scripts/image_metadata_check.py の試験 (画像は試験の中で bytes から作る)"""

from __future__ import annotations

import struct
import subprocess
import sys
import tempfile
import unittest
import zlib
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import image_metadata_check as imc  # noqa: E402

SCRIPT = Path(__file__).resolve().parent / "image_metadata_check.py"


def chunk(kind: bytes, data: bytes) -> bytes:
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)


def png(*extra: bytes) -> bytes:
    ihdr = chunk(b"IHDR", struct.pack(">IIBBBBB", 1, 1, 8, 2, 0, 0, 0))
    idat = chunk(b"IDAT", zlib.compress(b"\x00\x00\x00\x00"))
    return imc.PNG_SIG + ihdr + b"".join(extra) + idat + chunk(b"IEND", b"")


def seg(marker: int, data: bytes) -> bytes:
    return b"\xff" + bytes([marker]) + struct.pack(">H", len(data) + 2) + data


SCAN = seg(0xDA, b"\x01\x01\x00\x00\x3f\x00") + b"\x12\x34\xff\x00\x56\xff\xd0\x78"  # 0xFF00 と RST0 を含む


JFIF = b"JFIF\x00\x01\x01\x00\x00\x01\x00\x01\x00\x00"  # 14 byte、縮小画像 0×0
ADOBE = b"Adobe\x00\x64\x00\x00\x00\x00\x01"  # 12 byte


def jpeg(*segments: tuple[int, bytes], after_scan: bytes = b"", trailer: bytes = b"") -> bytes:
    body = b"".join(seg(m, d) for m, d in segments)
    return b"\xff\xd8" + seg(0xE0, JFIF) + body + SCAN + after_scan + b"\xff\xd9" + trailer


class Png(unittest.TestCase):
    def test_core_chunks_and_srgb_pass(self):
        self.assertEqual(imc.png_problems(png(chunk(b"sRGB", b"\x00"))), [])

    def test_each_metadata_chunk_is_refused(self):
        for kind in (b"iCCP", b"eXIf", b"iTXt", b"tEXt", b"zTXt", b"tIME", b"cICP", b"iDOT"):
            with self.subTest(kind=kind):
                self.assertEqual(imc.png_problems(png(chunk(kind, b"x"))), [f"chunk {kind.decode()}"])

    def test_a_cut_chunk_is_refused(self):
        self.assertTrue(imc.png_problems(png()[:-6]))

    def test_no_signature_is_refused(self):
        self.assertEqual(imc.png_problems(b"GIF89a"), ["PNG の signature が無い"])


class Jpeg(unittest.TestCase):
    def test_jfif_with_tables_passes(self):
        self.assertEqual(imc.jpeg_problems(jpeg((0xDB, b"\x00" * 65), (0xC0, b"\x08" * 9), (0xC4, b"\x00" * 20))), [])

    def test_adobe_app14_passes(self):
        self.assertEqual(imc.jpeg_problems(jpeg((0xEE, ADOBE))), [])

    def test_text_inside_an_adobe_segment_is_refused(self):
        self.assertEqual(imc.jpeg_problems(jpeg((0xEE, b"Adobe\x00camera serial 12345"))), ["APP14 (12 byte の Adobe でない)"])

    def test_text_after_the_jfif_header_is_refused(self):
        r = imc.jpeg_problems(b"\xff\xd8" + seg(0xE0, JFIF + b" owner name") + SCAN + b"\xff\xd9")
        self.assertEqual(r, ["APP0 (縮小画像の無い 14 byte の JFIF でない)"])

    def test_a_jfif_thumbnail_is_refused(self):
        thumb = JFIF[:12] + b"\x01\x01" + b"\x00\x00\x00"
        r = imc.jpeg_problems(b"\xff\xd8" + seg(0xE0, thumb) + SCAN + b"\xff\xd9")
        self.assertEqual(r, ["APP0 (縮小画像の無い 14 byte の JFIF でない)"])

    def test_every_other_marker_before_the_scan_is_refused(self):
        # APP1 Exif / XMP、APP2 ICC、APP12 Ducky、APP13 IPTC、APP15、COM
        for marker in (0xE1, 0xE2, 0xEC, 0xED, 0xEF, 0xFE):
            with self.subTest(marker=hex(marker)):
                self.assertEqual(imc.jpeg_problems(jpeg((marker, b"camera serial 1234"))), [f"marker 0x{marker:02X}"])

    def test_a_marker_between_the_scan_data_and_eoi_is_refused(self):
        self.assertEqual(imc.jpeg_problems(jpeg(after_scan=seg(0xFE, b"note"))), ["marker 0xFE"])

    def test_tables_and_a_second_scan_pass_but_a_comment_between_scans_is_refused(self):
        tables_then_scan = seg(0xC4, b"\x00" * 20) + SCAN
        self.assertEqual(imc.jpeg_problems(jpeg(after_scan=tables_then_scan)), [])
        self.assertEqual(imc.jpeg_problems(jpeg(after_scan=seg(0xFE, b"x") + SCAN)), ["marker 0xFE"])

    def test_bytes_after_eoi_are_refused(self):
        self.assertEqual(imc.jpeg_problems(jpeg(trailer=b"Exif camera model")), ["EOI の後に 17 byte"])

    def test_app0_that_is_not_jfif_is_refused(self):
        self.assertEqual(imc.jpeg_problems(jpeg((0xE0, b"JFXX\x00\x10"))), ["APP0 (縮小画像の無い 14 byte の JFIF でない)"])

    def test_a_stream_without_eoi_is_refused(self):
        self.assertTrue(imc.jpeg_problems(jpeg()[:-2]))

    def test_no_signature_is_refused(self):
        self.assertEqual(imc.jpeg_problems(b"\x00\x00"), ["JPEG の signature が無い"])


class Cli(unittest.TestCase):
    def run_in(self, files: dict[str, bytes]) -> subprocess.CompletedProcess[str]:
        tmp = tempfile.mkdtemp()
        subprocess.run(["git", "init", "-q", tmp], check=True)
        for name, data in files.items():
            Path(tmp, name).write_bytes(data)
            subprocess.run(["git", "-C", tmp, "add", name], check=True)
        return subprocess.run([sys.executable, str(SCRIPT), "--root", tmp], capture_output=True, text=True, encoding="utf-8")

    def test_clean_images_pass(self):
        r = self.run_in({"a.png": png(chunk(b"sRGB", b"\x00")), "b.jpg": jpeg()})
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        self.assertIn("2 image(s), 0 problem(s)", r.stdout)

    def test_a_profile_fails(self):
        r = self.run_in({"a.png": png(chunk(b"iCCP", b"Display\x00\x00x"))})
        self.assertEqual(r.returncode, 1)
        self.assertIn("a.png: chunk iCCP", r.stdout)

    def test_another_image_type_fails(self):
        r = self.run_in({"a.gif": b"GIF89a"})
        self.assertEqual(r.returncode, 1)
        self.assertIn("a.gif: 読めない画像の形式", r.stdout)

    def test_no_image_fails(self):
        r = self.run_in({"readme.txt": b"text"})
        self.assertEqual(r.returncode, 1)
        self.assertIn("0 件", r.stderr)


if __name__ == "__main__":
    unittest.main(verbosity=2)
