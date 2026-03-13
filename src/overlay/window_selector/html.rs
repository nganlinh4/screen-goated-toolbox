use super::{SelectorEntry, SelectorText};

pub fn generate_html(
    entries: &[SelectorEntry],
    font_css: &str,
    is_dark: bool,
    text: &SelectorText,
) -> String {
    let entries_json = serde_json::to_string(entries)
        .unwrap_or_else(|_| "[]".to_string())
        .replace("</", "<\\/");

    format!(
        r##"<!DOCTYPE html>
<html lang="en" data-theme="{theme}">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width,initial-scale=1.0">
<title>{title}</title>
<style>
{font_css}
*{{box-sizing:border-box;margin:0;padding:0}}
html,body{{width:100%;height:100%;overflow:hidden;font-family:'Google Sans Flex','Segoe UI',system-ui,sans-serif;font-variation-settings:'ROND' 100;background:transparent}}

html[data-theme='dark']{{
  --overlay-bg: rgba(10,10,12,0.88);
  --card-bg: rgba(255,255,255,0.07);
  --card-border: rgba(255,255,255,0.10);
  --card-hover-border: rgba(59,130,246,0.70);
  --thumb-bg: rgba(255,255,255,0.04);
  --title-color: #fff;
  --subtitle-color: rgba(255,255,255,0.40);
  --proc-color: rgba(255,255,255,0.32);
  --close-color: rgba(255,255,255,0.45);
  --close-hover-color: #fff;
  --wave-color: #60a5fa;
  --header-color: rgba(255,255,255,0.52);
  --scroll-thumb: rgba(255,255,255,0.18);
}}

html[data-theme='light']{{
  --overlay-bg: rgba(240,242,247,0.92);
  --card-bg: rgba(0,0,0,0.04);
  --card-border: rgba(0,0,0,0.09);
  --card-hover-border: rgba(37,99,235,0.65);
  --thumb-bg: rgba(0,0,0,0.05);
  --title-color: #0f172a;
  --subtitle-color: rgba(0,0,0,0.42);
  --proc-color: rgba(0,0,0,0.38);
  --close-color: rgba(0,0,0,0.38);
  --close-hover-color: #0f172a;
  --wave-color: #1d4ed8;
  --header-color: rgba(0,0,0,0.42);
  --scroll-thumb: rgba(0,0,0,0.22);
}}

::-webkit-scrollbar{{width:6px}}
::-webkit-scrollbar-track{{background:transparent}}
::-webkit-scrollbar-thumb{{background:var(--scroll-thumb);border-radius:3px}}

@keyframes overlayIn{{
  from{{opacity:0;transform:scale(0.98)}}
  to{{opacity:1;transform:scale(1)}}
}}
@keyframes cardIn{{
  from{{opacity:0;transform:translateY(10px)}}
  to{{opacity:1;transform:translateY(0)}}
}}
@keyframes waveColor{{
  0%,100%{{color:var(--header-color);font-variation-settings:'GRAD' 0,'wght' 600,'ROND' 100}}
  50%{{color:var(--wave-color);font-variation-settings:'GRAD' 200,'wght' 900,'ROND' 100}}
}}
@keyframes thumbFadeIn{{
  from{{opacity:0}}
  to{{opacity:1}}
}}

.overlay{{
  position:fixed;inset:0;
  background:var(--overlay-bg);
  backdrop-filter:blur(14px);-webkit-backdrop-filter:blur(14px);
  display:flex;flex-direction:column;align-items:center;
  padding:34px 24px 22px;overflow-y:auto;
  animation:overlayIn 0.22s cubic-bezier(0.2,0,0,1) forwards;
}}

.close-btn{{
  position:fixed;top:14px;right:18px;
  width:32px;height:32px;
  display:flex;align-items:center;justify-content:center;
  cursor:pointer;color:var(--close-color);font-size:20px;line-height:1;
  transition:color 0.12s;z-index:10;user-select:none;
  background:none;border:none;padding:0;
}}
.close-btn:hover{{color:var(--close-hover-color)}}

.header{{text-align:center;margin-bottom:18px;flex-shrink:0;width:min(92vw,1760px)}}
.title{{
  color:var(--title-color);
  font-size:clamp(24px,2.1vw,34px);font-weight:600;
  font-stretch:130%;text-transform:uppercase;
  letter-spacing:0.12em;
  margin-bottom:6px;
  font-family:'Google Sans Flex','Segoe UI',system-ui,sans-serif;
  font-variation-settings:'wght' 600,'ROND' 100;
  line-height:1.15
}}
.subtitle{{
  color:var(--subtitle-color);font-size:12px;
  font-variation-settings:'wght' 400,'ROND' 80;
  line-height:1.7
}}
.entry-count{{
  display:block;color:var(--proc-color);font-size:11px;
  font-variation-settings:'wght' 400,'ROND' 80
}}

.grid{{
  display:grid;
  grid-template-columns:repeat(auto-fill,minmax(clamp(220px,18vw,280px),1fr));
  gap:14px;width:min(92vw,1760px);max-width:1760px;
  align-items:start
}}

.card{{
  --card-accent-rgb: 128,128,128;
  background:
    radial-gradient(circle at top left, rgba(var(--card-accent-rgb),0.18), transparent 48%),
    linear-gradient(180deg, rgba(var(--card-accent-rgb),0.12), transparent 68%),
    var(--card-bg);
  border:1px solid var(--card-border);
  border-radius:10px;overflow:hidden;cursor:pointer;
  transition:background 0.12s,border-color 0.12s,transform 0.12s,box-shadow 0.12s;
  user-select:none;
  position:relative;
  opacity:0;
}}
.card::before{{
  content:'';
  position:absolute;left:0;right:0;top:0;height:3px;
  background:rgba(var(--card-accent-rgb),0.9);
  opacity:0.92;pointer-events:none
}}
.card:hover{{
  border-color:var(--card-hover-border);
  transform:translateY(-3px);
  box-shadow:
    0 10px 28px rgba(0,0,0,0.22),
    0 0 0 1px rgba(var(--card-accent-rgb),0.20) inset,
    0 0 24px rgba(var(--card-accent-rgb),0.14)
}}
.card:active{{transform:translateY(-1px)}}
.card.is-disabled{{opacity:0.35!important;cursor:not-allowed}}
.card.is-disabled:hover{{transform:none;box-shadow:none;border-color:var(--card-border)}}

.thumb{{
  width:100%;
  background:
    linear-gradient(180deg, rgba(var(--card-accent-rgb),0.16), transparent 78%),
    var(--thumb-bg);
  display:flex;align-items:center;justify-content:center;
  overflow:hidden;position:relative;
  max-height:196px
}}
.thumb img{{
  width:100%;height:100%;object-fit:cover;display:block;
  object-position:center top;
  animation:thumbFadeIn 0.3s ease forwards
}}
.thumb-ph{{
  position:absolute;inset:0;
  display:flex;align-items:center;justify-content:center;
  color:rgba(128,128,128,0.20);font-size:24px
}}

.info{{
  padding:8px 10px;display:flex;align-items:center;gap:9px;
  background:linear-gradient(90deg, rgba(var(--card-accent-rgb),0.16), transparent 78%)
}}
.icon{{
  width:22px;height:22px;flex-shrink:0;border-radius:6px;object-fit:contain;
  background:rgba(var(--card-accent-rgb),0.18);
  box-shadow:0 0 0 1px rgba(var(--card-accent-rgb),0.24) inset;
  padding:2px
}}
.icon-ph{{width:22px;height:22px;background:rgba(128,128,128,0.12);border-radius:4px;flex-shrink:0}}
.text{{flex:1;min-width:0}}
.entry-title{{
  color:var(--title-color);font-size:11.5px;font-weight:500;
  white-space:nowrap;overflow:hidden;text-overflow:ellipsis;
  font-variation-settings:'wght' 500,'ROND' 100
}}
.entry-subtitle{{
  color:var(--proc-color);font-size:10px;
  white-space:nowrap;overflow:hidden;text-overflow:ellipsis;margin-top:1px
}}
.status-badge{{
  background:rgba(239,68,68,0.16);color:rgb(239,68,68);
  border:1px solid rgba(239,68,68,0.28);
  border-radius:3px;font-size:8px;font-weight:700;
  padding:1px 4px;letter-spacing:0.07em;text-transform:uppercase;flex-shrink:0
}}
@media (max-width: 1100px){{
  .overlay{{padding:40px 18px 18px}}
  .header{{width:min(96vw,1200px)}}
  .grid{{
    width:min(96vw,1200px);
    grid-template-columns:repeat(auto-fill,minmax(200px,1fr));
    gap:12px
  }}
  .thumb{{max-height:172px}}
}}
@media (max-width: 760px){{
  .overlay{{padding:42px 14px 14px}}
  .header{{width:100%;margin-bottom:16px}}
  .title{{font-size:22px;letter-spacing:0.08em}}
  .subtitle{{font-size:11px}}
  .grid{{
    width:100%;
    grid-template-columns:repeat(auto-fill,minmax(170px,1fr));
    gap:10px
  }}
  .thumb{{max-height:148px}}
}}
</style>
</head>
<body>
<button class="close-btn" id="close-btn" title="{cancel_label}" aria-label="{cancel_label}">&#x2715;</button>
<div class="overlay" id="overlay">
  <div class="header">
    <div class="title" id="title-text">{title}</div>
    <div class="subtitle">{subtitle}<span class="entry-count">{count_label}</span></div>
  </div>
  <div class="grid" id="grid"></div>
</div>
<script>
var entries={entries_json};

(function(){{
  var el=document.getElementById('title-text');
  if(!el) return;
  el.innerHTML=el.innerText.split('').map(function(ch,i){{
    return '<span style="display:inline-block;animation:waveColor 0.65s ease forwards '+(0.08+i*0.035).toFixed(3)+'s">'+
      (ch===' '?'&nbsp;':ch.replace(/&/g,'&amp;').replace(/</g,'&lt;'))+'</span>';
  }}).join('');
}})();

function selectEntry(id){{window.ipc.postMessage('select:'+id);}}
function cancel(){{window.ipc.postMessage('cancel');}}
function clamp(value,min,max){{return Math.max(min,Math.min(max,value));}}
function rgbToHsl(r,g,b){{
  r/=255;g/=255;b/=255;
  var max=Math.max(r,g,b),min=Math.min(r,g,b);
  var h,s,l=(max+min)/2;
  if(max===min){{h=0;s=0;}}
  else {{
    var d=max-min;
    s=l>0.5?d/(2-max-min):d/(max+min);
    switch(max){{
      case r:h=(g-b)/d+(g<b?6:0);break;
      case g:h=(b-r)/d+2;break;
      default:h=(r-g)/d+4;break;
    }}
    h/=6;
  }}
  return [h,s,l];
}}
function hslToRgb(h,s,l){{
  var r,g,b;
  if(s===0){{r=g=b=l;}}
  else {{
    function hue2rgb(p,q,t){{
      if(t<0)t+=1;
      if(t>1)t-=1;
      if(t<1/6)return p+(q-p)*6*t;
      if(t<1/2)return q;
      if(t<2/3)return p+(q-p)*(2/3-t)*6;
      return p;
    }}
    var q=l<0.5?l*(1+s):l+s-l*s;
    var p=2*l-q;
    r=hue2rgb(p,q,h+1/3);
    g=hue2rgb(p,q,h);
    b=hue2rgb(p,q,h-1/3);
  }}
  return [Math.round(r*255),Math.round(g*255),Math.round(b*255)];
}}
function normalizeAccentColor(rgb){{
  var hsl=rgbToHsl(rgb[0],rgb[1],rgb[2]);
  hsl[1]=Math.max(hsl[1],0.28);
  hsl[2]=clamp(hsl[2],0.34,0.62);
  return hslToRgb(hsl[0],hsl[1],hsl[2]);
}}
function computeAverageIconColor(img){{
  try {{
    var canvas=document.createElement('canvas');
    canvas.width=16;canvas.height=16;
    var ctx=canvas.getContext('2d',{{willReadFrequently:true}});
    if(!ctx) return null;
    ctx.clearRect(0,0,16,16);
    ctx.drawImage(img,0,0,16,16);
    var data=ctx.getImageData(0,0,16,16).data;
    var r=0,g=0,b=0,total=0;
    for(var i=0;i<data.length;i+=4){{
      var alpha=data[i+3]/255;
      if(alpha<0.05) continue;
      var rr=data[i],gg=data[i+1],bb=data[i+2];
      var max=Math.max(rr,gg,bb),min=Math.min(rr,gg,bb);
      var sat=max===0?0:(max-min)/max;
      var lum=(0.2126*rr+0.7152*gg+0.0722*bb)/255;
      var weight=alpha*(0.35+sat*0.9);
      if(lum<0.08||lum>0.96) weight*=0.55;
      r+=rr*weight;g+=gg*weight;b+=bb*weight;total+=weight;
    }}
    if(total<0.001) return null;
    return normalizeAccentColor([
      Math.round(r/total),
      Math.round(g/total),
      Math.round(b/total)
    ]);
  }} catch(_err) {{
    return null;
  }}
}}
function applyCardAccent(card,rgb){{
  if(!rgb) return;
  card.style.setProperty('--card-accent-rgb',rgb.join(','));
}}
function attachCardAccentFromIcon(card,img){{
  var apply=function(){{applyCardAccent(card,computeAverageIconColor(img));}};
  if(img.complete&&img.naturalWidth>0) apply();
  else img.addEventListener('load',apply,{{once:true}});
}}

var grid=document.getElementById('grid');
var overlay=document.getElementById('overlay');

document.getElementById('close-btn').addEventListener('click',cancel);

entries.forEach(function(entry,idx){{
  var raw=(entry.winW&&entry.winH)?(entry.winW/entry.winH):(16/9);
  var ar=Math.min(Math.max(raw,1.0),2.8);

  var card=document.createElement('div');
  card.className='card'+(entry.disabled?' is-disabled':'');
  card.style.animation='cardIn 0.22s cubic-bezier(0.2,0,0,1) forwards '+(0.04+idx*0.022).toFixed(3)+'s';

  var thumb=document.createElement('div');
  thumb.className='thumb';
  thumb.style.aspectRatio=ar.toFixed(4);
  thumb.id='thumb-wrap-'+entry.id;

  var ph=document.createElement('div');ph.className='thumb-ph';ph.textContent='\u25a3';
  thumb.appendChild(ph);

  if(entry.previewDataUrl){{
    var img=document.createElement('img');img.src=entry.previewDataUrl;img.alt='';
    thumb.appendChild(img);ph.style.display='none';
  }}

  var info=document.createElement('div');info.className='info';
  if(entry.iconDataUrl){{
    var ic=document.createElement('img');ic.className='icon';ic.src=entry.iconDataUrl;ic.alt='';info.appendChild(ic);
    attachCardAccentFromIcon(card,ic);
  }}else{{
    var iph=document.createElement('div');iph.className='icon-ph';info.appendChild(iph);
  }}
  var text=document.createElement('div');text.className='text';
  var t=document.createElement('div');t.className='entry-title';t.textContent=entry.title;t.title=entry.title;
  var p=document.createElement('div');p.className='entry-subtitle';p.textContent=entry.subtitle;
  text.appendChild(t);text.appendChild(p);info.appendChild(text);
  if(entry.badgeText){{
    var badge=document.createElement('span');badge.className='status-badge';badge.textContent=entry.badgeText;info.appendChild(badge);
  }}
  card.appendChild(thumb);card.appendChild(info);
  if(!entry.disabled){{card.addEventListener('click',function(){{selectEntry(entry.id);}});}}
  grid.appendChild(card);
}});

window.updateThumb=function(id,dataUrl){{
  var wrap=document.getElementById('thumb-wrap-'+id);
  if(!wrap) return;
  var ph=wrap.querySelector('.thumb-ph');
  if(ph) ph.style.display='none';
  var existing=wrap.querySelector('img');
  if(existing){{existing.src=dataUrl;return;}}
  var img=document.createElement('img');img.src=dataUrl;img.alt='';
  wrap.appendChild(img);
}};

window.setTheme=function(theme){{
  document.documentElement.setAttribute('data-theme', theme==='light' ? 'light' : 'dark');
}};

document.addEventListener('keydown',function(e){{if(e.key==='Escape')cancel();}});
overlay.addEventListener('click',function(e){{if(e.target===overlay)cancel();}});
</script>
</body>
</html>"##,
        theme = if is_dark { "dark" } else { "light" },
        font_css = font_css,
        title = text.title,
        subtitle = text.subtitle,
        count_label = text.count_label,
        cancel_label = text.cancel_label,
        entries_json = entries_json,
    )
}
