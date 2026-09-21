"""Overlay flotante "liquid glass" que aparece mientras se graba.

Blanco y negro, sin robar foco (critico: si esta ventana se activara,
GetForegroundWindow() dejaria de apuntar a la ventana real y el paste
fallaria o el texto se pegaria en el overlay). Se logra con flags de Qt
mas el estilo nativo WS_EX_NOACTIVATE, que ademas permite clickear la
tuerca sin activar la ventana (mismo mecanismo que usa el teclado en
pantalla de Windows).

El efecto de vidrio esmerilado NO usa el backdrop nativo de Windows
(DWMWA_SYSTEMBACKDROP_TYPE / SetWindowCompositionAttribute): ambas APIs se
probaron y solo devuelven un panel solido sin blur cuando la ventana tiene
contenido pintado a mano por Qt en vez de ser una app WinUI3 pura, que es
para lo que estan pensadas. En cambio, se captura la region de pantalla
donde va a aparecer la pildora justo antes de mostrarla y se desenfoca a
mano (downscale + upscale), en escala de grises.

Animaciones: entrada con fundido + deslizamiento hacia arriba, salida
inversa; barras de volumen interpoladas (no saltan) con una "respiracion"
sutil cuando hay silencio para que se vea viva desde la pulsacion; tuerca
que gira al pasar el mouse y al clickearla (placeholder de ajustes).
"""

import ctypes
import math
import sys
import time

from PySide6.QtCore import QEasingCurve, QObject, QPointF, Qt, QTimer, QVariantAnimation, Signal
from PySide6.QtGui import QColor, QFont, QImage, QLinearGradient, QPainter, QPainterPath, QPixmap, QTransform
from PySide6.QtWidgets import QApplication, QWidget

WS_EX_NOACTIVATE = 0x08000000
WS_EX_TOOLWINDOW = 0x00000080
GWL_EXSTYLE = -20


def _apply_native_overlay_styles(hwnd: int):
    user32 = ctypes.windll.user32
    style = user32.GetWindowLongW(hwnd, GWL_EXSTYLE)
    user32.SetWindowLongW(hwnd, GWL_EXSTYLE, style | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW)


WIDTH = 200
HEIGHT = 56
SLIDE_PX = 14
BOTTOM_MARGIN = 40

BAR_COUNT = 5
BAR_WIDTH = 6
BAR_GAP = 8
BAR_MIN_HEIGHT = 6
BAR_MAX_HEIGHT = 30
BARS_AREA_WIDTH = WIDTH - 44  # deja lugar a la tuerca a la derecha

GEAR_CENTER_X = WIDTH - 24
GEAR_OUTER_R = 9.6
GEAR_BODY_R = 6.9
GEAR_TOOTH_W = 3.8
GEAR_HOLE_R = 3.1
GEAR_TEETH = 8
GEAR_HIT_R = 14

SHOW_MS = 220
HIDE_MS = 170
FRAME_MS = 16
NOTE_SECONDS = 1.4


def _lerp(a: float, b: float, t: float) -> float:
    return a + (b - a) * t


def _gear_path() -> QPainterPath:
    """Cuerpo circular + dientes redondeados (union booleana) menos el agujero
    central: bordes suaves y forma de tuerca estandar, sin poligono a mano."""
    body = QPainterPath()
    body.addEllipse(QPointF(0, 0), GEAR_BODY_R, GEAR_BODY_R)
    tooth_len = GEAR_OUTER_R - GEAR_BODY_R + 2.6
    for k in range(GEAR_TEETH):
        tooth = QPainterPath()
        tooth.addRoundedRect(-GEAR_TOOTH_W / 2, -GEAR_OUTER_R, GEAR_TOOTH_W, tooth_len, 1.5, 1.5)
        rotation = QTransform()
        rotation.rotate(k * 360.0 / GEAR_TEETH)
        body = body.united(rotation.map(tooth))
    hole = QPainterPath()
    hole.addEllipse(QPointF(0, 0), GEAR_HOLE_R, GEAR_HOLE_R)
    return body.subtracted(hole)


