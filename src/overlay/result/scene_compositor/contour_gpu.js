window.__SGT_PROCESSING_GPU__ = function(canvas) {
  const gl = canvas.getContext('webgl2', { alpha: true, premultipliedAlpha: true, antialias: false,
    depth: false, stencil: false, preserveDrawingBuffer: false, powerPreference: 'low-power' });
  if (!gl) throw new Error('WebGL2 unavailable');
  const vertex = `#version 300 es
    out vec2 uv;
    void main(){vec2 p=vec2((gl_VertexID<<1)&2,gl_VertexID&2);uv=vec2(p.x,1.0-p.y);gl_Position=vec4(p*2.0-1.0,0,1);}`;
  const fragment = `#version 300 es
    precision highp float;
    in vec2 uv;out vec4 color;
    uniform vec2 size;uniform vec4 viewport;uniform sampler2D field;
    uniform float fieldRange,targetBand,morph,contract,softness,phase,alpha,retreat;
    uniform int controlCount;uniform vec4 controlBounds,controlWork,controlRects[16];uniform float controlRadii[16],controlAlpha;
    float rectangle(vec2 p,vec4 bounds){vec2 halfSize=(bounds.zw-bounds.xy)*.5;
      float radius=min(28.,min(halfSize.x,halfSize.y));
      vec2 q=abs(p-(bounds.xy+bounds.zw)*.5)-(halfSize-vec2(radius));float interior=min(max(q.x,q.y),0.);
      float k=min(24.,-interior*.5),h=max(k-abs(q.x-q.y),0.);
      return length(max(q,0.))+interior+h*h/(4.*max(k,.0001))-radius;}
    vec3 palette(float t){t=fract(t)*4.;vec3 a,b;
      if(t<1.){a=vec3(85,220,255);b=vec3(136,112,255);}
      else if(t<2.){a=vec3(136,112,255);b=vec3(255,92,200);}
      else if(t<3.){a=vec3(255,92,200);b=vec3(255,179,64);}
      else{a=vec3(255,179,64);b=vec3(85,220,255);}
      return mix(a,b,fract(t))/255.;}
    float sampleField(vec2 at){vec2 packed=texture(field,at).rg*255.;
      return ((packed.x*256.+packed.y)/65535.*2.-1.)*fieldRange;}
    float mixedDistance(vec2 p){return mix(rectangle(p,vec4(0,0,size)),sampleField(p/size),morph);}
    float boundary(vec2 p){
      float radius=112.*morph*(1.-morph);
      if(radius<.1)return mixedDistance(p);
      vec2 x=vec2(radius,0),y=vec2(0,radius);
      return (4.*mixedDistance(p)+2.*(mixedDistance(p-x)+mixedDistance(p+x)+mixedDistance(p-y)+mixedDistance(p+y))
        +mixedDistance(p-x-y)+mixedDistance(p-x+y)+mixedDistance(p+x-y)+mixedDistance(p+x+y))/16.;}
    vec4 light(float d,float weight,float band,float blur,vec3 hue){
      float aa=max(fwidth(d),.7)+blur*1.6,core=1.-smoothstep(1.5-aa*.5,2.5+aa*.5,-d);
      float halo=pow(clamp(1.-max(0.,-d-2.)/max(1.,band-2.),0.,1.),3.);
      float coverage=(1.-smoothstep(-aa*.5,aa*.5,d))*max(core,halo)*weight*alpha;
      return vec4(mix(hue,vec3(1),core)*coverage,coverage);}
    vec3 hueAt(vec2 p,vec2 bounds){
      vec2 v=p-bounds*.5;float ray=max(abs(v.x)/(bounds.x*.5),abs(v.y)/(bounds.y*.5));
      vec2 edge=bounds*.5+v/max(ray,.0001);float perimeter=2.*(bounds.x+bounds.y),t;
      if(edge.y<.01)t=edge.x;else if(edge.x>bounds.x-.01)t=bounds.x+edge.y;
      else if(edge.y>bounds.y-.01)t=2.*bounds.x+bounds.y-edge.x;else t=perimeter-edge.y;
      return palette(t/perimeter+phase);}
    void main(){vec2 p=viewport.xy+uv*viewport.zw;
      float contour=boundary(p)+retreat;
      // One boundary throughout. Spatial smoothing rounds intermediate distance
      // ridges; it vanishes at both endpoints, preserving their exact geometry.
      color=light(contour,1.,mix(clamp(min(size.x,size.y)*.18,8.,52.),targetBand,contract),softness,hueAt(p,size));
      if(any(lessThan(p,vec2(0)))||any(greaterThan(p,size)))color=vec4(0);
      // Small rounded boxes share the exact light model and clock, not a second renderer.
      if(controlCount>0&&all(greaterThanEqual(p,controlBounds.xy))&&all(lessThanEqual(p,controlBounds.zw))){
        vec4 controls=vec4(0);
        for(int i=0;i<16;i++){if(i>=controlCount)break;
          vec4 r=controlRects[i];vec2 halfSize=r.zw*.5;float radius=controlRadii[i];
          vec2 q=abs(p-r.xy-halfSize)-halfSize+radius;
          float d=length(max(q,0.))+min(max(q.x,q.y),0.)-radius;
          color*=mix(1.,smoothstep(0.,2.,d-1.),controlAlpha);
          vec4 glow=light(d,controlAlpha,clamp(min(r.z,r.w)*.18,8.,52.),softness,hueAt(p-r.xy,r.zw));
          controls=glow+controls*(1.-glow.a);
        }
        if(all(greaterThanEqual(p,controlWork.xy))&&all(lessThanEqual(p,controlWork.zw)))
          color=controls+color*(1.-controls.a);
      }}`;
  function compile(kind, source) {
    const shader = gl.createShader(kind); gl.shaderSource(shader, source); gl.compileShader(shader);
    if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
      const error = gl.getShaderInfoLog(shader); gl.deleteShader(shader); throw new Error(error);
    }
    return shader;
  }
  const program = gl.createProgram(); let vs, fs;
  try {
    vs = compile(gl.VERTEX_SHADER, vertex); fs = compile(gl.FRAGMENT_SHADER, fragment);
    gl.attachShader(program, vs); gl.attachShader(program, fs); gl.linkProgram(program);
    if (!gl.getProgramParameter(program, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(program));
  } catch(error) { gl.deleteProgram(program); throw error; }
  finally { if (vs) gl.deleteShader(vs); if (fs) gl.deleteShader(fs); }
  const uniforms = Object.fromEntries(['size', 'field', 'fieldRange', 'targetBand', 'morph', 'contract', 'softness', 'phase', 'alpha', 'retreat',
    'viewport', 'controlAlpha', 'controlWork', 'controlCount', 'controlBounds', 'controlRects[0]', 'controlRadii[0]']
    .map(name => [name, gl.getUniformLocation(program, name)]));
  let motion;
  try { motion = window.__SGT_PROCESSING_MOTION__(gl); }
  catch(error) { gl.deleteProgram(program); throw error; }
  let range = 1, targetBand = 12;
  return {
    setControls(boxes, work = [-1e6,-1e6,1e6,1e6]) {
      boxes = boxes.slice(0,16);
      gl.useProgram(program); gl.uniform1i(uniforms.controlCount, boxes.length);
      if (!boxes.length) return;
      gl.uniform4fv(uniforms.controlWork, work);
      gl.uniform4fv(uniforms['controlRects[0]'], new Float32Array(boxes.flatMap(r => [r.x,r.y,r.w,r.h])));
      gl.uniform1fv(uniforms['controlRadii[0]'], new Float32Array(boxes.map(r => r.radius)));
      gl.uniform4f(uniforms.controlBounds, Math.min(...boxes.map(r=>r.x))-2, Math.min(...boxes.map(r=>r.y))-2,
        Math.max(...boxes.map(r=>r.x+r.w))+2, Math.max(...boxes.map(r=>r.y+r.h))+2);
    },
    upload(data, now = performance.now()) {
      range = data.range; targetBand = Math.min(24, data.padding * 0.85);
      motion.upload(data, now);
    },
    draw(width, height, morph, phase, alpha, contract = morph, softness = 0, now = performance.now(), reduced = false, retreat = 0,
      viewport = [0,0,width,height], controlAlpha = 1) {
      motion.advance(now, reduced);
      gl.viewport(0, 0, canvas.width, canvas.height); gl.useProgram(program);
      gl.bindTexture(gl.TEXTURE_2D, motion.texture); gl.uniform1i(uniforms.field, 0);
      gl.uniform2f(uniforms.size, width, height); gl.uniform1f(uniforms.fieldRange, range);
      gl.uniform4fv(uniforms.viewport, viewport); gl.uniform1f(uniforms.controlAlpha, controlAlpha);
      gl.uniform1f(uniforms.targetBand, targetBand);
      gl.uniform1f(uniforms.contract, contract);
      gl.uniform1f(uniforms.retreat, retreat);
      gl.uniform1f(uniforms.softness, softness);
      gl.uniform1f(uniforms.morph, morph); gl.uniform1f(uniforms.phase, phase); gl.uniform1f(uniforms.alpha, alpha);
      gl.drawArrays(gl.TRIANGLES, 0, 3);
    },
    destroy() { motion.destroy(); gl.deleteProgram(program); }
  };
};
