"""Shared icon artwork: the same drawing backs the tray icon and the .ico
asset used by the Start menu shortcut, so they can never drift apart."""

from PySide6.QtGui import QColor, QPainter, QPainterPath, QPixmap


def paint_icon(size: int) -> QPixmap:
    pixmap = QPixmap(size, size)
    pixmap.fill(QColor(0, 0, 0, 0))
    painter = QPainter(pixmap)
    painter.setRenderHint(QPainter.RenderHint.Antialiasing)
    margin = max(1, round(size / 16))
    radius = max(2, round(size / 4))
    shape = QPainterPath()
    shape.addRoundedRect(margin, margin, size - 2 * margin, size - 2 * margin, radius, radius)
    painter.fillPath(shape, QColor(255, 255, 255))
    painter.setPen(QColor(0, 0, 0, 60))
    painter.drawPath(shape)
    painter.setPen(QColor(0, 0, 0, 0))
    heights = [size * h / 64 for h in (16, 26, 36, 26, 16)]
    bar_w, gap = size * 6 / 64, size * 5 / 64
    total = len(heights) * bar_w + (len(heights) - 1) * gap
    x = (size - total) / 2
    for h in heights:
        bar = QPainterPath()
        bar.addRoundedRect(x, size / 2 - h / 2, bar_w, h, bar_w / 2, bar_w / 2)
        painter.fillPath(bar, QColor(0, 0, 0))
        x += bar_w + gap
    painter.end()
    return pixmap
