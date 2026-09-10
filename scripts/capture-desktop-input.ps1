[CmdletBinding()]
param(
    [string]$OutputPath,
    [ValidateRange(0, 60)]
    [int]$DelaySeconds = 5
)

$ErrorActionPreference = 'Stop'
if ([string]::IsNullOrWhiteSpace($OutputPath)) {
    $OutputPath = Join-Path $env:TEMP ("sgt-desktop-input-{0}.json" -f (Get-Date -Format 'yyyyMMdd-HHmmss-fff'))
}
$OutputPath = [IO.Path]::GetFullPath($OutputPath)
if (Test-Path -LiteralPath $OutputPath) {
    throw "Output already exists: $OutputPath"
}

# Capture native metadata only. Do not activate windows or read their text.
if (-not ('SgtDesktopInputProbe' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;

public static class SgtDesktopInputProbe {
    [StructLayout(LayoutKind.Sequential)] public struct Point { public int X, Y; }
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
    [StructLayout(LayoutKind.Sequential)] struct GuiThreadInfo {
        public uint Size, Flags;
        public IntPtr Active, Focus, Capture, MenuOwner, MoveSize, Caret;
        public Rect CaretRect;
    }
    public delegate bool EnumProc(IntPtr hwnd, IntPtr context);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc callback, IntPtr context);
    [DllImport("user32.dll")] static extern bool EnumChildWindows(IntPtr hwnd, EnumProc callback, IntPtr context);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern int GetClassName(IntPtr hwnd, StringBuilder name, int count);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
    [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr hwnd, out Rect rect);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("user32.dll")] static extern bool IsWindowEnabled(IntPtr hwnd);
    [DllImport("user32.dll", EntryPoint="GetWindowLongW")] static extern int GetWindowLong(IntPtr hwnd, int index);
    [DllImport("user32.dll")] static extern IntPtr GetWindow(IntPtr hwnd, uint command);
    [DllImport("user32.dll")] static extern IntPtr GetAncestor(IntPtr hwnd, uint flags);
    [DllImport("user32.dll")] static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] static extern bool GetGUIThreadInfo(uint threadId, ref GuiThreadInfo info);
    [DllImport("user32.dll")] static extern bool GetCursorPos(out Point point);
    [DllImport("user32.dll")] static extern IntPtr WindowFromPoint(Point point);
    [DllImport("user32.dll")] static extern int GetSystemMetrics(int index);
    [DllImport("user32.dll")] static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)] static extern IntPtr SendMessageTimeout(IntPtr hwnd, uint message, IntPtr wparam, IntPtr lparam, uint flags, uint timeout, out UIntPtr result);
    [DllImport("gdi32.dll")] static extern IntPtr CreateRectRgn(int left, int top, int right, int bottom);
    [DllImport("user32.dll")] static extern int GetWindowRgn(IntPtr hwnd, IntPtr region);
    [DllImport("gdi32.dll")] static extern int GetRgnBox(IntPtr region, out Rect rect);
    [DllImport("gdi32.dll")] static extern bool DeleteObject(IntPtr obj);

    static string Handle(IntPtr hwnd) { return "0x" + hwnd.ToInt64().ToString("x"); }
    static string ClassName(IntPtr hwnd) {
        var name = new StringBuilder(256);
        GetClassName(hwnd, name, name.Capacity);
        return name.ToString();
    }
    static bool IsShell(string name) {
        return name == "Progman" || name == "WorkerW" || name == "SHELLDLL_DefView" || name == "SysListView32"
            || name == "Shell_TrayWnd" || name == "Shell_SecondaryTrayWnd";
    }
    static Dictionary<string, object> Describe(IntPtr hwnd, bool probeShell) {
        uint pid;
        uint tid = GetWindowThreadProcessId(hwnd, out pid);
        Rect bounds;
        bool boundsValid = GetWindowRect(hwnd, out bounds);
        string name = ClassName(hwnd);
        var item = new Dictionary<string, object> {
            {"hwnd", Handle(hwnd)}, {"root", Handle(GetAncestor(hwnd, 2))},
            {"owner", Handle(GetWindow(hwnd, 4))}, {"pid", pid}, {"threadId", tid},
            {"class", name}, {"visible", IsWindowVisible(hwnd)}, {"enabled", IsWindowEnabled(hwnd)},
            {"style", "0x" + unchecked((uint)GetWindowLong(hwnd, -16)).ToString("x8")},
            {"extendedStyle", "0x" + unchecked((uint)GetWindowLong(hwnd, -20)).ToString("x8")},
            {"boundsValid", boundsValid}, {"bounds", bounds}
        };
        var gui = new GuiThreadInfo { Size = (uint)Marshal.SizeOf(typeof(GuiThreadInfo)) };
        if (tid != 0 && GetGUIThreadInfo(tid, ref gui)) {
            item["threadInput"] = new {
                flags = gui.Flags, active = Handle(gui.Active), focus = Handle(gui.Focus),
                capture = Handle(gui.Capture), menuOwner = Handle(gui.MenuOwner), moveSize = Handle(gui.MoveSize)
            };
        }
        IntPtr region = CreateRectRgn(0, 0, 0, 0);
        if (region != IntPtr.Zero) {
            try {
                int kind = GetWindowRgn(hwnd, region);
                item["regionKind"] = kind; // 0: no region/error, 1: empty, 2: rectangle, 3: complex
                Rect regionBounds;
                if (kind != 0 && GetRgnBox(region, out regionBounds) != 0) item["regionBounds"] = regionBounds;
            } finally { DeleteObject(region); }
        }
        if (probeShell && IsShell(name)) {
            UIntPtr result;
            item["respondsWithin200ms"] = SendMessageTimeout(hwnd, 0, IntPtr.Zero, IntPtr.Zero, 2, 200, out result) != IntPtr.Zero;
        }
        return item;
    }
    public static object Capture() {
        IntPtr previousDpi = SetThreadDpiAwarenessContext(new IntPtr(-4));
        try {
            var windows = new List<object>();
            EnumWindows((hwnd, context) => {
                windows.Add(Describe(hwnd, true));
                string name = ClassName(hwnd);
                // Include desktop controls, whose thread may stall independently of other apps.
                if (name == "Progman" || name == "WorkerW") {
                    EnumChildWindows(hwnd, (child, unused) => {
                        if (IsShell(ClassName(child))) windows.Add(Describe(child, true));
                        return true;
                    }, IntPtr.Zero);
                }
                return true;
            }, IntPtr.Zero);
            Point cursor;
            bool cursorValid = GetCursorPos(out cursor);
            var points = new List<object>();
            int x = GetSystemMetrics(76), y = GetSystemMetrics(77);
            int width = GetSystemMetrics(78), height = GetSystemMetrics(79);
            for (int row = 0; row < 3; row++) for (int col = 0; col < 5; col++) {
                var point = new Point { X = x + (2 * col + 1) * width / 10, Y = y + (2 * row + 1) * height / 6 };
                points.Add(new { point, receiver = Describe(WindowFromPoint(point), false) });
            }
            return new {
                capturedUtc = DateTime.UtcNow.ToString("o"),
                virtualDesktop = new { x, y, width, height },
                foreground = Handle(GetForegroundWindow()), cursorValid, cursor,
                cursorReceiver = cursorValid ? Describe(WindowFromPoint(cursor), false) : null,
                sampledReceivers = points, windows
            };
        } finally {
            if (previousDpi != IntPtr.Zero) SetThreadDpiAwarenessContext(previousDpi);
        }
    }
}
'@
}

Write-Host "Place the pointer over the affected desktop. Capturing in $DelaySeconds seconds."
if ($DelaySeconds -gt 0) { Start-Sleep -Seconds $DelaySeconds }
$snapshot = [SgtDesktopInputProbe]::Capture()
$processes = @(Get-CimInstance Win32_Process | Select-Object @{Name='Id'; Expression={$_.ProcessId}},
    @{Name='ProcessName'; Expression={$_.Name}}, ParentProcessId)
@{ native = $snapshot; processes = $processes } |
    ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $OutputPath -Encoding UTF8
Write-Host "Saved native window metadata: $OutputPath"