class RecordingOverlay(QWidget):
    """Pildora flotante blanco y negro con barras que reaccionan al volumen."""

    settings_clicked = Signal()

    def __init__(self):
        super().__init__(
            None,
            Qt.FramelessWindowHint | Qt.WindowStaysOnTopHint | Qt.Tool | Qt.WindowDoesNotAcceptFocus,
        )
        self.setAttribute(Qt.WA_TranslucentBackground)
        self.setAttribute(Qt.WA_ShowWithoutActivating)
        self.setMouseTracking(True)
        self.resize(WIDTH, HEIGHT + SLIDE_PX)

        self._levels_target = [0.0] * BAR_COUNT
        self._levels_shown = [0.0] * BAR_COUNT
        self._phase = 0.0
        self._slide = 1.0
        self._bg_pixmap = None
        self._gear_path = _gear_path()
        self._gear_hover = False
        self._gear_hover_t = 0.0
        self._gear_angle = 0.0
        self._gear_angle_target = 0.0
        self._gear_clicks = 0
        self._note = None
        self._note_until = 0.0
        self._closing = False

        self._frame = QTimer(self)
        self._frame.setInterval(FRAME_MS)
        self._frame.timeout.connect(self._tick)

        self._anim = QVariantAnimation(self)
        self._anim.valueChanged.connect(self._on_progress)
        self._anim.finished.connect(self._on_anim_finished)

        _apply_native_overlay_styles(int(self.winId()))
        self._place_bottom_center()

    # --- posicion / fondo ---------------------------------------------------

    def _place_bottom_center(self):
        screen = QApplication.primaryScreen().availableGeometry()
        x = screen.x() + (screen.width() - WIDTH) // 2
        y = screen.y() + screen.height() - self.height() - BOTTOM_MARGIN
        self.move(x, y)

    def showEvent(self, event):
        super().showEvent(event)
        _apply_native_overlay_styles(int(self.winId()))

    def _capture_blurred_background(self):
        """Foto de lo que hay detras (la ventana todavia esta oculta aca),
        desenfocada con downscale+upscale barato, en escala de grises."""
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
            gray.width(), gray.height(), Qt.AspectRatioMode.IgnoreAspectRatio, Qt.TransformationMode.SmoothTransformation
        )
        result = QPixmap.fromImage(blurred)
        result.setDevicePixelRatio(pixmap.devicePixelRatio())
        self._bg_pixmap = result

    # --- entrada / salida ---------------------------------------------------

    def fade_in(self):
        self._closing = False
        self._note = None
        self._levels_target = [0.0] * BAR_COUNT
        self._levels_shown = [0.0] * BAR_COUNT
        if not self.isVisible():
            self._place_bottom_center()
            self._capture_blurred_background()
            self.setWindowOpacity(0.0)
            self._slide = 1.0
            self.show()
        self._frame.start()
        self._anim.stop()
        self._anim.setDuration(SHOW_MS)
        self._anim.setEasingCurve(QEasingCurve.Type.OutCubic)
        self._anim.setStartValue(self.windowOpacity())
        self._anim.setEndValue(1.0)
        self._anim.start()

    def fade_out(self):
        if not self.isVisible():
            return
        self._closing = True
        self._anim.stop()
        self._anim.setDuration(HIDE_MS)
        self._anim.setEasingCurve(QEasingCurve.Type.InCubic)
        self._anim.setStartValue(self.windowOpacity())
        self._anim.setEndValue(0.0)
        self._anim.start()

    def _on_progress(self, value):
        self.setWindowOpacity(float(value))
        self._slide = 1.0 - float(value)
        self.update()

    def _on_anim_finished(self):
        if self._closing:
            self._frame.stop()
            self.hide()

    # --- estado animado -----------------------------------------------------

    def set_level(self, normalized: float):
        """normalized en [0, 1]. Desplaza las barras como un ecualizador simple."""
        normalized = max(0.0, min(1.0, normalized))
        self._levels_target = self._levels_target[1:] + [normalized]

    def _tick(self):
        self._phase += 0.11
        for i in range(BAR_COUNT):
            self._levels_shown[i] = _lerp(self._levels_shown[i], self._levels_target[i], 0.35)
        self._gear_hover_t = _lerp(self._gear_hover_t, 1.0 if self._gear_hover else 0.0, 0.25)
        self._gear_angle = _lerp(self._gear_angle, self._gear_angle_target, 0.18)
        if self._note is not None and time.monotonic() > self._note_until:
            self._note = None
        self.update()

    # --- mouse (tuerca) -----------------------------------------------------

    def _gear_center(self) -> QPointF:
        return QPointF(GEAR_CENTER_X, self._pill_top() + HEIGHT / 2)

    def _pill_top(self) -> float:
        return self._slide * SLIDE_PX

    def _over_gear(self, pos) -> bool:
        c = self._gear_center()
        return math.hypot(pos.x() - c.x(), pos.y() - c.y()) <= GEAR_HIT_R

    def mouseMoveEvent(self, event):
        hover = self._over_gear(event.position())
        if hover != self._gear_hover:
            self._gear_hover = hover
            self._gear_angle_target = self._gear_clicks * 90.0 + (30.0 if hover else 0.0)
            self.setCursor(Qt.CursorShape.PointingHandCursor if hover else Qt.CursorShape.ArrowCursor)

    def leaveEvent(self, event):
        self._gear_hover = False
        self._gear_angle_target = self._gear_clicks * 90.0
        self.setCursor(Qt.CursorShape.ArrowCursor)

    def mousePressEvent(self, event):
        if event.button() == Qt.MouseButton.LeftButton and self._over_gear(event.position()):
            self._gear_clicks += 1
            self._gear_angle_target = self._gear_clicks * 90.0 + 30.0
            self._note = "ajustes: pronto"
            self._note_until = time.monotonic() + NOTE_SECONDS
            self.settings_clicked.emit()

    # --- pintura ------------------------------------------------------------

    def paintEvent(self, event):
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)

        top = self._pill_top()
        radius = HEIGHT / 2
        pill = QPainterPath()
        pill.addRoundedRect(0.0, top, float(WIDTH), float(HEIGHT), radius, radius)

        painter.setClipPath(pill)
        if self._bg_pixmap is not None:
            painter.drawPixmap(0, 0, self._bg_pixmap)
        painter.fillPath(pill, QColor(255, 255, 255, 178))

        sheen = QLinearGradient(0, top, 0, top + HEIGHT * 0.5)
        sheen.setColorAt(0.0, QColor(255, 255, 255, 120))
        sheen.setColorAt(1.0, QColor(255, 255, 255, 0))
        painter.fillPath(pill, sheen)

        depth = QLinearGradient(0, top + HEIGHT * 0.55, 0, top + HEIGHT)
        depth.setColorAt(0.0, QColor(0, 0, 0, 0))
        depth.setColorAt(1.0, QColor(0, 0, 0, 22))
        painter.fillPath(pill, depth)
        painter.setClipping(False)

        painter.setBrush(Qt.BrushStyle.NoBrush)
        painter.setPen(QColor(255, 255, 255, 190))
        painter.drawPath(pill)
        inner = QPainterPath()
        inner.addRoundedRect(1.0, top + 1.0, WIDTH - 2.0, HEIGHT - 2.0, radius - 1.0, radius - 1.0)
        painter.setPen(QColor(0, 0, 0, 30))
        painter.drawPath(inner)

        if self._note is not None:
            self._paint_note(painter, top)
        else:
            self._paint_bars(painter, top)
        self._paint_gear(painter)

    def _paint_bars(self, painter: QPainter, top: float):
        total_w = BAR_COUNT * BAR_WIDTH + (BAR_COUNT - 1) * BAR_GAP
        start_x = (BARS_AREA_WIDTH - total_w) / 2
        center_y = top + HEIGHT / 2
        painter.setPen(Qt.PenStyle.NoPen)
        for i in range(BAR_COUNT):
            breath = 0.10 + 0.07 * math.sin(self._phase + i * 0.9)
            level = max(self._levels_shown[i], breath)
            bar_h = BAR_MIN_HEIGHT + level * (BAR_MAX_HEIGHT - BAR_MIN_HEIGHT)
            x = start_x + i * (BAR_WIDTH + BAR_GAP)
            y = center_y - bar_h / 2
            bar = QPainterPath()
            bar.addRoundedRect(x, y, float(BAR_WIDTH), bar_h, BAR_WIDTH / 2, BAR_WIDTH / 2)
            alpha = int(150 + 100 * min(1.0, level * 1.6))
            painter.fillPath(bar, QColor(0, 0, 0, alpha))

    def _paint_note(self, painter: QPainter, top: float):
        remaining = max(0.0, self._note_until - time.monotonic())
        alpha = int(225 * min(1.0, remaining / 0.35))
        painter.setPen(QColor(0, 0, 0, alpha))
        font = QFont("Segoe UI", 9)
        font.setLetterSpacing(QFont.SpacingType.AbsoluteSpacing, 0.6)
        painter.setFont(font)
        painter.drawText(0, int(top), BARS_AREA_WIDTH, HEIGHT, Qt.AlignmentFlag.AlignCenter, self._note)

    def _paint_gear(self, painter: QPainter):
        c = self._gear_center()
        alpha = int(115 + 120 * self._gear_hover_t)
        scale = 1.0 + 0.12 * self._gear_hover_t
        painter.save()
        painter.translate(c)
        painter.rotate(self._gear_angle)
        painter.scale(scale, scale)
        painter.setPen(Qt.PenStyle.NoPen)
        painter.fillPath(self._gear_path, QColor(0, 0, 0, alpha))
        painter.restore()


class OverlayBridge(QObject):
    """Puente thread-safe: los hilos de trabajo emiten senales, el overlay
    vive y se pinta en el hilo de la GUI (el que corre QApplication.exec())."""

    recording_started = Signal()
    recording_stopped = Signal()
    level_changed = Signal(float)

    def __init__(self, overlay: RecordingOverlay):
        super().__init__()
        self._overlay = overlay
        self.settings_clicked = overlay.settings_clicked
        self.recording_started.connect(overlay.fade_in)
        self.recording_stopped.connect(overlay.fade_out)
        self.level_changed.connect(overlay.set_level)


def create_app_and_overlay():
    app = QApplication.instance() or QApplication(sys.argv)
    overlay = RecordingOverlay()
    bridge = OverlayBridge(overlay)
    return app, bridge
