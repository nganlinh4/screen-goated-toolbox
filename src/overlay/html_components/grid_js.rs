pub fn get_css() -> &'static str {
    r#"
    /* --- Grid.js Theme-Adaptive Compact Styling --- */
    /* Uses CSS variables from markdown_view.rs theme system */
    
    .gridjs-container {
        color: var(--text-color, #e0e0e0);
        font-family: 'Google Sans Flex', 'Segoe UI', sans-serif !important;
        background: var(--table-bg, rgba(0,0,0,0.2)) !important;
        padding: 0 !important;
        border-radius: 8px;
        border: 1px solid var(--border-color, #333);
        box-shadow: none;
        margin: 0 !important;
        font-size: 13px !important;
        position: relative;
        overflow: auto !important;
        display: block !important;
    }
    
    .gridjs-wrapper, .gridjs-tbody, .gridjs-tr, .gridjs-td {
        background-color: var(--table-bg, rgba(0,0,0,0.2)) !important;
        border-color: var(--border-color, #333) !important;
    }
    
    .gridjs-table {
        /* Full width, let browser handle column sizing */
        width: 100% !important; 
        max-width: 100% !important;
        border-collapse: collapse !important;
        table-layout: auto !important; 
    }
    
    .gridjs-head {
        background: var(--table-header-bg, #252525) !important;
    }
    
    /* Header Styling */
    .gridjs-th {
        background: var(--table-header-bg, #252525) !important;
        color: var(--primary, #81d4fa) !important;
        border: none !important;
        border-bottom: 1px solid var(--border-color, #444) !important;
        /* Compact padding as requested */
        padding: 4px 8px !important;
        font-weight: 600 !important;
        position: relative !important;
        text-transform: none !important;
        outline: none !important;
        white-space: nowrap !important;
        width: auto !important;
    }
    
    .gridjs-th:hover {
        background: var(--glass, rgba(255,255,255,0.03)) !important;
    }
    
    /* Sort Icon - Inline */
    .gridjs-th-content {
        float: left !important;
        display: inline-block !important;
    }

    .gridjs-sort {
        float: none !important;
        display: inline-block !important;
        vertical-align: middle !important;
        opacity: 0.5 !important;
        /* Use filter to adapt to theme - dark mode inverts, light mode uses default */
        filter: var(--sort-icon-filter, invert(1) brightness(200%) grayscale(100%)) !important; 
        margin-left: 8px !important;
        margin-top: -2px !important;
        width: 10px !important;
        height: 10px !important;
    }
    .gridjs-th:hover .gridjs-sort { opacity: 1 !important; }
    
    /* Cells */
    .gridjs-td {
        border: none !important;
        border-bottom: 1px solid var(--border-color, #333) !important;
        color: var(--text-color, #e0e0e0) !important;
        /* Compact padding as requested */
        padding: 4px 8px !important;
        white-space: normal !important; 
        max-width: 400px;
        overflow-wrap: break-word;
    }
    
    .gridjs-tr:last-child .gridjs-td {
        border-bottom: none !important;
    }
    
    .gridjs-tr:hover .gridjs-td {
        background-color: var(--glass, rgba(255,255,255,0.03)) !important;
    }

    /* Footer */
    .gridjs-footer {
        background: var(--table-header-bg, #252525) !important;
        border-top: 1px solid var(--border-color, #333) !important;
        padding: 8px !important;
        width: 100% !important; 
        display: block !important;
    }
    
    .gridjs-pagination button {
        background: transparent !important;
        border: 1px solid var(--border-color, rgba(255,255,255,0.1)) !important;
        color: var(--h4-color, #aaa) !important;
        border-radius: 4px !important;
    }
    
    .gridjs-pagination button:hover:not([disabled]) {
        background: var(--glass, #333) !important;
        color: var(--text-color, #fff) !important;
    }
    
    .gridjs-pagination button.gridjs-currentPage {
        background: var(--glass, #333) !important;
        border-color: var(--primary, #81d4fa) !important;
        color: var(--primary, #81d4fa) !important;
        font-weight: bold;
    }
    
    .gridjs-tr-header { display: table-row !important; }
    
    .gridjs-wrapper::-webkit-scrollbar { width: 8px; height: 8px; }
    .gridjs-wrapper::-webkit-scrollbar-track { background: var(--table-bg, #1a1a1a); }
    .gridjs-wrapper::-webkit-scrollbar-thumb { background: var(--border-color, #444); border-radius: 4px; }
    .gridjs-wrapper::-webkit-scrollbar-thumb:hover { background: var(--h4-color, #555); }
    
    .gridjs-hidden-source {
        display: none !important;
    }
    "#
}

pub fn get_init_script() -> &'static str {
    r#"
    (function() {
        var processTimeout;

        var initGridJs = function() {
            if (typeof gridjs === 'undefined') {
                setTimeout(initGridJs, 50);
                return;
            }

            var tables = document.querySelectorAll('table:not(.gridjs-table):not([data-processed-table="true"])');
            
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

                    grid.on('ready', function() {
                        table.classList.add('gridjs-hidden-source');
                    });

                    grid.render(wrapper);
                    
                } catch (e) {
                    console.error('Grid.js init error:', e);
                    if(wrapper.parentNode) wrapper.parentNode.removeChild(wrapper);
                }
            }
        };

        if (document.readyState === 'loading') {
            document.addEventListener('DOMContentLoaded', initGridJs);
        } else {
            initGridJs();
        }
        
        var observer = new MutationObserver(function(mutations) {
            var shouldCheck = false;
            
            for (var i = 0; i < mutations.length; i++) {
                var m = mutations[i];
                var target = m.target;
                
                if (target && (
                    target.closest('.gridjs-injected-wrapper') || 
                    target.closest('.gridjs-container') ||
                    target.classList.contains('gridjs-table') ||
                    target.classList.contains('gridjs-head') ||
                    target.classList.contains('gridjs-wrapper')
                )) {
                    continue;
                }

                if (m.addedNodes.length > 0) {
                    for (var k = 0; k < m.addedNodes.length; k++) {
                        var n = m.addedNodes[k];
                        if (n.nodeType !== 1) continue; 
                        
                        if (n.classList.contains('gridjs-container') || n.classList.contains('gridjs-wrapper')) continue;

                        if (n.nodeName === 'TABLE') {
                            if (!n.hasAttribute('data-processed-table') && !n.classList.contains('gridjs-table')) {
                                shouldCheck = true;
                                break;
                            }
                        } else if (n.querySelector) {
                            if (n.querySelector('table:not(.gridjs-table):not([data-processed-table="true"])')) {
                                shouldCheck = true;
                                break;
                            }
                        }
                    }
                }
                if (shouldCheck) break;
            }
            
            if (shouldCheck) {
                if (window.gridJsTimeout) clearTimeout(window.gridJsTimeout);
                window.gridJsTimeout = setTimeout(initGridJs, 100);
            }
        });
        
        observer.observe(document.body, { childList: true, subtree: true });
    })();
    "#
}

pub fn get_lib_urls() -> (&'static str, &'static str) {
    (
        "https://unpkg.com/gridjs/dist/theme/mermaid.min.css",
        "https://unpkg.com/gridjs/dist/gridjs.umd.js",
    )
}
