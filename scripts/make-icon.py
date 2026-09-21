"""Renders voice_input.icon.paint_icon() at several sizes and packs them into
a single multi-resolution assets/claudetalk.ico, so the Start menu shortcut
shows the exact same artwork as the tray icon.

Run with the project's own venv:
    voice-input\\.venv\\Scripts\\python.exe scripts\\make-icon.py
"""

import struct
import sys
from io import BytesIO
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "voice-input"))

from PySide6.QtGui import QGuiApplication, QImage  # noqa: E402

import icon  # noqa: E402

SIZES = [16, 32, 48, 64, 128, 256]


def _png_bytes(size: int) -> bytes:
    pixmap = icon.paint_icon(size)
    image: QImage = pixmap.toImage()
    buf = BytesIO()
    from PySide6.QtCore import QBuffer, QByteArray

    qbuf = QBuffer()
    qbuf.open(QBuffer.OpenModeFlag.WriteOnly)
    image.save(qbuf, "PNG")
    buf.write(bytes(qbuf.data()))
    qbuf.close()
    return buf.getvalue()


def build_ico(out_path: Path) -> None:
    entries = [(size, _png_bytes(size)) for size in SIZES]

    header = struct.pack("<HHH", 0, 1, len(entries))
    dir_entries = b""
    image_data = b""
    offset = 6 + 16 * len(entries)

    for size, png in entries:
        w = size if size < 256 else 0  # 0 means 256 in ICO format
        h = size if size < 256 else 0
        dir_entries += struct.pack(
            "<BBBBHHII", w, h, 0, 0, 1, 32, len(png), offset
        )
        image_data += png
        offset += len(png)

    out_path.write_bytes(header + dir_entries + image_data)


if __name__ == "__main__":
    app = QGuiApplication.instance() or QGuiApplication(sys.argv)
    out = ROOT / "assets" / "claudetalk.ico"
    out.parent.mkdir(exist_ok=True)
    build_ico(out)
    print(f"Written {out} ({out.stat().st_size} bytes)")
