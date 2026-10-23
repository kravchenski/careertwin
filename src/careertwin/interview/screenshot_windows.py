"""Screen capture on Windows through GDI, with no extra package.

The window hides itself before it calls this (see the overlay), so it is not in the picture.
"""

from __future__ import annotations

import ctypes
import struct
import zlib
from ctypes import wintypes

import numpy as np

SM_CXSCREEN, SM_CYSCREEN = 0, 1
SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN = 76, 77, 78, 79
SRCCOPY, CAPTUREBLT = 0x00CC0020, 0x40000000
BI_RGB, DIB_RGB_COLORS = 0, 0


class BITMAPINFOHEADER(ctypes.Structure):
    _fields_ = [
        ("biSize", wintypes.DWORD), ("biWidth", wintypes.LONG), ("biHeight", wintypes.LONG),
        ("biPlanes", wintypes.WORD), ("biBitCount", wintypes.WORD), ("biCompression", wintypes.DWORD),
        ("biSizeImage", wintypes.DWORD), ("biXPelsPerMeter", wintypes.LONG),
        ("biYPelsPerMeter", wintypes.LONG), ("biClrUsed", wintypes.DWORD),
        ("biClrImportant", wintypes.DWORD),
    ]


class CaptureError(RuntimeError):
    """Windows would not hand over the screen."""


def bgra_to_rgb(pixels: np.ndarray) -> np.ndarray:
    """GDI's top-down 32-bit BGRA rows (height, width, 4) to RGB."""
    return np.ascontiguousarray(pixels[:, :, 2::-1])


def encode_png(rgb: np.ndarray) -> bytes:
    """An 8-bit RGB image (height, width, 3) as PNG bytes; zlib level 3 keeps a 4K screen quick."""
    height, width, _ = rgb.shape
    rows = np.zeros((height, width * 3 + 1), dtype=np.uint8)  # each row starts with filter type 0
    rows[:, 1:] = rgb.reshape(height, width * 3)

    def chunk(tag: bytes, data: bytes) -> bytes:
        return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", zlib.crc32(tag + data))

    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(rows.tobytes(), 3))
        + chunk(b"IEND", b"")
    )


def _libraries():
    """Private handles, so the argument types set here do not leak into other users of the DLLs."""
    user32, gdi32 = ctypes.WinDLL("user32", use_last_error=True), ctypes.WinDLL("gdi32", use_last_error=True)
    user32.GetDC.argtypes, user32.GetDC.restype = [wintypes.HWND], wintypes.HDC
    user32.ReleaseDC.argtypes = [wintypes.HWND, wintypes.HDC]
    gdi32.CreateCompatibleDC.argtypes, gdi32.CreateCompatibleDC.restype = [wintypes.HDC], wintypes.HDC
    gdi32.CreateCompatibleBitmap.argtypes = [wintypes.HDC, ctypes.c_int, ctypes.c_int]
    gdi32.CreateCompatibleBitmap.restype = wintypes.HBITMAP
    gdi32.SelectObject.argtypes, gdi32.SelectObject.restype = [wintypes.HDC, wintypes.HGDIOBJ], wintypes.HGDIOBJ
    gdi32.BitBlt.argtypes = [wintypes.HDC, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_int,
                             wintypes.HDC, ctypes.c_int, ctypes.c_int, wintypes.DWORD]
    gdi32.BitBlt.restype = wintypes.BOOL
    gdi32.GetDIBits.argtypes = [wintypes.HDC, wintypes.HBITMAP, wintypes.UINT, wintypes.UINT,
                                ctypes.c_void_p, ctypes.POINTER(BITMAPINFOHEADER), wintypes.UINT]
    gdi32.GetDIBits.restype = ctypes.c_int
    gdi32.DeleteObject.argtypes = [wintypes.HGDIOBJ]
    gdi32.DeleteDC.argtypes = [wintypes.HDC]
    return user32, gdi32


def read_pixels(gdi32, memory, bitmap, width: int, height: int) -> np.ndarray:
    """The pixels of a bitmap selected into a memory DC, as top-down BGRA rows (height, width, 4)."""
    header = BITMAPINFOHEADER(
        biSize=ctypes.sizeof(BITMAPINFOHEADER), biWidth=width, biHeight=-height,  # negative: top-down
        biPlanes=1, biBitCount=32, biCompression=BI_RGB,
    )
    buffer = ctypes.create_string_buffer(width * height * 4)
    if gdi32.GetDIBits(memory, bitmap, 0, height, buffer, ctypes.byref(header), DIB_RGB_COLORS) != height:
        raise CaptureError("Windows could not read the copied screen")
    return np.frombuffer(buffer, dtype=np.uint8).reshape(height, width, 4)


def capture_png(all_screens: bool = False) -> bytes:
    """The primary screen, or every screen side by side, as PNG bytes."""
    user32, gdi32 = _libraries()
    try:
        user32.SetProcessDPIAware()  # real pixels; a no-op once the window has set its own awareness
    except OSError:
        pass
    if all_screens:
        left, top = user32.GetSystemMetrics(SM_XVIRTUALSCREEN), user32.GetSystemMetrics(SM_YVIRTUALSCREEN)
        width, height = user32.GetSystemMetrics(SM_CXVIRTUALSCREEN), user32.GetSystemMetrics(SM_CYVIRTUALSCREEN)
    else:
        left = top = 0
        width, height = user32.GetSystemMetrics(SM_CXSCREEN), user32.GetSystemMetrics(SM_CYSCREEN)
    if width <= 0 or height <= 0:
        raise CaptureError("Windows reports no screen")
    screen = user32.GetDC(None)
    if not screen:
        raise CaptureError("Windows would not open the screen for reading")
    memory = gdi32.CreateCompatibleDC(screen)
    bitmap = gdi32.CreateCompatibleBitmap(screen, width, height)
    previous = gdi32.SelectObject(memory, bitmap)
    try:
        if not gdi32.BitBlt(memory, 0, 0, width, height, screen, left, top, SRCCOPY | CAPTUREBLT):
            raise CaptureError(f"Windows could not copy the screen (error {ctypes.get_last_error()})")
        pixels = read_pixels(gdi32, memory, bitmap, width, height)
    finally:
        gdi32.SelectObject(memory, previous)
        gdi32.DeleteObject(bitmap)
        gdi32.DeleteDC(memory)
        user32.ReleaseDC(None, screen)
    return encode_png(bgra_to_rgb(pixels))
