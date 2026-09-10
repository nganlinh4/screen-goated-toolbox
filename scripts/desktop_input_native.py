"""Win32 support for the interactive compositor input regression."""
import ctypes as c
from ctypes import wintypes as w
import time

user = c.WinDLL('user32', use_last_error=True)
WNDPROC = c.WINFUNCTYPE(c.c_ssize_t, w.HWND, w.UINT, c.c_size_t, c.c_ssize_t)
ENUMPROC = c.WINFUNCTYPE(w.BOOL, w.HWND, c.c_ssize_t)


class WNDCLASS(c.Structure):
    _fields_ = [('style', w.UINT), ('proc', WNDPROC), ('cls_extra', c.c_int),
               ('win_extra', c.c_int), ('instance', w.HINSTANCE), ('icon', w.HICON),
               ('cursor', w.HANDLE), ('brush', w.HBRUSH), ('menu', w.LPCWSTR),
               ('name', w.LPCWSTR)]


class MOUSEINPUT(c.Structure):
    _fields_ = [('dx', w.LONG), ('dy', w.LONG), ('data', w.DWORD),
               ('flags', w.DWORD), ('time', w.DWORD), ('extra', c.c_size_t)]


class KEYINPUT(c.Structure):
    _fields_ = [('vk', w.WORD), ('scan', w.WORD), ('flags', w.DWORD),
               ('time', w.DWORD), ('extra', c.c_size_t)]


class INPUTUNION(c.Union):
    _fields_ = [('mouse', MOUSEINPUT), ('key', KEYINPUT)]


class INPUT(c.Structure):
    _anonymous_ = ('value',)
    _fields_ = [('kind', w.DWORD), ('value', INPUTUNION)]


def bind(name, result, *arguments):
    fn = getattr(user, name)
    fn.restype, fn.argtypes = result, arguments
    return fn


bind('EnumWindows', w.BOOL, ENUMPROC, c.c_ssize_t)
bind('GetClassNameW', c.c_int, w.HWND, w.LPWSTR, c.c_int)
bind('GetWindowRect', w.BOOL, w.HWND, c.POINTER(w.RECT))
bind('IsWindowVisible', w.BOOL, w.HWND)
bind('WindowFromPoint', w.HWND, w.POINT)
bind('GetAncestor', w.HWND, w.HWND, w.UINT)
bind('GetWindowThreadProcessId', w.DWORD, w.HWND, c.POINTER(w.DWORD))
bind('GetCursorPos', w.BOOL, c.POINTER(w.POINT))
bind('SetCursorPos', w.BOOL, c.c_int, c.c_int)
bind('SendInput', w.UINT, w.UINT, c.POINTER(INPUT), c.c_int)
bind('RegisterClassW', w.ATOM, c.POINTER(WNDCLASS))
bind('CreateWindowExW', w.HWND, w.DWORD, w.LPCWSTR, w.LPCWSTR, w.DWORD,
     c.c_int, c.c_int, c.c_int, c.c_int, w.HWND, w.HMENU, w.HINSTANCE, c.c_void_p)
bind('DefWindowProcW', c.c_ssize_t, w.HWND, w.UINT, c.c_size_t, c.c_ssize_t)
bind('DestroyWindow', w.BOOL, w.HWND)
bind('ShowWindow', w.BOOL, w.HWND, c.c_int)
bind('SetWindowPos', w.BOOL, w.HWND, w.HWND, c.c_int, c.c_int, c.c_int, c.c_int, w.UINT)
bind('PeekMessageW', w.BOOL, c.POINTER(w.MSG), w.HWND, w.UINT, w.UINT, w.UINT)
bind('DispatchMessageW', c.c_ssize_t, c.POINTER(w.MSG))
bind('GetForegroundWindow', w.HWND)
bind('SetForegroundWindow', w.BOOL, w.HWND)


def class_name(hwnd):
    text = c.create_unicode_buffer(256)
    user.GetClassNameW(hwnd, text, len(text))
    return text.value


def windows():
    found = []
    @ENUMPROC
    def collect(hwnd, _):
        if user.IsWindowVisible(hwnd):
            rect, pid = w.RECT(), w.DWORD()
            user.GetWindowRect(hwnd, c.byref(rect))
            user.GetWindowThreadProcessId(hwnd, c.byref(pid))
            found.append(dict(hwnd=hwnd, pid=pid.value, cls=class_name(hwnd),
                              rect=[rect.left, rect.top, rect.right, rect.bottom]))
        return True
    user.EnumWindows(collect, 0)
    return found


