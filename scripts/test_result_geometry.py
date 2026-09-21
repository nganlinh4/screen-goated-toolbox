"""Verify result geometry with real WebView2, physical input, and an isolated profile.

Requires websocket-client and an unlocked desktop. CDP is enabled only for this
test process; no debugging endpoint is added to production code.
"""
import argparse
import base64
import ctypes
import hashlib
import json
import os
from pathlib import Path
import socket
import time
import urllib.request

import websocket
import desktop_input_native as native
from dev_cache_paths import managed_cache_root
from test_desktop_compositor_input import Renderer


class Browser:
    def __init__(self, port):
        deadline = time.monotonic() + 15
        while True:
            try:
                with urllib.request.urlopen(f'http://127.0.0.1:{port}/json', timeout=1) as response:
                    pages = json.load(response)
                page = next(p for p in pages if p['type'] == 'page' and '/index.html' in p['url'])
                self.socket = websocket.create_connection(page['webSocketDebuggerUrl'],
                                                          timeout=5, suppress_origin=True)
                self.sequence = 0
                break
            except (OSError, StopIteration):
                if time.monotonic() > deadline:
                    raise
                time.sleep(0.1)

    def call(self, method, **params):
        self.sequence += 1
        self.socket.send(json.dumps(dict(id=self.sequence, method=method, params=params)))
        while True:
            reply = json.loads(self.socket.recv())
            if reply.get('id') != self.sequence:
                continue
            if 'error' in reply:
                raise RuntimeError(reply['error'])
            return reply['result']

    def evaluate(self, expression):
        result = self.call('Runtime.evaluate', expression=expression, returnByValue=True)
        if 'exceptionDetails' in result:
            raise RuntimeError(result['exceptionDetails'])
        return result['result'].get('value')

    def bounds(self):
        return self.evaluate("""(()=>{
          const entry=cards.get('-7001'), r=entry.card.getBoundingClientRect(), s=devicePixelRatio;
          return {x:r.x*s,y:r.y*s,width:r.width*s,height:r.height*s,
            preview:window.__SGT_BUTTON_SCENE__.isGeometryPreviewActive()};
        })()""")

    def screenshot(self, path):
        path.write_bytes(base64.b64decode(self.call('Page.captureScreenshot')['data']))


def assert_bounds(browser, expected):
    actual = browser.bounds()
    for key, value in expected.items():
        if abs(actual[key] - value) > 1:
            raise AssertionError(dict(expected=expected, actual=actual))
    if actual['preview']:
        raise AssertionError('Gesture did not settle')
    return actual


def mouse_button(down):
    native.send(native.INPUT(kind=0, mouse=native.MOUSEINPUT(flags=2 if down else 4)))


