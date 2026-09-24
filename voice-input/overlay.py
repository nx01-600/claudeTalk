"""Floating "liquid glass" overlay shown while recording, plus the settings
panel opened by the gear icon.

Black and white, never steals focus (critical: if these windows activated,
GetForegroundWindow() would stop pointing at the real window and the paste
would fail or the text would be pasted into the overlay). Achieved with Qt
flags plus the native WS_EX_NOACTIVATE style, which also lets you click the
gear and the panel without activating the window (the same mechanism Windows'
on-screen keyboard uses). That's why the panel doesn't use focusable Qt
widgets (QPushButton, QSlider): everything is painted and resolved with the
mouse.

The glass is real and live: the window is excluded from screen capture
(WDA_EXCLUDEFROMCAPTURE), so while it is visible it can grab what is behind
itself 25 times a second, blur it (color kept, saturation boosted), tint it,
and add edge lensing plus a specular rim. With "Show in screen share" on,
the window stays capturable instead (it shows in Meet/Zoom/recordings) and
the glass is a single snapshot taken just before the window appears: it
would otherwise capture itself. Focus handling is the same either way. Windows' native backdrops
(DWMWA_SYSTEMBACKDROP_TYPE / SetWindowCompositionAttribute) were tried and
only produce a flat solid panel for a window whose content Qt paints by hand,
so they are not used. The "Glass" setting drives blur, tint and saturation.
"""

import ctypes
import math
import sys
import time

import numpy as np

from PySide6.QtCore import QEasingCurve, QObject, QPointF, QRectF, Qt, QTimer, QVariantAnimation, Signal
from PySide6.QtGui import QColor, QFont, QImage, QPainter, QPainterPath, QPen, QPixmap, QTransform
from PySide6.QtWidgets import QApplication, QLineEdit, QWidget

import config as cfg
import hotkey as hk
import inject
import tts

WS_EX_NOACTIVATE = 0x08000000
WS_EX_TOOLWINDOW = 0x00000080
GWL_EXSTYLE = -20

WINDOW_FLAGS = Qt.FramelessWindowHint | Qt.WindowStaysOnTopHint | Qt.Tool | Qt.WindowDoesNotAcceptFocus
FONT_FAMILY = "Segoe UI"
FRAME_MS = 16
# Transparent margin around each window to paint the shadow (offset
# downward and blurred) outside the glass.
INSET = 16
SHADOW_OFFSET_Y = 4
SHADOW_SPREAD = 12


WDA_NONE = 0x00
WDA_EXCLUDEFROMCAPTURE = 0x11


def _apply_native_overlay_styles(hwnd: int, capturable: bool):
    user32 = ctypes.windll.user32
    style = user32.GetWindowLongW(hwnd, GWL_EXSTYLE)
    user32.SetWindowLongW(hwnd, GWL_EXSTYLE, style | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW)
    # Invisible to screen capture: that is what lets the window grab what is
    # behind itself while visible (live backdrop). Side effect: the overlay
    # does not show up in screenshots or screen sharing, unless the user
    # asked for that ("Show in screen share").
    user32.SetWindowDisplayAffinity(hwnd, WDA_NONE if capturable else WDA_EXCLUDEFROMCAPTURE)


def _lerp(a: float, b: float, t: float) -> float:
    return a + (b - a) * t


# --- shared style (dark glass) --------------------------------------------


class Style:
    """Reads glass/position from the config; the overlay and panel
    always paint from this, so a settings change is reflected instantly."""

    def __init__(self, config: cfg.Config):
        self.config = config

    @property
    def glass(self) -> float:
        return max(0.0, min(1.0, self.config.get("glass") / 100.0))

    @property
    def top(self) -> bool:
        return self.config.get("position") == "top"

    @property
    def capturable(self) -> bool:
        return bool(self.config.get("show_in_capture"))

    def fg(self, alpha: int) -> QColor:
        return QColor(255, 255, 255, alpha)

    def surface(self, alpha: int) -> QColor:
        return QColor(0, 0, 0, alpha)

    def glyph(self, alpha: int) -> QColor:
        """The pill's bars and gear: pure white."""
        return QColor(255, 255, 255, alpha)

    def pill_veil(self) -> QColor:
        """Veil over the pill's backdrop."""
        return self.surface(self.tint_alpha())

    def text(self, primary: bool = True) -> QColor:
        return QColor(255, 255, 255, 235 if primary else 150)

    # "Glass" slider: 0 = almost opaque and barely blurred, 100 = very clear
    # glass with a deep blur. Tint is what keeps text legible on busy backgrounds.
    def blur_px(self) -> int:
        return int(round(_lerp(10, 36, self.glass)))

    def tint_alpha(self) -> int:
        # No solid wash: at the top of the slider the glass is just the blurred,
        # darkened backdrop with its rims. Lower values add a faint black veil
        # for legibility on very busy content.
        return int(round(_lerp(90, 0, self.glass)))

    def saturation(self) -> float:
        return _lerp(1.15, 1.5, self.glass)


def _box_blur(a: np.ndarray, r: int) -> np.ndarray:
    """Separable box blur on a float32 HxWxC array, edge-replicated."""
    if r <= 0:
        return a
    pad = np.pad(a, ((r, r), (0, 0), (0, 0)), mode="edge")
    c = np.cumsum(pad, axis=0)
    a = (c[2 * r :] - c[: -2 * r]) / (2 * r)
    pad = np.pad(a, ((0, 0), (r, r), (0, 0)), mode="edge")
    c = np.cumsum(pad, axis=1)
    return (c[:, 2 * r :] - c[:, : -2 * r]) / (2 * r)


