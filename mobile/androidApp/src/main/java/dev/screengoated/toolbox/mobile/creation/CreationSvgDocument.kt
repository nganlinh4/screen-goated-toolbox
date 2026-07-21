@file:OptIn(androidx.compose.foundation.layout.ExperimentalLayoutApi::class)

package dev.screengoated.toolbox.mobile.creation

import android.annotation.SuppressLint
import android.graphics.Color as AndroidColor
import android.webkit.JavascriptInterface
import android.webkit.WebView
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.ui.i18n.CreationCommonLocale
import dev.screengoated.toolbox.mobile.ui.i18n.CreationSvgLocale
import java.util.Base64
import kotlin.coroutines.resume
import kotlinx.coroutines.suspendCancellableCoroutine
import org.json.JSONTokener
import org.json.JSONObject

internal class CreationSvgDocumentController {
    private var webView: WebView? = null

    internal fun attach(view: WebView) {
        webView = view
    }

    internal fun destroy() {
        webView?.let { view ->
            webView = null
            view.removeJavascriptInterface("SgtSvg")
            view.stopLoading()
            view.destroy()
        }
    }

    fun fit() = evaluate("window.SGT?.fit()")
    fun zoomIn() = evaluate("window.SGT?.zoom(1.2)")
    fun zoomOut() = evaluate("window.SGT?.zoom(0.833333)")
    fun undo() = evaluate("window.SGT?.undo()")
    fun redo() = evaluate("window.SGT?.redo()")
    fun deleteSelected() = evaluate("window.SGT?.deleteSelected()")
    fun setFill(value: String) = evaluate("window.SGT?.setPaint('fill', ${JSONObject.quote(value)})")
    fun setStroke(value: String) = evaluate("window.SGT?.setPaint('stroke', ${JSONObject.quote(value)})")

    suspend fun serialize(): String = suspendCancellableCoroutine { continuation ->
        val view = webView
        if (view == null) {
            continuation.resume("")
            return@suspendCancellableCoroutine
        }
        view.evaluateJavascript("window.SGT?.serialize() || ''") { encoded ->
            val value = runCatching { JSONTokener(encoded).nextValue() as? String }
                .getOrNull()
                .orEmpty()
            if (continuation.isActive) continuation.resume(value)
        }
    }

    private fun evaluate(script: String) {
        webView?.evaluateJavascript(script, null)
    }
}

@SuppressLint("SetJavaScriptEnabled")
@Composable
internal fun CreationSvgDocument(
    outputPath: String,
    viewModel: CreationNativeViewModel,
    controller: CreationSvgDocumentController,
    modifier: Modifier = Modifier,
) {
    val context = LocalContext.current
    val svg by produceState<String?>(null, outputPath) {
        value = runCatching { viewModel.readSvg(outputPath) }.getOrNull()
    }
    val document = svg
    if (document == null) {
        Box(modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
            androidx.compose.material3.CircularProgressIndicator()
        }
        return
    }
    val html = remember(document) { svgDocumentHtml(document) }
    val bridge = remember { SvgSelectionBridge() }
    key(outputPath) {
        AndroidView(
            modifier = modifier
                .fillMaxSize()
                .background(MaterialTheme.colorScheme.surfaceContainerLowest),
            factory = {
                WebView(context).apply {
                    setBackgroundColor(AndroidColor.TRANSPARENT)
                    settings.javaScriptEnabled = true
                    settings.domStorageEnabled = false
                    settings.allowFileAccess = false
                    settings.allowContentAccess = false
                    settings.blockNetworkLoads = true
                    settings.setSupportZoom(false)
                    addJavascriptInterface(bridge, "SgtSvg")
                    controller.attach(this)
                    loadDataWithBaseURL(
                        "https://sgt.local/svg-document/",
                        html,
                        "text/html",
                        "utf-8",
                        null,
                    )
                }
            },
        )
        DisposableEffect(controller) {
            onDispose { controller.destroy() }
        }
    }
}