def run(exe, root, report):
    with socket.socket() as listener:
        listener.bind(('127.0.0.1', 0))
        port = listener.getsockname()[1]
    saved = {name: os.environ.get(name) for name in
             ['WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS', 'SGT_VERBOSE_LOGS']}
    set_dpi_context = native.bind('SetThreadDpiAwarenessContext', ctypes.c_void_p, ctypes.c_void_p)
    previous_dpi_context = set_dpi_context(-4)
    cursor = native.w.POINT()
    native.user.GetCursorPos(ctypes.byref(cursor))
    renderer = browser = None
    try:
        os.environ['WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS'] = f'--remote-debugging-port={port}'
        os.environ['SGT_VERBOSE_LOGS'] = '1'
        renderer = Renderer(exe, 'result', root / 'renderer')
        renderer.wait(lambda event: event.get('type') == 'ready')
        browser = Browser(port)
        rect = dict(x=180, y=180, width=520, height=260)
        card = dict(id=-7001, rect=rect.copy(), control_rect=rect.copy(),
                    body='<p>Result geometry remains aligned through interruption and recovery.</p>',
                    document=None, refining=False, background='#204060', opacity=100,
                    visible=True, streaming=False, streaming_enabled=False)
        renderer.scene([card], 1)
        renderer.wait(lambda event: event.get('phase') == 'final_fit_completed')
        revision = 1
        for kind in ['drag_cancel', 'drag_move', 'resize_cancel', 'resize_move']:
            selector = '.result-handle' if kind.startswith('drag') else '.resize-handle[data-edge="se"]'
            point = browser.evaluate(f"""(()=>{{const r=document.querySelector({json.dumps(selector)}).getBoundingClientRect();
                return [Math.round((r.x+r.width/2)*devicePixelRatio),Math.round((r.y+r.height/2)*devicePixelRatio)];}})()""")
            if not native.user.SetCursorPos(*point):
                raise RuntimeError(f'Windows refused cursor positioning (error {ctypes.get_last_error()}); interactive input is unavailable')
            native.pump(1)
            target = native.hit(point)
            owned = [w for w in native.windows() if w['pid'] == renderer.process.pid]
            if target['root'] not in [w['hwnd'] for w in owned]:
                browser.screenshot(root / f'{kind}-obscured.png')
                state = browser.evaluate("({cursorX,cursorY,scale:devicePixelRatio,opacity:document.querySelector('.button-group').style.opacity})")
                raise RuntimeError(f'Test target obscured: {dict(point=point, target=target, owned=owned, bounds=browser.bounds(), state=state)}')
            mouse_button(True)
            renderer.wait(lambda event: event.get('type') == 'drag_started')
            native.user.SetCursorPos(point[0] + 70, point[1] + 40)
            native.pump(0.25)
            if kind.endswith('cancel'):
                # Send the ordinary capture-loss message only to the test renderer.
                post = native.bind('PostMessageW', native.w.BOOL, native.w.HWND,
                                   native.w.UINT, ctypes.c_size_t, ctypes.c_ssize_t)
                post(target['root'], 0x001F, 0, 0)  # WM_CANCELMODE
            else:
                mouse_button(False)
            event = renderer.wait(lambda e: e.get('type') in ['drag_finished', 'resize_finished'])
            mouse_button(False)
            if kind.endswith('cancel'):
                if event.get('outcome') != 'cancelled':
                    raise AssertionError(event)
            elif kind.startswith('resize'):
                rect = event['rect']
            else:
                rect = {**rect, 'x': rect['x'] + event['dx'], 'y': rect['y'] + event['dy']}
            card.update(rect=rect.copy(), control_rect=rect.copy())
            renderer.send(dict(type='drag_settled', gesture_id=event['gesture_id'], cards=[
                dict(id=-7001, rect=rect.copy(), control_rect=rect.copy(), visible=True)]))
            revision += 1
            renderer.send(dict(type='apply_revision', revision=revision))
            renderer.wait(lambda e: e.get('type') == 'state_acknowledged' and e.get('revision') == revision)
            report['stages'].append(dict(stage=kind, bounds=assert_bounds(browser, rect)))
            browser.screenshot(root / f'{kind}.png')
            print(f'{kind}: physical input and browser geometry passed', flush=True)

        before = browser.evaluate('devicePixelRatio')
        native.user.SetCursorPos(rect['x'] + 50, rect['y'] + 50)
        native.send(native.INPUT(kind=1, key=native.KEYINPUT(vk=17)))
        try:
            native.send(native.INPUT(kind=0, mouse=native.MOUSEINPUT(flags=0x800, data=120)))
            native.pump(0.3)
        finally:
            native.send(native.INPUT(kind=1, key=native.KEYINPUT(vk=17, flags=2)))
        if browser.evaluate('devicePixelRatio') != before:
            raise AssertionError('Ctrl+wheel changed shared compositor zoom')
        report['stages'].append(dict(stage='zoom_locked', bounds=assert_bounds(browser, rect)))

        browser.evaluate("cards.get('-7001').card.style.transform='translate3d(10px,10px,0)'")
        revision += 1
        renderer.send(dict(type='apply_revision', revision=revision))
        renderer.wait(lambda e: e.get('type') == 'state_acknowledged' and e.get('revision') == revision)
        report['stages'].append(dict(stage='geometry_repaired_before_ack', bounds=assert_bounds(browser, rect)))
        browser.screenshot(root / 'repaired.png')
        # A failing browser operation must report failure and never acknowledge its revision.
        browser.evaluate("window.__SGT_VERIFY_SCENE_GEOMETRY__=()=>{throw new Error('geometry verification failed')} ")
        revision += 1
        renderer.send(dict(type='apply_revision', revision=revision))
        renderer.wait(lambda e: e.get('type') == 'command_error' and e.get('command') == 'scene_batch')
        try:
            renderer.wait(lambda e: e.get('type') == 'state_acknowledged' and e.get('revision') == revision, timeout=0.5)
        except RuntimeError as error:
            if str(error) != 'Renderer event deadline exceeded':
                raise
        else:
            raise AssertionError('Failed browser revision was acknowledged')
        report['stages'].append(dict(stage='failed_revision_not_acknowledged'))
    finally:
        mouse_button(False)
        if browser:
            browser.socket.close()
        if renderer:
            renderer.close()
        native.user.SetCursorPos(cursor.x, cursor.y)
        if previous_dpi_context:
            set_dpi_context(previous_dpi_context)
        for name, value in saved.items():
            if value is None:
                os.environ.pop(name, None)
            else:
                os.environ[name] = value


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--exe', type=Path, required=True)
    args = parser.parse_args()
    exe = args.exe.resolve(strict=True)
    root = managed_cache_root() / 'evidence' / ('result-geometry-' + time.strftime('%Y%m%d-%H%M%S'))
    root.mkdir(parents=True, exist_ok=False)
    report = dict(exe=str(exe), sha256=hashlib.sha256(exe.read_bytes()).hexdigest(), stages=[], passed=False)
    try:
        run(exe, root, report)
        report['passed'] = True
    except Exception as error:
        report['error'] = str(error)
        print(f'FAILED: {error}', flush=True)
    (root / 'result.json').write_text(json.dumps(report, indent=2), encoding='utf-8')
    print(root, flush=True)
    return 0 if report['passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