def _glassify(raw: QPixmap, style: Style) -> QPixmap:
    """Turns a capture of what is behind the window into the glass backdrop:
    real blur (three box passes at 1/3 resolution, close to a gaussian) with
    color kept and saturation boosted, the way Apple's material does. Runs in
    about 3 ms for the pill, so it can refresh live while the window is visible."""
    img = raw.toImage().convertToFormat(QImage.Format.Format_ARGB32)
    w, h = img.width(), img.height()
    scale = 3
    small = img.scaled(
        max(1, w // scale), max(1, h // scale), Qt.AspectRatioMode.IgnoreAspectRatio, Qt.TransformationMode.SmoothTransformation
    )
    sw, sh = small.width(), small.height()
    buf = np.frombuffer(small.constBits(), dtype=np.uint8).reshape(sh, small.bytesPerLine())[:, : sw * 4].reshape(sh, sw, 4)
    rgb = buf[:, :, :3].astype(np.float32)
    r = max(1, style.blur_px() // scale)
    for _ in range(3):
        rgb = _box_blur(rgb, r)
    gray = rgb @ np.array([0.114, 0.587, 0.299], dtype=np.float32)  # BGR order
    rgb = gray[..., None] + (rgb - gray[..., None]) * style.saturation()
    rgb = np.clip(rgb * 0.8, 0, 255)
    out = np.empty((sh, sw, 4), dtype=np.uint8)
    out[:, :, :3] = rgb.astype(np.uint8)
    out[:, :, 3] = 255
    image = QImage(out.tobytes(), sw, sh, sw * 4, QImage.Format.Format_ARGB32).scaled(
        w, h, Qt.AspectRatioMode.IgnoreAspectRatio, Qt.TransformationMode.SmoothTransformation
    )
    result = QPixmap.fromImage(image)
    result.setDevicePixelRatio(raw.devicePixelRatio())
    return result


def _capture_behind(widget: QWidget) -> QPixmap:
    """Grabs the screen area under the widget. Only shows what is behind it
    if the window is hidden or excluded from capture (WDA_EXCLUDEFROMCAPTURE)."""
    geo = widget.geometry()
    screen = QApplication.screenAt(geo.center()) or QApplication.primaryScreen()
    return screen.grabWindow(0, geo.x(), geo.y(), geo.width(), geo.height())


def _paint_shadow(painter: QPainter, rect: QRectF, radius: float, style: Style):
    """Soft offset shadow: concentric layers, each smaller and more opaque.
    Cheap and good enough to separate the glass from the background."""
    painter.setPen(Qt.PenStyle.NoPen)
    layers = SHADOW_SPREAD
    peak = 60
    for k in range(layers, -1, -1):
        alpha = int(peak * ((layers - k) / layers) ** 2 / 3.0) + 1
        shadow = QPainterPath()
        shadow.addRoundedRect(rect.adjusted(-k, -k + SHADOW_OFFSET_Y, k, k + SHADOW_OFFSET_Y), radius + k, radius + k)
        painter.fillPath(shadow, QColor(0, 0, 0, alpha))


LENS_BAND_PX = 9
LENS_SCALE = 1.07


def _paint_glass(
    painter: QPainter, shape: QPainterPath, bg: QPixmap | None, rect: QRectF, radius: float, style: Style, veil: QColor | None = None
):
    """Liquid glass, layer by layer: outer shadow, live blurred backdrop, tint,
    edge lensing (the backdrop slightly magnified inside the outer band, like
    light bending at the edge of thick glass), a thin bright rim with a faint
    dark inner line, and a specular highlight along the top edge."""
    _paint_shadow(painter, rect, radius, style)
    painter.setClipPath(shape)
    if bg is not None:
        painter.drawPixmap(0, 0, bg)
        inner = QPainterPath()
        inner.addRoundedRect(
            rect.adjusted(LENS_BAND_PX, LENS_BAND_PX, -LENS_BAND_PX, -LENS_BAND_PX), radius - LENS_BAND_PX, radius - LENS_BAND_PX
        )
        painter.save()
        painter.setClipPath(shape.subtracted(inner))
        painter.setOpacity(0.7)
        center = rect.center()
        painter.translate(center)
        painter.scale(LENS_SCALE, LENS_SCALE)
        painter.translate(-center)
        painter.drawPixmap(0, 0, bg)
        painter.restore()
        painter.setClipPath(shape)
        # Veil after the lens band, so the band is tinted like the rest and
        # doesn't show as a lighter/darker ring.
        painter.fillPath(shape, veil or style.surface(style.tint_alpha()))
    else:
        painter.fillPath(shape, style.surface(235))
    painter.setClipping(False)

    painter.setBrush(Qt.BrushStyle.NoBrush)
    painter.setPen(QPen(QColor(255, 255, 255, 120), 1.0))
    painter.drawPath(shape)
    inner_line = QPainterPath()
    inner_line.addRoundedRect(rect.adjusted(1, 1, -1, -1), radius - 1.0, radius - 1.0)
    painter.setPen(QPen(QColor(0, 0, 0, 70), 1.0))
    painter.drawPath(inner_line)
    painter.save()
    painter.setClipRect(QRectF(rect.x(), rect.y(), rect.width(), rect.height() * 0.34))
    painter.setPen(QPen(QColor(255, 255, 255, 150), 1.4))
    painter.drawPath(inner_line)
    painter.restore()


LIVE_REFRESH_MS = 40


class _GlassWindow(QWidget):
    """Shared base: frameless, always on top, never activates, translucent
    background, and the glass backdrop: refreshed live while visible when
    excluded from screen capture, or a snapshot taken before showing when
    the user wants it visible in screen sharing."""

    def __init__(self, style: Style):
        super().__init__(None, WINDOW_FLAGS)
        self.setAttribute(Qt.WA_TranslucentBackground)
        self.setAttribute(Qt.WA_ShowWithoutActivating)
        self.setMouseTracking(True)
        self.style_ = style
        self._bg_pixmap = None
        self._bg_raw = None  # last capture, re-blurred when glass changes
        self._live = QTimer(self)
        self._live.setInterval(LIVE_REFRESH_MS)
        self._live.timeout.connect(self.refresh_background)
        _apply_native_overlay_styles(int(self.winId()), style.capturable)

    def showEvent(self, event):
        super().showEvent(event)
        self.apply_capture_mode()

    def hideEvent(self, event):
        super().hideEvent(event)
        self._live.stop()

    def apply_capture_mode(self):
        """Excluded from capture: live glass. Capturable: snapshot glass, no
        timer at all (cheaper too)."""
        _apply_native_overlay_styles(int(self.winId()), self.style_.capturable)
        if self.isVisible() and not self.style_.capturable:
            self._live.start()
        else:
            self._live.stop()

    def refresh_background(self):
        # A capturable window that is on screen would grab itself: keep the
        # snapshot taken before it appeared and only re-blur it.
        if self._bg_raw is None or not (self.style_.capturable and self.isVisible()):
            self._bg_raw = _capture_behind(self)
        self._bg_pixmap = _glassify(self._bg_raw, self.style_)
        self.update()


# --- pill -----------------------------------------------------------------

WIDTH = 200
HEIGHT = 56
SLIDE_PX = 14
EDGE_MARGIN = 40

BAR_COUNT = 5
BAR_WIDTH = 6
BAR_GAP = 8
BAR_MIN_HEIGHT = 6
BAR_MAX_HEIGHT = 30
BARS_AREA_WIDTH = WIDTH - 44  # leaves room for the gear on the right

GEAR_CENTER_X = WIDTH - 24
GEAR_OUTER_R = 9.6
GEAR_BODY_R = 6.9
GEAR_TOOTH_W = 3.8
GEAR_HOLE_R = 3.1
GEAR_TEETH = 8
GEAR_HIT_R = 14

SHOW_MS = 220
HIDE_MS = 170

# After the text is sent the whole pill shrinks into a glass circle (the
# gear fades away, the bars fold into the middle) and a badge pops up inside
# it: green check when it was pasted, amber clipboard icon when it was only left on the
# clipboard (a clipboard icon). It holds for a moment, then fades out.
MORPH_MS = 340
BADGE_R = 16.0
BADGE_IN_MS = 260  # starts halfway through the morph
CHECK_DRAW_MS = 240
BADGE_HOLD_MS = 700
BADGE_OK = QColor(52, 199, 89)
BADGE_WARN = QColor(255, 159, 10)
BUSY_WAVE_SPEED = 7.0  # rad/s of the "transcribing" wave running through the bars


def _gear_path() -> QPainterPath:
    """Circular body + rounded teeth (boolean union) minus the central hole:
    smooth edges and a standard gear shape, no hand-drawn polygon."""
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


class RecordingOverlay(_GlassWindow):
    """Floating pill with bars that react to volume, plus the gear icon."""

    def __init__(self, style: Style, panel: "SettingsPanel"):
        super().__init__(style)
        self.resize(WIDTH + 2 * INSET, HEIGHT + SLIDE_PX + 2 * INSET)

        self._panel = panel
        self._panel.closed.connect(self._on_panel_closed)
        self._hide_pending = False

        self._levels_target = [0.0] * BAR_COUNT
        self._levels_shown = [0.0] * BAR_COUNT
        self._slide = 1.0
        self._gear_path = _gear_path()
        self._gear_hover = False
        self._gear_hover_t = 0.0
        self._gear_angle = 0.0
        self._gear_angle_target = 0.0
        self._closing = False
        self._phase = "record"  # record | busy | done
        self._phase_t0 = 0.0
        self._ok = True
        self._phase_token = 0

        self._frame = QTimer(self)
        self._frame.setInterval(FRAME_MS)
        self._frame.timeout.connect(self._tick)

        self._anim = QVariantAnimation(self)
        self._anim.valueChanged.connect(self._on_progress)
        self._anim.finished.connect(self._on_anim_finished)

        self._place()

    # --- position -----------------------------------------------------------

    def _place(self):
        screen = QApplication.primaryScreen().availableGeometry()
        x = screen.x() + (screen.width() - self.width()) // 2
        if self.style_.top:
            y = screen.y() + EDGE_MARGIN - SLIDE_PX - INSET
        else:
            y = screen.y() + screen.height() - self.height() - EDGE_MARGIN + INSET
        self.move(x, y)

    def _pill_top(self) -> float:
        # bottom: slides in upward; top: slides in downward
        return INSET + (self._slide if not self.style_.top else (1.0 - self._slide)) * SLIDE_PX

    def pill_rect_on_screen(self) -> QRectF:
        """Visible rectangle of the pill (without the shadow margin), in
        screen coordinates; used by the panel to position itself."""
        return QRectF(self.x() + INSET, self.y() + self._pill_top(), WIDTH, HEIGHT)

    # --- entry / exit ---------------------------------------------------

    def fade_in(self):
        self._set_phase("record")
        self._closing = False
        self._hide_pending = False
        self._levels_target = [0.0] * BAR_COUNT
        self._levels_shown = [0.0] * BAR_COUNT
        if not self.isVisible():
            self._place()
            self.refresh_background()
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
        if self._panel.isVisible():
            # the user is in settings: the pill stays until the panel closes
            self._hide_pending = True
            return
        self._closing = True
        self._anim.stop()
        self._anim.setDuration(HIDE_MS)
        self._anim.setEasingCurve(QEasingCurve.Type.InCubic)
        self._anim.setStartValue(self.windowOpacity())
        self._anim.setEndValue(0.0)
        self._anim.start()

    def _set_phase(self, phase: str):
        self._phase = phase
        self._phase_t0 = time.monotonic()
        self._phase_token += 1

    def show_busy(self):
        """Recording over, transcribing and pasting: the pill stays up."""
        if self.isVisible() and not self._closing:
            self._set_phase("busy")

    def show_result(self, ok: bool):
        """The text was sent (ok) or only left on the clipboard: show the
        badge, then fade out."""
        if not self.isVisible() or self._closing:
            return
        self._ok = ok
        self._set_phase("done")
        token = self._phase_token
        QTimer.singleShot(MORPH_MS // 2 + BADGE_IN_MS + CHECK_DRAW_MS + BADGE_HOLD_MS, lambda: self._badge_done(token))

    def _badge_done(self, token: int):
        if token == self._phase_token:
            self.fade_out()

    def _phase_ms(self) -> float:
        return (time.monotonic() - self._phase_t0) * 1000.0

    def _morph(self) -> float:
        """0 = full pill, 1 = compacted into a circle (done phase only)."""
        if self._phase != "done":
            return 0.0
        return QEasingCurve(QEasingCurve.Type.InOutCubic).valueForProgress(min(1.0, self._phase_ms() / MORPH_MS))

    def _on_progress(self, value):
        self.setWindowOpacity(float(value))
        self._slide = 1.0 - float(value)
        self.update()

    def _on_anim_finished(self):
        if self._closing:
            self._frame.stop()
            self.hide()

    def _on_panel_closed(self):
        self._gear_angle_target = 0.0
        if self._hide_pending:
            self._hide_pending = False
            self.fade_out()

    def restyle(self):
        """After a glass/position change in settings."""
        if self.isVisible():
            self._place()
            self.refresh_background()
            self.update()

    # --- animated state -----------------------------------------------------

    def set_level(self, normalized: float):
        """normalized in [0, 1]. Shifts the bars like a simple equalizer."""
        normalized = max(0.0, min(1.0, normalized))
        self._levels_target = self._levels_target[1:] + [normalized]

    def _tick(self):
        if self._phase == "busy":
            t = time.monotonic() * BUSY_WAVE_SPEED
            self._levels_target = [0.08 + 0.3 * max(0.0, math.sin(t - i * 0.9)) ** 2 for i in range(BAR_COUNT)]
        for i in range(BAR_COUNT):
            self._levels_shown[i] = _lerp(self._levels_shown[i], self._levels_target[i], 0.35)
        self._gear_hover_t = _lerp(self._gear_hover_t, 1.0 if self._gear_hover else 0.0, 0.25)
        self._gear_angle = _lerp(self._gear_angle, self._gear_angle_target, 0.18)
        self.update()

    # --- mouse (gear) -----------------------------------------------------

    def _gear_center(self) -> QPointF:
        return QPointF(INSET + GEAR_CENTER_X, self._pill_top() + HEIGHT / 2)

    def _over_gear(self, pos) -> bool:
        if self._phase == "done":
            return False  # the gear is gone once the message is sent
        c = self._gear_center()
        return math.hypot(pos.x() - c.x(), pos.y() - c.y()) <= GEAR_HIT_R

    def _set_gear_hover(self, hover: bool):
        if hover != self._gear_hover:
            self._gear_hover = hover
            base = 90.0 if self._panel.isVisible() else 0.0
            self._gear_angle_target = base + (30.0 if hover else 0.0)
            self.setCursor(Qt.CursorShape.PointingHandCursor if hover else Qt.CursorShape.ArrowCursor)

    def mouseMoveEvent(self, event):
        self._set_gear_hover(self._over_gear(event.position()))

    def leaveEvent(self, event):
        self._set_gear_hover(False)

    def mousePressEvent(self, event):
        if event.button() == Qt.MouseButton.LeftButton and self._over_gear(event.position()):
            if self._panel.isVisible():
                self._panel.close_panel()
            else:
                self._panel.open_near(self)
                self._gear_angle_target = 120.0

    # --- painting ------------------------------------------------------------

    def paintEvent(self, event):
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)

        top = self._pill_top()
        radius = HEIGHT / 2
        morph = self._morph()
        width = _lerp(WIDTH, HEIGHT, morph)
        rect = QRectF(INSET + (WIDTH - width) / 2, top, width, float(HEIGHT))
        pill = QPainterPath()
        pill.addRoundedRect(rect, radius, radius)
        _paint_glass(painter, pill, self._bg_pixmap, rect, radius, self.style_, veil=self.style_.pill_veil())

        if self._phase == "done":
            if morph < 1.0:
                self._paint_bars(painter, top, morph)
            self._paint_badge(painter, top)
        else:
            self._paint_bars(painter, top)
        gear_alpha = max(0.0, 1.0 - morph * 2.5)
        if gear_alpha > 0.0:
            painter.setOpacity(gear_alpha)
            self._paint_gear(painter)
            painter.setOpacity(1.0)

    def _paint_bars(self, painter: QPainter, top: float, fold: float = 0.0):
        """fold 0..1: the bars slide into the middle of the pill and shrink,
        along with the pill compacting into a circle."""
        total_w = BAR_COUNT * BAR_WIDTH + (BAR_COUNT - 1) * BAR_GAP
        start_x = INSET + (BARS_AREA_WIDTH - total_w) / 2
        center_x = INSET + WIDTH / 2
        center_y = top + HEIGHT / 2
        painter.setPen(Qt.PenStyle.NoPen)
        for i in range(BAR_COUNT):
            level = self._levels_shown[i] * (1.0 - fold)
            bar_h = BAR_MIN_HEIGHT + level * (BAR_MAX_HEIGHT - BAR_MIN_HEIGHT)
            x = _lerp(start_x + i * (BAR_WIDTH + BAR_GAP), center_x - BAR_WIDTH / 2, fold)
            y = center_y - bar_h / 2
            bar = QPainterPath()
            bar.addRoundedRect(x, y, float(BAR_WIDTH), bar_h, BAR_WIDTH / 2, BAR_WIDTH / 2)
            alpha = int((170 + 85 * min(1.0, level * 1.6)) * (1.0 - fold))
            painter.fillPath(bar, self.style_.glyph(alpha))

    def _paint_badge(self, painter: QPainter, top: float):
        """Colored disc that pops in, then a white check drawn stroke by
        stroke (or a clipboard icon)."""
        ms = self._phase_ms() - MORPH_MS / 2
        if ms <= 0.0:
            return
        grow = QEasingCurve(QEasingCurve.Type.OutBack).valueForProgress(min(1.0, ms / BADGE_IN_MS))
        c = QPointF(INSET + WIDTH / 2, top + HEIGHT / 2)
        r = BADGE_R * grow
        painter.setPen(Qt.PenStyle.NoPen)
        painter.setBrush(BADGE_OK if self._ok else BADGE_WARN)
        painter.drawEllipse(c, r, r)
        painter.setBrush(Qt.BrushStyle.NoBrush)

        draw = QEasingCurve(QEasingCurve.Type.OutCubic).valueForProgress(
            max(0.0, min(1.0, (ms - BADGE_IN_MS * 0.55) / CHECK_DRAW_MS))
        )
        if draw <= 0.0:
            return
        pen = QPen(QColor(255, 255, 255), 2.6)
        pen.setCapStyle(Qt.PenCapStyle.RoundCap)
        pen.setJoinStyle(Qt.PenJoinStyle.RoundJoin)
        painter.setPen(pen)
        if self._ok:
            points = [QPointF(-5.8, 0.4), QPointF(-1.9, 4.3), QPointF(6.0, -4.4)]
            lengths = [math.dist((a.x(), a.y()), (b.x(), b.y())) for a, b in zip(points, points[1:])]
            left = draw * sum(lengths)
            path = QPainterPath(c + points[0])
            for a, b, seg in zip(points, points[1:], lengths):
                path.lineTo(c + a + (b - a) * min(1.0, left / seg))
                left -= seg
                if left <= 0:
                    break
            painter.drawPath(path)
        else:
            # Clipboard: the text is waiting there for a manual Ctrl+V.
            painter.save()
            painter.translate(c)
            painter.scale(0.6 + 0.4 * draw, 0.6 + 0.4 * draw)
            painter.setOpacity(draw)
            pen.setWidthF(2.0)
            painter.setPen(pen)
            painter.drawRoundedRect(QRectF(-6.0, -6.0, 12.0, 14.5), 2.2, 2.2)
            painter.setPen(Qt.PenStyle.NoPen)
            painter.setBrush(QColor(255, 255, 255))
            painter.drawRoundedRect(QRectF(-3.4, -8.4, 6.8, 4.4), 1.4, 1.4)
            painter.setBrush(Qt.BrushStyle.NoBrush)
            pen.setWidthF(1.7)
            painter.setPen(pen)
            painter.drawLine(QPointF(-2.8, 0.2), QPointF(2.8, 0.2))
            painter.drawLine(QPointF(-2.8, 3.8), QPointF(1.2, 3.8))
            painter.restore()

    def _paint_gear(self, painter: QPainter):
        c = self._gear_center()
        alpha = int(150 + 105 * self._gear_hover_t)
        scale = 1.0 + 0.12 * self._gear_hover_t
        painter.save()
        painter.translate(c)
        painter.rotate(self._gear_angle)
        painter.scale(scale, scale)
        painter.setPen(Qt.PenStyle.NoPen)
        painter.fillPath(self._gear_path, self.style_.glyph(alpha))
        painter.restore()


# --- settings panel ----------------------------------------------------------

PANEL_W = 328
PANEL_RADIUS = 20
PAD = 16
TITLE_H = 44
GROUP_TITLE_H = 28
ROW_H = 42
GROUP_GAP = 10
GROUP_RADIUS = 12
CONTROL_RIGHT = PANEL_W - PAD - 12

TOGGLE_W, TOGGLE_H = 40, 24
SEG_H = 26
SEG_MIN_W = 58
SLIDER_W = 132
SLIDER_KNOB_R = 8
SLIDER_VALUE_W = 44  # value label to the left of a ranged slider
TEXT_MAX_W = 150  # a text field's chip; longer text is elided


class SliderRange:
    """A slider over [low, high] in `step`s, showing its value as `fmt(v)`.
    Sliders without one run 0..100 and show no number."""

    def __init__(self, low, high, step, fmt):
        self.low, self.high, self.step, self.fmt = low, high, step, fmt


PERCENT = SliderRange(0, 100, 1, None)
SILENCE_RANGE = SliderRange(500, 10000, 250, lambda v: f"{v / 1000:g} s")
CAPTURE_TIMEOUT_S = 8.0

_MOD_RANK = {0xA2: 0, 0xA3: 0, 0xA4: 1, 0xA5: 1, 0xA0: 2, 0xA1: 2, 0x5B: 3, 0x5C: 3}


def _chord_sorted(vks) -> list[int]:
    return sorted(set(vks), key=lambda vk: (_MOD_RANK.get(vk, 4), vk))


class SettingsPanel(_GlassWindow):
    """Settings in groups (Activation / Appearance / Transcription) with
    hand-painted controls: switch, segmented control, slider, key capture.
    Turning off asks for inline confirmation (no modal) before emitting
    quit_requested."""

    settings_changed = Signal(str, object)
    capture_started = Signal()
    capture_finished = Signal()
    quit_requested = Signal()
    closed = Signal()

    TITLE = "Dictation"

    def __init__(self, style: Style, companion: "SettingsPanel | None" = None):
        super().__init__(style)
        self.config = style.config
        self._companion = companion  # panel opened to the right of this one
        self._hover = None  # (row_index, part)
        self._pressed = None
        self._dragging_slider = None  # row index of the slider being dragged
        self._confirming_quit = False
        self._capturing = False
        self._capture_acc: set[int] = set()
        self._capture_live: list[int] = []
        self._capture_had = False
        self._capture_deadline = 0.0
        self._toggle_t: dict[str, float] = {}
        self._phase = 0.0
        self._editor = None  # _TextEditor, created on first use

        self._frame = QTimer(self)
        self._frame.setInterval(FRAME_MS)
        self._frame.timeout.connect(self._tick)
        self._capture_timer = QTimer(self)
        self._capture_timer.setInterval(30)
        self._capture_timer.timeout.connect(self._capture_tick)

        self._anchor_y = 0
        self._closing = False
        self._anim = QVariantAnimation(self)
        self._anim.valueChanged.connect(self._on_progress)
        self._anim.finished.connect(self._on_anim_finished)

        self._rows = self._build_rows()
        self.resize(PANEL_W + 2 * INSET, self._content_height() + 2 * INSET)

    # --- row model ----------------------------------------------------

    def _build_rows(self):
        rows = [
            ("group", "Activation", None, None),
            ("hotkey", "Keys", "hotkey", None),
            ("slider", "Silence cutoff", "silence_ms", SILENCE_RANGE),
            ("slider", "Mic sensitivity", "sensitivity", None),
            ("toggle", "Sound on start", "sound", None),
            ("toggle", "Send with Enter", "auto_enter", None),
            ("group", "Appearance", None, None),
            ("slider", "Glass", "glass", None),
            ("segment", "Position", "position", [("bottom", "Bottom"), ("top", "Top")]),
            ("toggle", "Show in screen share", "show_in_capture", None),
            ("group", "Transcription", None, None),
            ("segment", "Language", "language", [("es", "Spanish"), ("en", "English"), ("auto", "Auto")]),
            ("group", "", None, None),
            ("danger", "Turn off dictation", None, None),
        ]
        return rows

    def _open_companion(self):
        if self._companion is None:
            return
        screen = QApplication.primaryScreen().availableGeometry()
        gap = 10
        x = self.x() + PANEL_W + gap
        if x + PANEL_W + INSET > screen.right() - 8:
            x = self.x() - PANEL_W - gap  # no room on the right: go left
        # line up the bottoms (or the tops when the pill sits at the top)
        y = self._anchor_y
        if not self.style_.top:
            y += self._content_height() - self._companion._content_height()
        self._companion._open_at(x, y)

    def _row_rects(self):
        """Returns [(index, QRectF, group_rect_or_None)] for visible rows;
        groups are drawn as rounded containers."""
        y = PAD + TITLE_H
        out = []
        group_start = None
        for i, (kind, label, key, opts) in enumerate(self._rows):
            if kind == "group":
                if group_start is not None:
                    out.append(("group_end", None, y))
                y += GROUP_GAP if label == "" else GROUP_TITLE_H
                out.append(("group_title", i, y))
                group_start = y
                continue
            out.append(("row", i, QRectF(PAD, y, PANEL_W - 2 * PAD, ROW_H)))
            y += ROW_H
        out.append(("group_end", None, y))
        return out, y + PAD

    def _content_height(self) -> int:
        _, h = self._row_rects()
        return int(h)

    def _rect_of(self, index: int) -> QRectF | None:
        items, _ = self._row_rects()
        for item in items:
            if item[0] == "row" and item[1] == index:
                return item[2]
        return None

    # --- open / close -----------------------------------------------------

    def open_near(self, pill: "RecordingOverlay"):
        screen = QApplication.primaryScreen().availableGeometry()
        visible = pill.pill_rect_on_screen()
        x = int(visible.right() - PANEL_W - INSET)
        if self.style_.top:
            y = int(visible.bottom() + 6 - INSET)
        else:
            y = int(visible.top() - 6 - INSET - self._content_height())
        x = max(screen.x() + 8 - INSET, min(x, screen.right() - PANEL_W - 8 - INSET))
        self._open_at(x, y)
        self._open_companion()

    def open_standalone(self):
        screen = QApplication.primaryScreen().availableGeometry()
        x = screen.x() + (screen.width() - self.width()) // 2
        y = screen.y() + screen.height() - self.height() - 96
        self._open_at(x, y)
        self._open_companion()

    def _open_at(self, x: int, y: int):
        self._confirming_quit = False
        self._hover = None
        self._pressed = None
        self._closing = False
        self._anchor_y = y
        self.move(x, y)
        self.refresh_background()
        self.setWindowOpacity(0.0)
        self.move(x, y + 8)
        self.show()
        self._frame.start()
        self._anim.stop()
        self._anim.setDuration(180)
        self._anim.setEasingCurve(QEasingCurve.Type.OutCubic)
        self._anim.setStartValue(0.0)
        self._anim.setEndValue(1.0)
        self._anim.start()

    def close_panel(self):
        if not self.isVisible() or self._closing:
            return
        self._end_capture(commit=False)
        if self._editor is not None:
            self._editor.finish(commit=True)
        if self._companion is not None:
            self._companion.close_panel()
        self._closing = True
        self._anim.stop()
        self._anim.setDuration(130)
        self._anim.setEasingCurve(QEasingCurve.Type.InCubic)
        self._anim.setStartValue(self.windowOpacity())
        self._anim.setEndValue(0.0)
        self._anim.start()

    def _on_progress(self, value):
        v = float(value)
        self.setWindowOpacity(v)
        self.move(self.x(), self._anchor_y + int(round(8 * (1.0 - v))))

    def _on_anim_finished(self):
        if self._closing:
            self._closing = False
            self._frame.stop()
            self.hide()
            self.closed.emit()

    def _tick(self):
        self._phase += 0.12
        for key in (row[2] for row in self._rows if row[0] == "toggle"):
            target = 1.0 if self.config.get(key) else 0.0
            self._toggle_t[key] = _lerp(self._toggle_t.get(key, target), target, 0.3)
        self.update()

    # --- key capture --------------------------------------------------

    def _begin_capture(self):
        self._capturing = True
        self._capture_acc = set()
        self._capture_live = []
        self._capture_had = False
        self._capture_deadline = time.monotonic() + CAPTURE_TIMEOUT_S
        self.capture_started.emit()
        self._capture_timer.start()

    def _end_capture(self, commit: bool):
        if not self._capturing:
            return
        self._capture_timer.stop()
        self._capturing = False
        if commit and self._capture_acc:
            keys = _chord_sorted(self._capture_acc)
            self.config.set("hotkey", keys)
            self.settings_changed.emit("hotkey", keys)
        self.capture_finished.emit()
        self.update()

    def _capture_tick(self):
        current = set(hk.pressed_keys())
        if current == {hk.VK_ESCAPE}:
            self._end_capture(commit=False)
            return
        if current:
            self._capture_acc |= current
            self._capture_had = True
            self._capture_live = _chord_sorted(current)
        elif self._capture_had:
            self._end_capture(commit=True)
            return
        if time.monotonic() > self._capture_deadline:
            self._end_capture(commit=False)

    # --- control geometry ---------------------------------------------

    def _control_rect(self, index: int, rect: QRectF) -> QRectF:
        kind = self._rows[index][0]
        cy = rect.center().y()
        if kind == "toggle":
            return QRectF(CONTROL_RIGHT - TOGGLE_W, cy - TOGGLE_H / 2, TOGGLE_W, TOGGLE_H)
        if kind == "segment":
            opts = self._rows[index][3]
            w = SEG_MIN_W * len(opts)
            return QRectF(CONTROL_RIGHT - w, cy - SEG_H / 2, w, SEG_H)
        if kind == "slider":
            return QRectF(CONTROL_RIGHT - SLIDER_W, cy - 10, SLIDER_W, 20)
        if kind == "hotkey":
            label = self._hotkey_text()
            w = self._text_width(label, 9) + 24
            return QRectF(CONTROL_RIGHT - w, cy - 13, w, 26)
        if kind == "text":
            w = min(TEXT_MAX_W, max(90.0, self._text_width(str(self.config.get(self._rows[index][2])), 9) + 24))
            return QRectF(CONTROL_RIGHT - w, cy - 13, w, 26)
        return rect

    def _text_width(self, text: str, pt: float) -> float:
        from PySide6.QtGui import QFontMetricsF

        return QFontMetricsF(QFont(FONT_FAMILY, pt)).horizontalAdvance(text)

    def _hotkey_text(self) -> str:
        if self._capturing:
            return cfg.hotkey_label(self._capture_live) if self._capture_live else "Press the keys"
        return cfg.hotkey_label(self.config.get("hotkey"))

    def _hit(self, pos):
        """(row_index, part). part: 'control' | 'seg:<n>' | 'confirm' | 'cancel' | 'close' | None"""
        if QRectF(PANEL_W - PAD - 28, PAD + 8, 28, 28).contains(pos):
            return (None, "close")
        items, _ = self._row_rects()
        for item in items:
            if item[0] != "row":
                continue
            index, rect = item[1], item[2]
            if not rect.contains(pos):
                continue
            kind, _label, _key, opts = self._rows[index]
            if kind == "danger":
                if self._confirming_quit:
                    confirm, cancel = self._confirm_rects(rect)
                    if confirm.contains(pos):
                        return (index, "confirm")
                    if cancel.contains(pos):
                        return (index, "cancel")
                    return (index, None)
                return (index, "control")
            control = self._control_rect(index, rect)
            if kind == "segment":
                if control.contains(pos):
                    n = int((pos.x() - control.left()) // (control.width() / len(opts)))
                    return (index, f"seg:{max(0, min(len(opts) - 1, n))}")
                return (index, None)
            if kind == "slider":
                return (index, "control") if control.adjusted(-8, -6, 8, 6).contains(pos) else (index, None)
            if kind in ("toggle", "hotkey", "text"):
                return (index, "control") if control.adjusted(-4, -4, 4, 4).contains(pos) else (index, None)
        return (None, None)

    def _confirm_rects(self, rect: QRectF):
        w = 76
        confirm = QRectF(rect.right() - 12 - w, rect.center().y() - 14, w, 28)
        cancel = QRectF(confirm.left() - 8 - w, rect.center().y() - 14, w, 28)
        return confirm, cancel

    # --- mouse --------------------------------------------------------------

    def _local(self, event) -> QPointF:
        return event.position() - QPointF(INSET, INSET)

    def mouseMoveEvent(self, event):
        pos = self._local(event)
        if self._dragging_slider is not None:
            self._apply_slider(pos)
            return
        hit = self._hit(pos)
        if hit != self._hover:
            self._hover = hit
            self.setCursor(Qt.CursorShape.PointingHandCursor if hit[1] else Qt.CursorShape.ArrowCursor)
            self.update()

    def leaveEvent(self, event):
        self._hover = None
        self.update()

    def mousePressEvent(self, event):
        if event.button() != Qt.MouseButton.LeftButton:
            return
        pos = self._local(event)
        index, part = self._hit(pos)
        if part == "close":
            self.close_panel()
            return
        if index is None or part is None:
            return
        kind, _label, key, opts = self._rows[index]
        if kind == "toggle":
            self._set(key, not self.config.get(key))
        elif kind == "segment" and part.startswith("seg:"):
            self._set(key, opts[int(part[4:])][0])
        elif kind == "slider":
            self._dragging_slider = index
            self._apply_slider(pos)
        elif kind == "hotkey":
            if not self._capturing:
                self._begin_capture()
        elif kind == "text":
            self._edit_text(index)
        elif kind == "danger":
            if part == "control":
                self._confirming_quit = True
            elif part == "cancel":
                self._confirming_quit = False
            elif part == "confirm":
                self.close_panel()
                self.quit_requested.emit()
        self.update()

    def mouseReleaseEvent(self, event):
        self._dragging_slider = None

    def _apply_slider(self, pos):
        i = self._dragging_slider
        if i is None:
            return
        key = self._rows[i][2]
        control = self._control_rect(i, self._rect_of(i))
        t = (pos.x() - control.left() - SLIDER_KNOB_R) / (control.width() - 2 * SLIDER_KNOB_R)
        rng = self._rows[i][3] or PERCENT
        raw = rng.low + max(0.0, min(1.0, t)) * (rng.high - rng.low)
        value = int(round(raw / rng.step) * rng.step)
        if value != self.config.get(key):
            self._set(key, value)

    def _set(self, key: str, value):
        self.config.set(key, value)
        self.settings_changed.emit(key, value)
        if key == "glass":
            self.refresh_background()
        self.update()

    # --- painting ------------------------------------------------------------

    def paintEvent(self, event):
        painter = QPainter(self)
        painter.setRenderHint(QPainter.RenderHint.Antialiasing)
        st = self.style_
        rect = QRectF(float(INSET), float(INSET), float(PANEL_W), float(self.height() - 2 * INSET))
        shape = QPainterPath()
        shape.addRoundedRect(rect, PANEL_RADIUS, PANEL_RADIUS)
        _paint_glass(painter, shape, self._bg_pixmap, rect, PANEL_RADIUS, st)
        # from here on everything is drawn relative to the glass, not the window
        painter.translate(INSET, INSET)

        # title + close
        title_font = QFont(FONT_FAMILY, 11)
        title_font.setWeight(QFont.Weight.DemiBold)
        painter.setFont(title_font)
        painter.setPen(st.text())
        painter.drawText(QRectF(PAD + 4, PAD, 200, TITLE_H - 8), Qt.AlignmentFlag.AlignVCenter, self.TITLE)
        self._paint_close(painter)

        items, _ = self._row_rects()
        group_top = None
        for item in items:
            if item[0] == "group_title":
                index, y = item[1], item[2]
                label = self._rows[index][1]
                if label:
                    painter.setFont(QFont(FONT_FAMILY, 8))
                    painter.setPen(st.text(False))
                    painter.drawText(
                        QRectF(PAD + 4, y - GROUP_TITLE_H, 200, GROUP_TITLE_H - 4),
                        Qt.AlignmentFlag.AlignBottom,
                        label.upper(),
                    )
                group_top = y
            elif item[0] == "group_end":
                group_top = None
        # rows on top of their containers
        previous_in_group = False
        for item in items:
            if item[0] == "group_title":
                previous_in_group = False
                continue
            if item[0] != "row":
                continue
            index, row_rect = item[1], item[2]
            if previous_in_group:
                painter.setPen(QPen(st.fg(22), 1))
                painter.drawLine(QPointF(row_rect.left() + 12, row_rect.top()), QPointF(row_rect.right() - 12, row_rect.top()))
            previous_in_group = True
            self._paint_row(painter, index, row_rect)

    def _paint_close(self, painter: QPainter):
        st = self.style_
        rect = QRectF(PANEL_W - PAD - 28, PAD + 8, 28, 28)
        hovered = self._hover == (None, "close")
        circle = QPainterPath()
        circle.addEllipse(rect)
        painter.fillPath(circle, st.fg(40 if hovered else 16))
        painter.setPen(QPen(st.fg(200), 1.6, Qt.PenStyle.SolidLine, Qt.PenCapStyle.RoundCap))
        c = rect.center()
        painter.drawLine(QPointF(c.x() - 4.5, c.y() - 4.5), QPointF(c.x() + 4.5, c.y() + 4.5))
        painter.drawLine(QPointF(c.x() - 4.5, c.y() + 4.5), QPointF(c.x() + 4.5, c.y() - 4.5))

    def _paint_row(self, painter: QPainter, index: int, rect: QRectF):
        st = self.style_
        kind, label, key, opts = self._rows[index]
        painter.setFont(QFont(FONT_FAMILY, 9.5))
        if kind == "danger":
            self._paint_danger(painter, rect)
            return
        painter.setPen(st.text())
        painter.drawText(rect.adjusted(14, 0, -14, 0), Qt.AlignmentFlag.AlignVCenter, label(self.config) if callable(label) else label)
        control = self._control_rect(index, rect)
        hovered = self._hover is not None and self._hover[0] == index and self._hover[1] is not None
        if kind == "toggle":
            self._paint_toggle(painter, control, key, hovered)
        elif kind == "segment":
            self._paint_segment(painter, control, key, opts, index)
        elif kind == "slider":
            self._paint_slider(painter, control, key, hovered, opts or PERCENT)
        elif kind == "hotkey":
            self._paint_hotkey(painter, control, hovered)
        elif kind == "text":
            self._paint_text(painter, control, key, hovered)

    def _paint_toggle(self, painter: QPainter, r: QRectF, key: str, hovered: bool):
        st = self.style_
        t = self._toggle_t.get(key, 1.0 if self.config.get(key) else 0.0)
        track = QPainterPath()
        track.addRoundedRect(r, TOGGLE_H / 2, TOGGLE_H / 2)
        off_alpha = 60 if hovered else 45
        painter.fillPath(track, st.fg(int(_lerp(off_alpha, 230, t))))
        knob_r = TOGGLE_H / 2 - 3
        kx = _lerp(r.left() + 3 + knob_r, r.right() - 3 - knob_r, t)
        knob = QPainterPath()
        knob.addEllipse(QPointF(kx, r.center().y()), knob_r, knob_r)
        painter.fillPath(knob, QColor(255, 255, 255))
        painter.setPen(QPen(QColor(0, 0, 0, 60), 1))
        painter.drawPath(knob)

    def _paint_segment(self, painter: QPainter, r: QRectF, key: str, opts, index: int):
        st = self.style_
        container = QPainterPath()
        container.addRoundedRect(r, SEG_H / 2, SEG_H / 2)
        painter.fillPath(container, st.fg(18))
        n = len(opts)
        seg_w = r.width() / n
        current = self.config.get(key)
        hover_n = None
        if self._hover and self._hover[0] == index and self._hover[1] and self._hover[1].startswith("seg:"):
            hover_n = int(self._hover[1][4:])
        painter.setFont(QFont(FONT_FAMILY, 8.5))
        for i, (value, text) in enumerate(opts):
            seg = QRectF(r.left() + i * seg_w, r.top(), seg_w, r.height())
            selected = value == current
            if selected:
                chip = QPainterPath()
                chip.addRoundedRect(seg.adjusted(2, 2, -2, -2), SEG_H / 2 - 2, SEG_H / 2 - 2)
                painter.fillPath(chip, st.fg(60))
                painter.setPen(QPen(st.fg(28), 1))
                painter.drawPath(chip)
            elif hover_n == i:
                chip = QPainterPath()
                chip.addRoundedRect(seg.adjusted(2, 2, -2, -2), SEG_H / 2 - 2, SEG_H / 2 - 2)
                painter.fillPath(chip, st.fg(14))
            painter.setPen(st.text() if selected else st.text(False))
            painter.drawText(seg, Qt.AlignmentFlag.AlignCenter, text)

    def _paint_slider(self, painter: QPainter, r: QRectF, key: str, hovered: bool, rng: SliderRange):
        st = self.style_
        current = self.config.get(key)
        value = max(0.0, min(1.0, (current - rng.low) / (rng.high - rng.low)))
        if rng.fmt is not None:
            painter.setFont(QFont(FONT_FAMILY, 8.5))
            painter.setPen(st.text(False))
            painter.drawText(
                QRectF(r.left() - SLIDER_VALUE_W - 4, r.top(), SLIDER_VALUE_W, r.height()),
                Qt.AlignmentFlag.AlignVCenter | Qt.AlignmentFlag.AlignRight,
                rng.fmt(current),
            )
        cy = r.center().y()
        x0 = r.left() + SLIDER_KNOB_R
        x1 = r.right() - SLIDER_KNOB_R
        track = QPainterPath()
        track.addRoundedRect(QRectF(x0, cy - 2, x1 - x0, 4), 2, 2)
        painter.fillPath(track, st.fg(40))
        kx = _lerp(x0, x1, value)
        filled = QPainterPath()
        filled.addRoundedRect(QRectF(x0, cy - 2, max(4.0, kx - x0), 4), 2, 2)
        painter.fillPath(filled, st.fg(225))
        dragging = self._dragging_slider is not None and self._rows[self._dragging_slider][2] == key
        knob_r = SLIDER_KNOB_R + (1.5 if (hovered or dragging) else 0)
        knob = QPainterPath()
        knob.addEllipse(QPointF(kx, cy), knob_r, knob_r)
        painter.fillPath(knob, QColor(255, 255, 255))
        painter.setPen(QPen(QColor(0, 0, 0, 70), 1))
        painter.drawPath(knob)

    def _paint_hotkey(self, painter: QPainter, r: QRectF, hovered: bool):
        st = self.style_
        chip = QPainterPath()
        chip.addRoundedRect(r, 8, 8)
        if self._capturing:
            pulse = 0.5 + 0.5 * math.sin(self._phase * 2)
            painter.fillPath(chip, st.fg(int(_lerp(18, 48, pulse))))
            painter.setPen(QPen(st.fg(120), 1))
            painter.drawPath(chip)
        else:
            painter.fillPath(chip, st.fg(34 if hovered else 22))
        painter.setFont(QFont(FONT_FAMILY, 9))
        painter.setPen(st.text())
        painter.drawText(r, Qt.AlignmentFlag.AlignCenter, self._hotkey_text())

    def _paint_text(self, painter: QPainter, r: QRectF, key: str, hovered: bool):
        from PySide6.QtGui import QFontMetricsF

        st = self.style_
        chip = QPainterPath()
        chip.addRoundedRect(r, 8, 8)
        painter.fillPath(chip, st.fg(34 if hovered else 22))
        font = QFont(FONT_FAMILY, 9)
        painter.setFont(font)
        painter.setPen(st.text())
        text = QFontMetricsF(font).elidedText(str(self.config.get(key)), Qt.TextElideMode.ElideRight, r.width() - 20)
        painter.drawText(r, Qt.AlignmentFlag.AlignCenter, text)

    # --- text fields ----------------------------------------------------
    #
    # The panel never takes the focus (so opening it doesn't pull the focus
    # off the app being dictated into), so it can't be typed into. Editing
    # opens a small field of its own right over the chip, which does.

    def _edit_text(self, index: int):
        key = self._rows[index][2]
        chip = self._control_rect(index, self._rect_of(index))
        editor = self._editor
        if editor is None:
            editor = self._editor = _TextEditor(self.style_)
            editor.committed.connect(self._commit_text)
        width = max(chip.width(), 200.0)
        top_left = self.mapToGlobal(QPointF(INSET + chip.right() - width, INSET + chip.top()).toPoint())
        editor.open(key, str(self.config.get(key)), top_left, int(width), int(chip.height()))

    def _commit_text(self, key: str, value: str):
        value = " ".join(value.split())
        if value and value != self.config.get(key):
            self._set(key, value)
        self.update()

    def _paint_danger(self, painter: QPainter, rect: QRectF):
        st = self.style_
        painter.setFont(QFont(FONT_FAMILY, 9.5))
        if not self._confirming_quit:
            hovered = self._hover is not None and self._hover[0] is not None and self._rows[self._hover[0]][0] == "danger"
            if hovered:
                hl = QPainterPath()
                hl.addRoundedRect(rect.adjusted(4, 4, -4, -4), 9, 9)
                painter.fillPath(hl, st.fg(14))
            painter.setPen(st.text())
            painter.drawText(rect.adjusted(14, 0, -14, 0), Qt.AlignmentFlag.AlignVCenter, "Turn off dictation")
            return
        confirm, cancel = self._confirm_rects(rect)
        painter.setPen(st.text())
        painter.drawText(
            QRectF(rect.left() + 14, rect.top(), cancel.left() - rect.left() - 22, rect.height()),
            Qt.AlignmentFlag.AlignVCenter,
            "Turn off?",
        )
        hover_part = self._hover[1] if self._hover else None
        painter.setFont(QFont(FONT_FAMILY, 9))
        cancel_path = QPainterPath()
        cancel_path.addRoundedRect(cancel, 14, 14)
        painter.fillPath(cancel_path, st.fg(34 if hover_part == "cancel" else 22))
        painter.setPen(st.text())
        painter.drawText(cancel, Qt.AlignmentFlag.AlignCenter, "Cancel")
        confirm_path = QPainterPath()
        confirm_path.addRoundedRect(confirm, 14, 14)
        painter.fillPath(confirm_path, st.fg(255 if hover_part == "confirm" else 225))
        painter.setPen(st.surface(255))
        painter.drawText(confirm, Qt.AlignmentFlag.AlignCenter, "Turn off")


class _TextEditor(QLineEdit):
    """A one-line field over a text row's chip. Enter or clicking away
    saves, Esc drops the change."""

    committed = Signal(str, str)

    def __init__(self, style: Style):
        super().__init__()
        self.setWindowFlags(Qt.FramelessWindowHint | Qt.WindowStaysOnTopHint | Qt.Tool)
        self.setFont(QFont(FONT_FAMILY, 9))
        self.setMaxLength(40)
        self.setStyleSheet(
            "QLineEdit { background: #2b2d31; color: #ffffff; border: 1px solid #8a8d93;"
            " border-radius: 8px; padding: 0 8px; selection-background-color: #5b5e66; }"
        )
        self._key = None
        self.returnPressed.connect(lambda: self.finish(commit=True))

    def open(self, key: str, value: str, top_left, width: int, height: int):
        self._key = key
        self.setText(value)
        self.selectAll()
        self.setGeometry(top_left.x(), top_left.y(), width, height)
        self.show()
        # Windows only hands the focus to the process in front; the panel
        # never was, so take it the way the paste does.
        inject._focus(int(self.winId()))
        self.activateWindow()
        self.setFocus()

    def finish(self, commit: bool):
        if self._key is None:
            return
        key, self._key = self._key, None
        self.hide()
        if commit:
            self.committed.emit(key, self.text())

    def keyPressEvent(self, event):
        if event.key() == Qt.Key.Key_Escape:
            self.finish(commit=False)
            return
        super().keyPressEvent(event)

    def focusOutEvent(self, event):
        super().focusOutEvent(event)
        self.finish(commit=True)


class VoicePanel(SettingsPanel):
    """Claude's voice (talk mode): which neural voice and how fast. Opens
    next to the dictation panel. Every change plays a sample right away."""

    TITLE = "Claude's voice"

    def _build_rows(self):
        return [
            ("group", "Talk mode", None, None),
            (
                "segment",
                "Voice",
                "tts_voice",
                [
                    ("es-CO-SalomeNeural", "Salomé"),
                    ("es-CO-GonzaloNeural", "Gonzalo"),
                    ("es-MX-DaliaNeural", "Dalia"),
                ],
            ),
            # Six voices don't fit in one row: the second row edits the same
            # setting, and only the row holding the current voice shows a chip.
            (
                "segment",
                "",
                "tts_voice",
                [
                    ("es-MX-JorgeNeural", "Jorge"),
                    ("es-AR-ElenaNeural", "Elena"),
                    ("es-US-AlonsoNeural", "Alonso"),
                ],
            ),
            ("segment", "Speed", "tts_rate", [("-15%", "Slow"), ("+0%", "Normal"), ("+20%", "Fast"), ("+40%", "Faster")]),
            ("slider", "Volume", "tts_volume", None),
            ("toggle", lambda config: f"Start with “{config.get('wake_phrase')}”", "wake_word", None),
            ("text", "Wake phrase", "wake_phrase", None),
            ("toggle", "Speak only when I talk", "speak_only_spoken", None),
        ]

    def _set(self, key: str, value):
        super()._set(key, value)
        if key in ("tts_voice", "tts_rate"):
            self._preview()

    def mouseReleaseEvent(self, event):
        # The volume slider saves on every step while dragging: play the
        # sample once, when the knob is let go.
        dragged = self._dragging_slider
        super().mouseReleaseEvent(event)
        if dragged is not None and self._rows[dragged][2] == "tts_volume":
            self._preview(tts.VOLUME_SAMPLE)

    def _preview(self, sample: str = tts.SAMPLE):
        tts.preview(self.config.get("tts_voice"), self.config.get("tts_rate"), self.config.get("tts_volume"), sample)


# --- bridge with the daemon ------------------------------------------------------


class OverlayBridge(QObject):
    """Thread-safe bridge: worker threads emit signals, the overlay lives
    and paints on the GUI thread (the one running QApplication.exec())."""

    recording_started = Signal()
    recording_stopped = Signal()
    transcribing = Signal()
    finished = Signal(bool)  # True: pasted; False: left on the clipboard
    level_changed = Signal(float)

    def __init__(self, overlay: RecordingOverlay, panel: SettingsPanel):
        super().__init__()
        self.overlay = overlay
        self.panel = panel
        self.settings_changed = panel.settings_changed
        self.capture_started = panel.capture_started
        self.capture_finished = panel.capture_finished
        self.quit_requested = panel.quit_requested
        self.recording_started.connect(overlay.fade_in)
        self.recording_stopped.connect(overlay.fade_out)
        self.transcribing.connect(overlay.show_busy)
        self.finished.connect(overlay.show_result)
        self.level_changed.connect(overlay.set_level)
        panel.settings_changed.connect(lambda key, _value: overlay.restyle() if key in ("glass", "position") else None)
        panel.settings_changed.connect(self._on_capture_setting)

    def _on_capture_setting(self, key, _value):
        if key != "show_in_capture":
            return
        for window in (self.overlay, self.panel, self.panel._companion):
            if window is not None:
                window.apply_capture_mode()


def create_app_and_overlay(config: cfg.Config):
    app = QApplication.instance() or QApplication(sys.argv)
    app.setQuitOnLastWindowClosed(False)
    style = Style(config)
    panel = SettingsPanel(style, companion=VoicePanel(style))
    overlay = RecordingOverlay(style, panel)
    bridge = OverlayBridge(overlay, panel)
    return app, bridge