@Composable
internal fun CreationSvgEditorControls(
    controller: CreationSvgDocumentController,
    common: CreationCommonLocale,
    strings: CreationSvgLocale,
    accent: Color,
    onSave: () -> Unit,
) {
    val swatches = listOf(
        "none" to Color.Transparent,
        "#111111" to Color(0xff111111),
        "#ffffff" to Color.White,
        "#1976d2" to Color(0xff1976d2),
        "#00a38c" to Color(0xff00a38c),
        "#e14d72" to Color(0xffe14d72),
        "#f4b400" to Color(0xfff4b400),
    )
    androidx.compose.foundation.layout.Column(
        modifier = Modifier.fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        FlowRow(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(4.dp),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            ViewerIconButton(R.drawable.ms_open_in_full, strings.fit, controller::fit)
            ViewerIconButton(R.drawable.ms_remove, strings.zoomOut, controller::zoomOut)
            ViewerIconButton(R.drawable.ms_add, strings.zoomIn, controller::zoomIn)
            ViewerIconButton(R.drawable.ms_arrow_back, strings.undo, controller::undo)
            ViewerIconButton(R.drawable.ms_arrow_forward, strings.redo, controller::redo)
            ViewerIconButton(R.drawable.ms_delete, common.delete, controller::deleteSelected)
            FilledTonalButton(onClick = onSave) {
                Icon(
                    painterResource(R.drawable.ms_check),
                    contentDescription = null,
                    modifier = Modifier.size(18.dp),
                )
                androidx.compose.foundation.layout.Spacer(Modifier.size(6.dp))
                Text(strings.saveEdits)
            }
        }
        PaintSwatches(strings.fill, swatches, accent) { controller.setFill(it) }
        PaintSwatches(strings.stroke, swatches, accent) { controller.setStroke(it) }
    }
}

@Composable
private fun ViewerIconButton(icon: Int, label: String, action: () -> Unit) {
    IconButton(onClick = action, modifier = Modifier.size(40.dp)) {
        Icon(painterResource(icon), contentDescription = label)
    }
}

@Composable
private fun PaintSwatches(
    label: String,
    swatches: List<Pair<String, Color>>,
    accent: Color,
    onSelect: (String) -> Unit,
) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Text(label, style = MaterialTheme.typography.labelMedium, modifier = Modifier.size(width = 48.dp, height = 24.dp))
        FlowRow(
            modifier = Modifier.weight(1f),
            horizontalArrangement = Arrangement.spacedBy(7.dp),
        ) {
            swatches.forEach { (value, color) ->
                Box(
                    modifier = Modifier
                        .size(25.dp)
                        .background(
                            if (color == Color.Transparent) MaterialTheme.colorScheme.surface else color,
                            CircleShape,
                        )
                        .border(
                            1.dp,
                            if (color == Color.Transparent) accent else MaterialTheme.colorScheme.outlineVariant,
                            CircleShape,
                        )
                        .clickable { onSelect(value) },
                )
            }
        }
    }
}

private class SvgSelectionBridge {
    @JavascriptInterface
    fun selected(index: String) = Unit
}

