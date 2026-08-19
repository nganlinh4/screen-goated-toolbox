import json
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path


LOCAL_APP_DATA = Path(os.environ["LOCALAPPDATA"])
DEV_CACHE = LOCAL_APP_DATA / "SGT-Development" / "cache"
EXECUTABLE = DEV_CACHE / "cargo" / "dev" / "debug" / "screen-goated-toolbox.exe"
EVIDENCE_ROOT = DEV_CACHE / "evidence" / "screen-translate" / "runs"
QUEUE_ROOT = DEV_CACHE / "screen-translate-lab-queue"
HOST_PROCESS = None


def show_source(image_path: Path, ready_path: Path) -> None:
    import ctypes
    import tkinter as tk
    from PIL import Image, ImageTk

    try:
        ctypes.windll.user32.SetProcessDpiAwarenessContext(ctypes.c_void_p(-4))
    except OSError:
        pass
    image = Image.open(image_path).convert("RGB")
    root = tk.Tk()
    root.overrideredirect(True)
    root.geometry(f"{image.width}x{image.height}+420+160")
    root.attributes("-topmost", True)
    photo = ImageTk.PhotoImage(image)
    canvas = tk.Canvas(root, width=image.width, height=image.height, highlightthickness=0)
    canvas.pack()
    canvas.create_image(0, 0, image=photo, anchor="nw")
    root.update_idletasks()
    root.update()
    ready_path.write_text("ready", encoding="utf-8")
    root.mainloop()


def newest_completed_run(started_ns: int, known: set[str]) -> Path | None:
    if not EVIDENCE_ROOT.is_dir():
        return None
    candidates = []
    for directory in EVIDENCE_ROOT.iterdir():
        record = directory / "run.json"
        if directory.name in known or not record.is_file():
            continue
        if record.stat().st_mtime_ns < started_ns:
            continue
        candidates.append(directory)
    return max(candidates, key=lambda path: path.stat().st_mtime_ns) if candidates else None


def ensure_warm_host() -> subprocess.Popen:
    global HOST_PROCESS
    if HOST_PROCESS is not None and HOST_PROCESS.poll() is None:
        return HOST_PROCESS
    QUEUE_ROOT.mkdir(parents=True, exist_ok=True)
    (QUEUE_ROOT / "request.json").unlink(missing_ok=True)
    environment = os.environ.copy()
    environment["SGT_DEV_CACHE_ROOT"] = str(DEV_CACHE)
    environment["SGT_SCREEN_TRANSLATE_AUTO_EVIDENCE"] = "1"
    HOST_PROCESS = subprocess.Popen(
        [str(EXECUTABLE), "--screen-translate-lab-queue", str(QUEUE_ROOT)],
        env=environment,
    )
    return HOST_PROCESS


