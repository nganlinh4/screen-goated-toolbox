"""Exercise real WebView2 controllers against Explorer, taskbars and app input.

Requires an unlocked interactive Windows desktop. --expose-desktop additionally
requires pywin32; it restores the minimized windows when the run finishes.
"""
import argparse
import ctypes
import hashlib
import json
import os
from pathlib import Path
import queue
import subprocess
import threading
import time

import desktop_input_native as native
from dev_cache_paths import managed_cache_root


class Renderer:
    def __init__(self, exe, kind, root):
        root.mkdir(parents=True)
        env = os.environ.copy()
        env['SGT_RUNTIME_STATE_ROOT'] = str(root / 'state')
        env['SGT_CREATION_WEBVIEW2_DATA_DIR'] = str(root / 'webview')
        self.events = queue.Queue()
        self.stderr = (root / 'stderr.txt').open('w', encoding='utf-8')
        self.process = subprocess.Popen(
            [str(exe), f'--internal-{kind}-compositor'], stdin=subprocess.PIPE,
            stdout=subprocess.PIPE, stderr=self.stderr, env=env,
            encoding='utf-8', creationflags=subprocess.CREATE_NO_WINDOW)
        self.thread = threading.Thread(target=self.read, daemon=True)
        self.thread.start()

    def read(self):
        for line in self.process.stdout:
            try:
                self.events.put(json.loads(line))
            except json.JSONDecodeError:
                pass

    def wait(self, predicate, timeout=15):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if self.process.poll() is not None:
                raise RuntimeError(f'Renderer exited with {self.process.returncode}')
            try:
                event = self.events.get(timeout=0.05)
            except queue.Empty:
                continue
            if predicate(event):
                return event
        raise RuntimeError('Renderer event deadline exceeded')

    def send(self, command):
        self.process.stdin.write(json.dumps(command) + '\n')
        self.process.stdin.flush()

    def scene(self, cards, revision):
        self.send(dict(type='snapshot', cards=cards))
        self.send(dict(type='apply_revision', revision=revision))
        self.wait(lambda event: event.get('type') == 'state_acknowledged'
                  and event.get('revision') == revision)

    def close(self):
        try:
            if self.process.poll() is None:
                self.send(dict(type='shutdown'))
                self.process.wait(timeout=5)
        except (OSError, subprocess.TimeoutExpired):
            self.process.kill()
            self.process.wait(timeout=5)
        finally:
            self.thread.join(timeout=2)
            self.stderr.close()


