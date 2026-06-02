use windows::Win32::Foundation::*;

use super::super::WEBVIEWS;

const FIT_FONT_SCRIPT: &str = concat!(
    include_str!("fit_impl/fit_font_script_part1.js"),
    include_str!("fit_impl/fit_font_script_part2.js"),
);

pub fn fit_font_to_window(parent_hwnd: HWND) {
    fit_font_to_window_ex(parent_hwnd, false);
}

pub fn fit_font_to_window_streaming(parent_hwnd: HWND) {
    fit_font_to_window_ex(parent_hwnd, true);
}

fn fit_font_to_window_ex(parent_hwnd: HWND, streaming_mode: bool) {
    let hwnd_key = parent_hwnd.0 as isize;
    let phase = if streaming_mode {
        "fit_font_to_window_streaming"
    } else {
        "fit_font_to_window_final"
    };
    let script = FIT_FONT_SCRIPT.replace("__FIT_PHASE__", phase).replace(
        "__STREAMING_MODE__",
        if streaming_mode { "true" } else { "false" },
    );

    WEBVIEWS.with(|webviews| {
        if let Some(webview) = webviews.borrow().get(&hwnd_key)
            && let Err(err) = webview.evaluate_script(&script)
        {
            crate::log_info!(
                "[MarkdownDiag] fit_evaluate_script_failed hwnd={:?} phase={} err={:?}",
                parent_hwnd,
                phase,
                err
            );
        }
    });
}

/// Trigger Grid.js initialization on any tables in the WebView.
/// Call this after streaming ends to convert tables to interactive Grid.js tables.
pub fn init_gridjs(parent_hwnd: HWND) {
    let hwnd_key = parent_hwnd.0 as isize;

    WEBVIEWS.with(|webviews| {
        if let Some(webview) = webviews.borrow().get(&hwnd_key) {
            let script = r#"
                (function() {
                    if (typeof gridjs === 'undefined') return;

                    var tables = document.querySelectorAll('table:not(.gridjs-table):not([data-processed-table="true"])');
                    // Post-Grid.js shrink: when the last grid fires 'ready' the
                    // real table layout is finally committed to scrollHeight.
                    // The final fit that ran moments ago measured the raw
                    // <table> (before Grid.js styling inflated it), so its
                    // target font-size can overshoot the viewport by the
                    // time the styled grid is in flow. Count pending grids,
                    // run a ratio-shrink once they're all ready.
                    var pendingGrids = 0;
                    function checkAndShrink() {
                        try {
                            var doc = document.documentElement;
                            var winH = window.innerHeight;
                            var overflowPx = doc.scrollHeight - winH;
                            if (overflowPx <= winH * 0.05) return;
                            var cFs = parseFloat(document.body.style.fontSize) || 14;
                            var minFs = 14;
                            if (cFs <= minFs) return;
                            var scale = (winH / doc.scrollHeight) * 0.92;
                            var nFs = Math.max(minFs, Math.floor(cFs * scale));
                            if (nFs >= cFs) return;
                            document.body.style.fontSize = nFs + 'px';
                            window._sgtCurrentFontSize = nFs;
                        } catch (_e) {}
                    }
                    function afterGridReady() {
                        pendingGrids -= 1;
                        if (pendingGrids > 0) return;
                        // If the fit's rAF is still interpolating, poll
                        // until it settles — otherwise we'd read scrollH
                        // at a transient mid-animation state.
                        var poll = function() {
                            if (window._sgtFitAnim) {
                                requestAnimationFrame(poll);
                            } else {
                                checkAndShrink();
                            }
                        };
                        poll();
                    }

                    for (var i = 0; i < tables.length; i++) {
                        var table = tables[i];
                        if (table.closest('.gridjs-container') || table.closest('.gridjs-injected-wrapper')) continue;

                        table.setAttribute('data-processed-table', 'true');

                        var wrapper = document.createElement('div');
                        wrapper.className = 'gridjs-injected-wrapper';
                        table.parentNode.insertBefore(wrapper, table);

                        try {
                            var grid = new gridjs.Grid({
                                from: table,
                                sort: true,
                                fixedHeader: true,
                                search: false,
                                resizable: false,
                                autoWidth: false,
                                style: {
                                    table: { 'width': '100%' },
                                    td: { 'border': '1px solid #333' },
                                    th: { 'border': '1px solid #333' }
                                },
                                className: {
                                    table: 'gridjs-table-premium',
                                    th: 'gridjs-th-premium',
                                    td: 'gridjs-td-premium'
                                }
                            });
                            pendingGrids += 1;
                            (function(capturedTable) {
                                grid.on('ready', function() {
                                    capturedTable.classList.add('gridjs-hidden-source');
                                    // Wait one extra frame so the grid's
                                    // final layout is actually in flow
                                    // before we measure scrollHeight.
                                    requestAnimationFrame(afterGridReady);
                                });
                            })(table);
                            grid.render(wrapper);
                        } catch (e) {
                            console.error('Grid.js streaming init error:', e);
                            if(wrapper.parentNode) wrapper.parentNode.removeChild(wrapper);
                        }
                    }
                })();
            "#;
            if let Err(err) = webview.evaluate_script(script) {
                crate::log_info!(
                    "[MarkdownDiag] gridjs_init_failed hwnd={:?} err={:?}",
                    parent_hwnd,
                    err
                );
            }
        }
    });
}