def run(case_directory: Path, timeout_seconds: int = 90) -> dict:
    global HOST_PROCESS
    source = case_directory / "source.jpg"
    if not source.is_file():
        raise RuntimeError("This case has no source.jpg")
    if not EXECUTABLE.is_file():
        raise RuntimeError("Run .\\run-dev.ps1 once so the development executable exists")

    EVIDENCE_ROOT.mkdir(parents=True, exist_ok=True)
    known = {path.name for path in EVIDENCE_ROOT.iterdir() if path.is_dir()}
    output = case_directory / "production-preview"
    output.mkdir(exist_ok=True)
    ready = output / "source-window.ready"
    ready.unlink(missing_ok=True)
    source_process = subprocess.Popen(
        [sys.executable, str(Path(__file__).resolve()), "source-window", str(source), str(ready)],
        creationflags=subprocess.CREATE_NO_WINDOW,
    )
    host = None
    one_shot_host = None
    started = time.time()
    try:
        while not ready.is_file():
            if source_process.poll() is not None:
                raise RuntimeError("The production source window failed to open")
            if time.time() - started > 8:
                raise RuntimeError("Timed out opening the production source window")
            time.sleep(0.05)

        host = ensure_warm_host()
        started_ns = time.time_ns()
        request = QUEUE_ROOT / "request.json"
        temporary = QUEUE_ROOT / "request.json.tmp"
        temporary.write_text(json.dumps({"image": str(source)}), encoding="utf-8")
        os.replace(temporary, request)
        run_directory = None
        while run_directory is None:
            if host.poll() is not None:
                raise RuntimeError(f"Production host exited with code {host.returncode}")
            if time.time() - started > 8 and request.is_file():
                request.unlink(missing_ok=True)
                host.terminate()
                host.wait(timeout=3)
                HOST_PROCESS = None
                environment = os.environ.copy()
                environment["SGT_DEV_CACHE_ROOT"] = str(DEV_CACHE)
                environment["SGT_SCREEN_TRANSLATE_AUTO_EVIDENCE"] = "1"
                one_shot_host = subprocess.Popen(
                    [str(EXECUTABLE), "--screen-translate-ui-test", str(source)],
                    env=environment,
                )
                host = one_shot_host
            run_directory = newest_completed_run(started_ns, known)
            if time.time() - started > timeout_seconds:
                raise RuntimeError("Production translation timed out")
            time.sleep(0.1)

        record = json.loads((run_directory / "run.json").read_text(encoding="utf-8"))
        if record.get("status") != "complete" or not (run_directory / "result.jpg").is_file():
            message = record.get("error") or record.get("status") or "unknown production failure"
            raise RuntimeError(str(message))
        shutil.copy2(run_directory / "result.jpg", output / "result.jpg")
        shutil.copy2(run_directory / "run.json", output / "run.json")
        shutil.copy2(run_directory / "source.jpg", output / "source.jpg")
        return {
            "resultUrl": f"inputs/{case_directory.name}/production-preview/result.jpg",
            "record": record,
            "elapsedMs": round((time.time() - started) * 1000),
        }
    finally:
        ready.unlink(missing_ok=True)
        for process in (one_shot_host, source_process):
            if process is not None and process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    process.kill()


def rerender(case_directory: Path, timeout_seconds: int = 20) -> dict:
    output = case_directory / "production-preview"
    source = output / "source.jpg"
    if not source.is_file() or not (output / "run.json").is_file():
        raise RuntimeError("Run production once before replaying layout")
    host = ensure_warm_host()
    ready = output / "source-window.ready"
    done = output / "replay-done.json"
    replay_image = output / "replay.jpg"
    for path in (ready, done):
        path.unlink(missing_ok=True)
    source_process = subprocess.Popen(
        [sys.executable, str(Path(__file__).resolve()), "source-window", str(source), str(ready)],
        creationflags=subprocess.CREATE_NO_WINDOW,
    )
    started = time.time()
    try:
        while not ready.is_file():
            if source_process.poll() is not None or time.time() - started > 8:
                raise RuntimeError("The production source window failed to open")
            time.sleep(0.05)
        request = QUEUE_ROOT / "request.json"
        temporary = QUEUE_ROOT / "request.json.tmp"
        temporary.write_text(json.dumps({
            "action": "replay",
            "runDirectory": str(output),
            "output": str(replay_image),
            "done": str(done),
        }), encoding="utf-8")
        os.replace(temporary, request)
        while not done.is_file():
            if host.poll() is not None:
                raise RuntimeError("Production replay host stopped")
            if time.time() - started > 8 and request.is_file():
                raise RuntimeError("Rerun .\\run-dev.ps1 once to enable layout replay")
            if time.time() - started > timeout_seconds:
                raise RuntimeError("Production layout replay timed out")
            time.sleep(0.05)
        result = json.loads(done.read_text(encoding="utf-8"))
        if result.get("status") != "complete" or not replay_image.is_file():
            raise RuntimeError(result.get("error", "Production layout replay failed"))
        shutil.copy2(replay_image, output / "result.jpg")
        return {
            "resultUrl": f"inputs/{case_directory.name}/production-preview/result.jpg",
            "elapsedMs": round((time.time() - started) * 1000),
            "renderedRegionCount": result.get("rendered", 0),
        }
    finally:
        ready.unlink(missing_ok=True)
        if source_process.poll() is None:
            source_process.terminate()


if __name__ == "__main__" and len(sys.argv) == 4 and sys.argv[1] == "source-window":
    show_source(Path(sys.argv[2]), Path(sys.argv[3]))