def check_routing(stage, point, receiver, report):
    entry = dict(stage=stage, desktopHit=native.hit(point))
    report['stages'].append(entry)
    if entry['desktopHit']['rootClass'] not in ('Progman', 'WorkerW'):
        raise RuntimeError(f'{stage}: desktop point is intercepted: {entry["desktopHit"]}')
    entry['desktopMenu'] = native.context_menu(point)
    receiver.check(point)
    entry['applicationClick'] = True
    entry['taskbars'] = []
    for bar in native.windows():
        if bar['cls'] not in ('Shell_TrayWnd', 'Shell_SecondaryTrayWnd'):
            continue
        left, top, right, bottom = bar['rect']
        target = ((left + right) // 2, (top + bottom) // 2)
        if native.hit(target)['root'] != bar['hwnd']:
            raise RuntimeError(f'{stage}: taskbar is obscured')
        entry['taskbars'].append(dict(kind=bar['cls'], menu=native.context_menu(target)))
    if not entry['taskbars']:
        raise RuntimeError('No visible taskbar; taskbar acceptance is incomplete')
    print(f'{stage}: desktop menu, app click, {len(entry["taskbars"])} taskbar(s) passed', flush=True)


def run(exe, root, expose, report):
    shell = None
    receiver = None
    children = []
    cursor = native.w.POINT()
    native.user.GetCursorPos(ctypes.byref(cursor))
    foreground = native.user.GetForegroundWindow()
    try:
        if expose:
            import win32com.client
            shell = win32com.client.Dispatch('Shell.Application')
            shell.MinimizeAll()
            native.pump(0.5)
        deadline = time.monotonic() + 5
        while True:
            try:
                point = native.desktop_point()
                break
            except RuntimeError:
                if time.monotonic() >= deadline:
                    raise
                native.pump(0.1)
        report['desktopPoint'] = point
        receiver = native.Receiver(point)
        check_routing('baseline', point, receiver, report)
        x, y, width, height = [native.user.GetSystemMetrics(i) for i in (76, 77, 78, 79)]
        rect = dict(x=width // 2, y=height // 2, width=280, height=120)
        if rect['x'] + x - 32 <= point[0] <= rect['x'] + x + 312:
            rect['x'] = 24
        card = dict(id=7, rect=rect, control_rect=rect, body='<p>Desktop input regression</p>',
                    document=None, refining=False, background='#204060', opacity=100,
                    visible=True, streaming=False, streaming_enabled=False)
        for generation in range(2):
            result = Renderer(exe, 'result', root / f'result-{generation}')
            children.append(result)
            result.wait(lambda event: event.get('type') == 'ready')
            status = Renderer(exe, 'status', root / f'status-{generation}')
            children.append(status)
            status.wait(lambda event: event.get('type') == 'ready')
            result.scene([], 1)
            check_routing(f'{generation}:initialized_empty', point, receiver, report)
            result.scene([card], 2)
            result.wait(lambda event: event.get('type') == 'card_diagnostic'
                        and event.get('phase') == 'final_fit_completed')
            status.send(dict(type='notification_add', rect={**rect, 'x': rect['x'] + x, 'y': rect['y'] + y},
                             notification=dict(id=1, title='Input regression', snippet='',
                                               kind='success', duration_ms=1500)))
            deadline = time.monotonic() + 2
            while not any(window['pid'] == status.process.pid
                          and window['cls'] == 'SGTStatusCompositorDComp'
                          for window in native.windows()):
                if time.monotonic() >= deadline:
                    raise RuntimeError('Notification visual host did not become visible')
                native.pump()
            check_routing(f'{generation}:visible_content', point, receiver, report)
            status.wait(lambda event: event.get('type') == 'notification_finished')
            check_routing(f'{generation}:notification_expired', point, receiver, report)
            result.scene([], 3)
            check_routing(f'{generation}:dismissed', point, receiver, report)
            result.scene([card], 4)
            result.wait(lambda event: event.get('type') == 'card_diagnostic'
                        and event.get('phase') == 'final_fit_completed')
            check_routing(f'{generation}:reopened', point, receiver, report)
            # Recreate both isolated renderers, including fresh browser profiles.
            status.close()
            result.close()
            children.clear()
        check_routing('renderers_closed', point, receiver, report)
    finally:
        for child in reversed(children):
            child.close()
        if receiver:
            receiver.close()
        if shell:
            shell.UndoMinimizeALL()
        native.user.SetCursorPos(cursor.x, cursor.y)
        if foreground:
            native.user.SetForegroundWindow(foreground)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--exe', type=Path, required=True)
    parser.add_argument('--output', type=Path)
    parser.add_argument('--expose-desktop', action='store_true')
    args = parser.parse_args()
    exe = args.exe.resolve(strict=True)
    root = (args.output or managed_cache_root() / 'evidence'
            / ('desktop-input-' + time.strftime('%Y%m%d-%H%M%S'))).resolve()
    root.mkdir(parents=True, exist_ok=False)
    report = dict(exe=str(exe), sha256=hashlib.sha256(exe.read_bytes()).hexdigest(),
                  stages=[], passed=False)
    try:
        run(exe, root, args.expose_desktop, report)
        report['passed'] = True
    except Exception as error:
        report['error'] = str(error)
        print(f'FAILED: {error}', flush=True)
    finally:
        (root / 'result.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
    return 0 if report['passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
