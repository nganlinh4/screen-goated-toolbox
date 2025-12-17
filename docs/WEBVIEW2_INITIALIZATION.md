# WebView2 Initialization on Windows - Critical Notes

## The Problem

WebView2 (`wry` crate) creation can hang indefinitely on Windows when:
1. Called from deeply nested spawned threads
2. Called without proper warmup of the WebView2 infrastructure

## Root Cause

WebView2 requires the first `WebViewBuilder::build_as_child()` call to happen in a specific context:
- Thread spawned **directly from the main thread** 
- With a proper message loop running
- Window styles matching: `WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED` (NO `WS_EX_NOACTIVATE`)
- Base style: `WS_POPUP` (NO `WS_CLIPCHILDREN`)

When the first WebView is created from a thread that was spawned several levels deep in the call stack (e.g., hotkey handler → capture thread → process thread → result window thread), the WebView2 controller initialization hangs at `CreateCoreWebView2Controller`.

## The Solution: Warmup Pattern

Follow the same pattern as `text_input.rs`:

1. **Call warmup at app startup** (in `main.rs`):
```rust
overlay::result::markdown_view::warmup();
```

2. **Warmup spawns a dedicated thread** from the main thread context:
```rust
pub fn warmup() {
    std::thread::spawn(|| {
        warmup_internal();
    });
}
```

3. **Create a hidden window with WebView** in that thread:
```rust
fn warmup_internal() {
    // Create hidden window
    let hwnd = CreateWindowExW(
        WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_LAYERED,
        class_name,
        w!("MarkdownWarmup"),
        WS_POPUP,
        0, 0, 100, 100,
        None, None, instance, None
    );
    
    // Make transparent
    SetLayeredWindowAttributes(hwnd, COLORREF(0), 0, LWA_ALPHA);
    
    // Create WebView - this "warms up" the WebView2 infrastructure
    let result = WebViewBuilder::new()
        .with_bounds(...)
        .with_html("<html><body>Warmup</body></html>")
        .with_transparent(false)
        .build_as_child(&wrapper);
    
    // Run message loop forever to keep thread alive
    while GetMessageW(&mut msg, None, 0, 0).into() {
        TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
}
```

4. **After warmup succeeds**, all subsequent WebView2 creations work - even from deeply nested threads!

## Why This Works

The first WebView2 creation initializes the shared WebView2 runtime infrastructure. Once initialized from a "good" thread context (spawned directly from main), all other threads can successfully create WebViews.

## Window Style Requirements for WebView2

| Style | Required for WebView2 |
|-------|----------------------|
| `WS_EX_NOACTIVATE` | ❌ AVOID - blocks initialization |
| `WS_CLIPCHILDREN` | ❌ AVOID - can interfere |
| `WS_EX_LAYERED` | ✅ OK |
| `WS_EX_TOOLWINDOW` | ✅ OK |
| `WS_EX_TOPMOST` | ✅ OK |
| `WS_POPUP` | ✅ Use as base style |

## Debug Tips

If WebView creation hangs:
1. Check if `[WARMUP] WebView created successfully!` appears at startup
2. If not, the warmup itself is failing
3. Add `eprintln!` at each step to find where it hangs
4. The typical hang point is Step 6 (`build_as_child()`)

## Related Files

- `src/overlay/result/markdown_view.rs` - Contains `warmup()` function
- `src/overlay/text_input.rs` - Reference implementation that works
- `src/main.rs` - Where warmup is called

---
*This issue was debugged in December 2024. The fix involved extensive investigation of thread spawning hierarchies, window styles, and COM initialization.*
