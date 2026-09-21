"""Overlay flotante "liquid glass" que aparece mientras se graba.

Blanco y negro, sin robar foco (critico: si esta ventana se activara,
GetForegroundWindow() dejaria de apuntar a la ventana real y el paste
fallaria o el texto se pegaria en el overlay). Se logra con flags de Qt
mas los estilos nativos WS_EX_NOACTIVATE/WS_EX_TOOLWINDOW por las dudas.

El efecto de vidrio esmerilado NO usa el backdrop nativo de Windows
(DWMWA_SYSTEMBACKDROP_TYPE / SetWindowCompositionAttribute): ambas APIs se
probaron y solo devuelven un panel solido sin blur cuando la ventana tiene
contenido pintado a mano por Qt en vez de ser una app WinUI3 pura, que es
para lo que estan pensadas. En cambio, se captura la region de pantalla
donde va a aparecer la pildora justo antes de mostrarla y se desenfoca a
mano (downscale + upscale), en escala de grises: vidrio esmerilado real,
sin depender de una API nativa fragil.
"""

import ctypes
import sys

from PySide6.QtCore import QObject, Property, QPropertyAnimation, Qt, Signal
from PySide6.QtGui import QColor, QImage, QPainter, QPainterPath, QPixmap
from PySide6.QtWidgets import QApplication, QWidget

WS_EX_NOACTIVATE = 0x08000000
WS_EX_TOOLWINDOW = 0x00000080
GWL_EXSTYLE = -20


def _apply_native_overlay_styles(hwnd: int):
    user32 = ctypes.windll.user32
    style = user32.GetWindowLongW(hwnd, GWL_EXSTYLE)
    user32.SetWindowLongW(hwnd, GWL_EXSTYLE, style | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW)


BAR_COUNT = 5
BAR_MIN_HEIGHT = 6
BAR_MAX_HEIGHT = 28
WIDTH = 160
HEIGHT = 56


class RecordingOverlay(QWidget):
    """Pildora flotante blanco y negro con barras que reaccionan al volumen."""

    def __init__(self):
        super().__init__(
            None,
            Qt.FramelessWindowHint
            | Qt.WindowStaysOnTopHint
            | Qt.Tool
            | Qt.WindowDoesNotAcceptFocus
            | Qt.WindowTransparentForInput,
        )
        self.setAttribute(Qt.WA_TranslucentBackground)
        self.setAttribute(Qt.WA_ShowWithoutActivating)
        self.resize(WIDTH, HEIGHT)
        self._levels = [0.0] * BAR_COUNT
        self._opacity = 0.0
        self._bg_pixmap = None
        self._fade = QPropertyAnimation(self, b"windowOpacity_")
        self._fade.setDuration(140)
        self._place_bottom_center()

    def _place_bottom_center(self):
        screen = QApplication.primaryScreen().availableGeometry()
        x = screen.x() + (screen.width() - WIDTH) // 2
        y = screen.y() + screen.height() - HEIGHT - 48
        self.move(x, y)

    def showEvent(self, event):
        super().showEvent(event)
        hwnd = int(self.winId())
        _apply_native_overlay_styles(hwnd)

    def _capture_blurred_background(self):
        """Saca una foto de lo que hay detras de donde va a aparecer la
        pildora (la ventana todavia esta oculta en este punto) y la
        desenfoca con un downscale+upscale barato, en escala de grises."""
        geo = self.geometry()
        screen = QApplication.screenAt(geo.center()) or QApplication.primaryScreen()
        pixmap = screen.grabWindow(0, geo.x(), geo.y(), geo.width(), geo.height())
        gray = pixmap.toImage().convertToFormat(QImage.Format.Format_Grayscale8)

        factor = 10
        tiny = gray.scaled(
            max(1, gray.width() // factor),
            max(1, gray.height() // factor),
            Qt.AspectRatioMode.IgnoreAspectRatio,
            Qt.TransformationMode.SmoothTransformation,
        )
        blurred = tiny.scaled(
            gray.width(),
            gray.height(),
            Qt.AspectRatioMode.IgnoreAspectRatio,
            Qt.TransformationMode.SmoothTransformation,
        )
        self._bg_pixmap = QPixmap.fromImage(blurred)

    def fade_in(self):
        self._place_bottom_center()
        self._capture_blurred_background()
        self.show()
        self._fade.stop()
        self._fade.setStartValue(self.windowOpacity())
        self._fade.setEndValue(1.0)
        self._fade.start()

    def fade_out(self):
        self._fade.stop()
        self._fade.setStartValue(self.windowOpacity())
        self._fade.setEndValue(0.0)
        self._fade.finished.connect(self.hide)
        self._fade.start()

    def set_level(self, normalized: float):
        """normalized en [0, 1]. Desplaza las barras como un ecualizador simple."""
        normalized = max(0.0, min(1.0, normalized))
        self._levels = self._levels[1:] + [normalized]
        self.update()

    def paintEvent(self, event):
        painter = QPainter(self)
        painter.setRenderHint(QPainter.Antialiasing)

        path = QPainterPath()
        path.addRoundedRect(0, 0, self.width(), self.height(), self.height() / 2, self.height() / 2)

        painter.setClipPath(path)
        if self._bg_pixmap is not None:
            painter.drawPixmap(0, 0, self._bg_pixmap)
        painter.fillPath(path, QColor(0, 0, 0, 150))
        painter.setClipping(False)

        painter.setPen(QColor(255, 255, 255, 70))
        painter.drawPath(path)

        gap = 8
        bar_width = 6
        total_w = BAR_COUNT * bar_width + (BAR_COUNT - 1) * gap
        start_x = (self.width() - total_w) / 2
        center_y = self.height() / 2

        painter.setPen(Qt.NoPen)
        for i, level in enumerate(self._levels):
            bar_h = BAR_MIN_HEIGHT + level * (BAR_MAX_HEIGHT - BAR_MIN_HEIGHT)
            x = start_x + i * (bar_width + gap)
            y = center_y - bar_h / 2
            bar_path = QPainterPath()
            bar_path.addRoundedRect(x, y, bar_width, bar_h, bar_width / 2, bar_width / 2)
            painter.fillPath(bar_path, QColor(255, 255, 255, 235))

    def _get_opacity(self):
        return self.windowOpacity()

    def _set_opacity(self, value):
        self.setWindowOpacity(value)

    windowOpacity_ = Property(float, _get_opacity, _set_opacity)


class OverlayBridge(QObject):
    """Puente thread-safe: los hilos de trabajo emiten senales, el overlay
    vive y se pinta en el hilo de la GUI (el que corre QApplication.exec())."""

    recording_started = Signal()
    recording_stopped = Signal()
    level_changed = Signal(float)

    def __init__(self, overlay: RecordingOverlay):
        super().__init__()
        self._overlay = overlay
        self.recording_started.connect(overlay.fade_in)
        self.recording_stopped.connect(overlay.fade_out)
        self.level_changed.connect(overlay.set_level)


def create_app_and_overlay():
    app = QApplication.instance() or QApplication(sys.argv)
    overlay = RecordingOverlay()
    bridge = OverlayBridge(overlay)
    return app, bridge