private fun svgDocumentHtml(svg: String): String {
    val encoded = Base64.getEncoder().encodeToString(svg.toByteArray(Charsets.UTF_8))
    return """
        <!doctype html>
        <html><head>
          <meta name="viewport" content="width=device-width,initial-scale=1,maximum-scale=1,user-scalable=no">
          <meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; script-src 'unsafe-inline'; img-src data: blob:">
          <style>
            *{box-sizing:border-box}html,body{width:100%;height:100%;margin:0;overflow:hidden;background:transparent}
            #stage{width:100%;height:100%;display:flex;align-items:center;justify-content:center;touch-action:none;overflow:hidden}
            #viewport{transform-origin:center center;will-change:transform;width:94%;height:94%}
            #viewport svg{display:block;width:100%;height:100%;max-width:100%;max-height:100%;overflow:visible}
            #viewport [data-sgt-selected="true"]{filter:drop-shadow(0 0 2px #00a38c) drop-shadow(0 0 1px #00a38c)}
          </style>
        </head><body><div id="stage"><div id="viewport"></div></div>
        <script>
        (()=>{
          const raw=new TextDecoder().decode(Uint8Array.from(atob('$encoded'),c=>c.charCodeAt(0)));
          const doc=new DOMParser().parseFromString(raw,'image/svg+xml');
          doc.querySelectorAll('script,foreignObject,iframe,object,embed').forEach(n=>n.remove());
          doc.querySelectorAll('*').forEach(n=>[...n.attributes].forEach(a=>{
            const k=a.name.toLowerCase(),v=a.value.toLowerCase();
            if(k.startsWith('on')||v.includes('javascript:'))n.removeAttribute(a.name);
          }));
          const root=doc.documentElement,viewport=document.querySelector('#viewport');
          root.setAttribute('preserveAspectRatio',root.getAttribute('preserveAspectRatio')||'xMidYMid meet');
          viewport.append(document.importNode(root,true));
          let scale=1,tx=0,ty=0,selected=null,undo=[],redo=[],drag=null;
          const shapes='path,rect,circle,ellipse,polygon,polyline,line';
          const transform=()=>viewport.style.transform='translate('+tx+'px,'+ty+'px) scale('+scale+')';
          const snapshot=()=>viewport.querySelector('svg').outerHTML;
          const restore=s=>{viewport.innerHTML=s;selected=null;bind();};
          const push=()=>{undo.push(snapshot());if(undo.length>40)undo.shift();redo=[];};
          const bind=()=>viewport.querySelectorAll(shapes).forEach((n,i)=>{
            n.dataset.sgtIndex=String(i);n.addEventListener('click',e=>{
              e.stopPropagation();selected?.removeAttribute('data-sgt-selected');selected=n;
              selected.dataset.sgtSelected='true';SgtSvg.selected(String(i));
            });
          });
          bind();
          const animated=[...viewport.querySelectorAll(shapes)],count=Math.max(1,animated.length);
          const step=Math.max(.6,Math.min(45,1200/count));
          animated.forEach((n,i)=>n.animate(
            [{opacity:0,transform:'translateY(4px)'},{opacity:1,transform:'translateY(0)'}],
            {duration:Math.max(320,Math.min(720,1600/count+360)),delay:i*step,easing:'cubic-bezier(.2,.8,.2,1)',fill:'both'}
          ));
          const stage=document.querySelector('#stage');
          stage.addEventListener('click',()=>{
            selected?.removeAttribute('data-sgt-selected');selected=null;
          });
          stage.addEventListener('pointerdown',e=>{drag={x:e.clientX,y:e.clientY,tx,ty};stage.setPointerCapture(e.pointerId)});
          stage.addEventListener('pointermove',e=>{if(!drag||scale<=1)return;tx=drag.tx+e.clientX-drag.x;ty=drag.ty+e.clientY-drag.y;transform()});
          stage.addEventListener('pointerup',()=>drag=null);stage.addEventListener('pointercancel',()=>drag=null);
          window.SGT={
            fit(){scale=1;tx=0;ty=0;transform()},
            zoom(f){scale=Math.max(.25,Math.min(8,scale*f));if(scale<=1){tx=0;ty=0}transform()},
            setPaint(k,v){if(!selected)return;push();selected.setAttribute(k,v)},
            deleteSelected(){if(!selected)return;push();selected.remove();selected=null},
            undo(){if(!undo.length)return;redo.push(snapshot());restore(undo.pop())},
            redo(){if(!redo.length)return;undo.push(snapshot());restore(redo.pop())},
            serialize(){const clone=viewport.querySelector('svg').cloneNode(true);clone.querySelectorAll('[data-sgt-index],[data-sgt-selected]').forEach(n=>{n.removeAttribute('data-sgt-index');n.removeAttribute('data-sgt-selected')});return new XMLSerializer().serializeToString(clone)}
          };
        })();
        </script></body></html>
    """.trimIndent()
}