def pump(seconds=0.05):
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        message = w.MSG()
        while user.PeekMessageW(c.byref(message), None, 0, 0, 1):
            user.DispatchMessageW(c.byref(message))
        time.sleep(0.005)


def hit(point):
    hwnd = user.WindowFromPoint(w.POINT(*point))
    root = user.GetAncestor(hwnd, 2)
    return dict(hwnd=hwnd, cls=class_name(hwnd), root=root, rootClass=class_name(root))


def desktop_point():
    x, y, width, height = [user.GetSystemMetrics(i) for i in (76, 77, 78, 79)]
    for px in range(x + 24, x + width - 24, 64):
        for py in range(y + 24, y + height - 24, 64):
            point = (px, py)
            if hit(point)['rootClass'] in ('Progman', 'WorkerW'):
                return point
    raise RuntimeError('No exposed desktop point; expose part of the desktop and rerun')


def send(*events):
    inputs = (INPUT * len(events))(*events)
    if user.SendInput(len(inputs), inputs, c.sizeof(INPUT)) != len(inputs):
        raise RuntimeError('SendInput did not deliver every test input')


def escape():
    send(INPUT(kind=1, key=KEYINPUT(vk=27)), INPUT(kind=1, key=KEYINPUT(vk=27, flags=2)))
    pump()


def right_click(point):
    if not user.SetCursorPos(*point):
        raise RuntimeError('Cannot place test cursor')
    send(INPUT(kind=0, mouse=MOUSEINPUT(flags=8)), INPUT(kind=0, mouse=MOUSEINPUT(flags=16)))


def context_menu(point):
    before = {entry['hwnd'] for entry in windows()}
    right_click(point)
    popup = None
    for _ in range(40):
        pump()
        popup = next((entry for entry in windows()
                      if entry['hwnd'] not in before and entry['cls'] in
                      ('#32768', 'Xaml_WindowedPopupClass', 'Windows.UI.Core.CoreWindow')), None)
        if popup:
            break
    if popup is None:
        escape()
        raise RuntimeError(f'No shell context menu observed at {point}; input check failed or inconclusive')
    # Shell popups can become visible before their opening transition accepts keys.
    pump(0.2)
    escape()
    deadline = time.monotonic() + 2
    while user.IsWindowVisible(popup['hwnd']) and time.monotonic() < deadline:
        pump(0.2)
        escape()
    if user.IsWindowVisible(popup['hwnd']):
        raise RuntimeError('Shell context menu did not dismiss')
    return popup['cls']


class Receiver:
    def __init__(self, point):
        self.clicks = 0
        @WNDPROC
        def receive(hwnd, message, wp, lp):
            if message == 0x205:
                self.clicks += 1
                return 0
            return user.DefWindowProcW(hwnd, message, wp, lp)
        self.proc = receive
        definition = WNDCLASS(proc=receive, name='SGTDesktopInputRegressionReceiver')
        user.RegisterClassW(c.byref(definition))
        self.hwnd = user.CreateWindowExW(0x08000080, definition.name, 'Input regression',
                                        0x80000000, point[0] - 8, point[1] - 8,
                                        160, 100, None, None, None, None)
        if not self.hwnd:
            raise c.WinError(c.get_last_error())

    def check(self, point):
        before = self.clicks
        user.ShowWindow(self.hwnd, 4)
        try:
            # ShowWindow does not guarantee raising a previously shown window.
            # HWND_TOP keeps this ordinary window in the non-topmost band.
            if not user.SetWindowPos(self.hwnd, None, 0, 0, 0, 0, 0x13):
                raise c.WinError(c.get_last_error())
            pump()
            if hit(point)['root'] != self.hwnd:
                raise RuntimeError(f'Normal application receiver is obscured: {hit(point)}')
            right_click(point)
            pump(0.15)
            if self.clicks != before + 1:
                raise RuntimeError('Normal application did not receive the physical right click')
        finally:
            user.ShowWindow(self.hwnd, 0)

    def close(self):
        user.DestroyWindow(self.hwnd)
