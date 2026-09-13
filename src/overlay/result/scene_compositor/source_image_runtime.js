// Rasterize a self-contained snapshot of the real fitted text, never desktop pixels.
window.__SGT_SOURCE_IMAGE__ = (function() {
  const pending = new Map();
  let font = null;
  const fontFace = __SGT_EXPORT_FONT_CSS_JSON__;
  function dataUrl(blob) {
    return new Promise((resolve,reject) => {
      const reader = new FileReader(); reader.onload = () => resolve(String(reader.result));
      reader.onerror = () => reject(new Error('image encoding failed')); reader.readAsDataURL(blob);
    });
  }
  function loadImage(url) {
    return new Promise((resolve,reject) => {
      const image = new Image(); image.onload = () => resolve(image);
      image.onerror = () => reject(new Error('image decoding failed')); image.src = url;
    });
  }
  function embeddedFont() {
    if (!font) font = fetch('/font.woff2').then(response => {
      if (!response.ok) throw new Error('export font unavailable');
      return response.blob();
    }).then(dataUrl).then(url => fontFace.replace('__SGT_EXPORT_FONT_URL__',url))
      .catch(error => { font = null; throw error; });
    return font;
  }
  function cloneStyled(source) {
    if (source.nodeType === Node.TEXT_NODE) return document.createTextNode(source.textContent);
    if (source.nodeType !== Node.ELEMENT_NODE) return null;
    const clone = document.createElement(source.localName);
    const style = getComputedStyle(source);
    for (const name of style) clone.style.setProperty(name,style.getPropertyValue(name));
    // Keep layout transforms (including footprint fitting), not reveal effects.
    clone.style.animation = 'none'; clone.style.transition = 'none';
    clone.style.opacity = '1'; clone.style.filter = 'none';
    for (const child of source.childNodes) {
      const copied = cloneStyled(child); if (copied) clone.appendChild(copied);
    }
    return clone;
  }
  async function rasterize(request) {
    const css = await embeddedFont();
    const entries = request.cells.map(cell => ({ cell, entry: cards.get(String(cell.id)) }));
    if (!cards.has(String(request.id)) || entries.some(({entry}) => !entry?.visible || !entry.sourceReplacement))
      throw new Error('translated image is no longer available');
    if (entries.some(({entry}) => entry.pendingContent || entry.contentFrame)) await new Promise(requestAnimationFrame);
    const revisions = entries.map(({entry}) => entry.contentRevision);
    await Promise.all(entries.map(({entry}) => entry.directState.sourceLayoutReady));
    if (!cards.has(String(request.id)) || entries.some(({cell,entry},index) =>
      cards.get(String(cell.id)) !== entry || !entry.visible || entry.pendingContent
      || entry.contentRevision !== revisions[index]))
      throw new Error('image layout changed while preparing the image');
    const root = document.createElement('div');
    root.setAttribute('xmlns','http://www.w3.org/1999/xhtml');
    root.style.cssText = `position:relative;width:${request.width}px;height:${request.height}px;overflow:hidden;margin:0;padding:0`;
    const style = document.createElement('style'); style.textContent = css; root.appendChild(style);
    const patches = [];
    for (const {cell,entry} of entries) {
      if (JSON.stringify(cell.segments) !== JSON.stringify(entry.sourceSegments))
        throw new Error('translation changed while preparing the image');
      if (!entry.bodyElement.textContent.trim() && cell.segments.some(text=>text.trim()))
        throw new Error('translated text is not ready');
      const {rect} = cell, w = parseFloat(entry.card.style.width), h = parseFloat(entry.card.style.height);
      if (!(w>0 && h>0 && rect.width>0 && rect.height>0)) throw new Error('invalid image cell geometry');
      const backdrop = entry.backdrop.dataset.url;
      if (!backdrop?.startsWith('data:image/png;base64,')) throw new Error('source mask is unavailable');
      const box = document.createElement('div');
      box.style.cssText = `position:absolute;left:${rect.x}px;top:${rect.y}px;width:${rect.width}px;height:${rect.height}px;overflow:hidden;`
        + `mask-image:url("${backdrop}");mask-size:100% 100%;mask-repeat:no-repeat`;
      const content = cloneStyled(entry.bodyElement);
      content.style.width = w+'px'; content.style.height = h+'px';
      content.style.position = 'absolute'; content.style.left = '0'; content.style.top = '0';
      content.style.transformOrigin = '0 0'; content.style.transform = `scale(${rect.width/w},${rect.height/h})`;
      box.appendChild(content); root.appendChild(box);
      patches.push({rect, image:loadImage(backdrop)});
    }
    const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="${request.width}" height="${request.height}" viewBox="0 0 ${request.width} ${request.height}">`
      + `<foreignObject width="100%" height="100%">${new XMLSerializer().serializeToString(root)}</foreignObject></svg>`;
    const textUrl = await dataUrl(new Blob([svg],{type:'image/svg+xml;charset=utf-8'}));
    const [source,text,...backgrounds] = await Promise.all([loadImage(request.source),loadImage(textUrl),...patches.map(p=>p.image)]);
    if (source.naturalWidth !== request.width || source.naturalHeight !== request.height) throw new Error('source dimensions changed');
    const canvas = document.createElement('canvas'); canvas.width=request.width; canvas.height=request.height;
    const context = canvas.getContext('2d'); if (!context) throw new Error('image canvas unavailable');
    context.drawImage(source,0,0); context.globalAlpha = Math.max(0,Math.min(100,request.opacity))/100;
    // All erasure patches go below all text, matching the shared overlay planes.
    patches.forEach(({rect},index)=>context.drawImage(backgrounds[index],rect.x,rect.y,rect.width,rect.height));
    context.drawImage(text,0,0);
    const blob = await new Promise(resolve=>canvas.toBlob(resolve,'image/png'));
    if (!blob) throw new Error('image canvas encoding failed');
    return (await dataUrl(blob)).split(',')[1];
  }
  function decorate(group,id) {
    const button = group?.querySelector('.copy-image-btn'); if (!button) return;
    const state = pending.get(String(id));
    button.classList.toggle('disabled',state?.busy === true);
    button.classList.toggle('success',state?.success === true);
    const icon = state?.busy ? 'hourglass_empty' : state?.success ? 'check' : 'filter';
    if (button.dataset.copyIcon !== icon) {
      button.dataset.copyIcon = icon; button.innerHTML = window.iconSvgs[icon];
    }
    button.title = state?.failed ? window.L10N.copy_image_failed : window.L10N.copy_image;
  }
  function repaint(id) { decorate(document.querySelector('.button-group[data-hwnd="'+id+'"]'),id); }
  function forget(id) {
    const state = pending.get(String(id)); if (state) clearTimeout(state.timer);
    pending.delete(String(id));
  }
  return {
    rasterize, decorate, forget,
    request(id) {
      id=String(id); if (pending.get(id)?.busy) return; forget(id);
      pending.set(id,{busy:true,timer:setTimeout(()=>{forget(id);repaint(id);},31000)}); repaint(id);
      window.ipc.postMessage(JSON.stringify({action:'copy_image',hwnd:id}));
    },
    async render(request) {
      let png=null,error=null;
      try { png=await rasterize(request); } catch(failure) { error=String(failure.message || failure); }
      window.ipc.postMessage(JSON.stringify({type:'source_image_ready',result:{id:request.id,token:request.token,png,error}}));
    },
    status(id,success) {
      id=String(id); forget(id);
      pending.set(id,{success,failed:!success,timer:setTimeout(()=>{forget(id);repaint(id);},1800)}); repaint(id);
    }
  };
})();
