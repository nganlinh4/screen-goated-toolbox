// A bounded, GPU-resident distance/velocity field. Retargeting changes only the
// destination: the visible contour keeps both position and velocity.
window.__SGT_PROCESSING_MOTION__ = function(gl) {
  const vertex = `#version 300 es
    out vec2 uv;
    void main(){vec2 p=vec2((gl_VertexID<<1)&2,gl_VertexID&2);uv=p;gl_Position=vec4(p*2.-1.,0,1);}`;
  const fragment = `#version 300 es
    precision highp float;
    in vec2 uv;out vec4 state;
    uniform sampler2D previous,target;
    uniform float elapsed,reset;
    float unpack(vec2 p){return (p.x*256.+p.y)/257.*2.-1.;}
    vec2 pack(float v){float n=floor(clamp(v*.5+.5,0.,1.)*65535.+.5);return vec2(floor(n/256.),mod(n,256.))/255.;}
    void main(){vec4 old=texture(previous,uv);float goal=unpack(texture(target,uv).rg);
      float x=unpack(old.rg),v=unpack(old.ba)*8.;
      float delta=x-goal,j=v+9.*delta,e=exp(-9.*elapsed);
      float next=goal+(delta+j*elapsed)*e,velocity=(v-9.*j*elapsed)*e;
      if(reset>.5){next=goal;velocity=0.;}
      state=vec4(pack(next),pack(velocity/8.));}`;
  const shaders = [], textures = [], buffers = [];
  const program = gl.createProgram();
  function cleanup() {
    textures.forEach(t => gl.deleteTexture(t)); buffers.forEach(b => gl.deleteFramebuffer(b));
    shaders.forEach(s => gl.deleteShader(s)); gl.deleteProgram(program);
  }
  try {
    for (const [kind, source] of [[gl.VERTEX_SHADER, vertex], [gl.FRAGMENT_SHADER, fragment]]) {
      const shader = gl.createShader(kind); shaders.push(shader);
      gl.shaderSource(shader, source); gl.compileShader(shader);
      if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(shader));
      gl.attachShader(program, shader);
    }
    gl.linkProgram(program);
    if (!gl.getProgramParameter(program, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(program));
    for (let i = 0; i < 3; i++) {
      const texture = gl.createTexture(); textures.push(texture); gl.bindTexture(gl.TEXTURE_2D, texture);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
      gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
      gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, 1, 1, 0, gl.RGBA, gl.UNSIGNED_BYTE, new Uint8Array([255,255,128,0]));
      if (i < 2) buffers.push(gl.createFramebuffer());
    }
  } catch(error) { cleanup(); throw error; }
  const uniforms = Object.fromEntries(['previous','target','elapsed','reset'].map(n => [n, gl.getUniformLocation(program,n)]));
  let width = 1, height = 1, current = 0, last = null, until = 0, ready = false;
  function integrate(seconds, reset) {
    gl.disable(gl.DITHER); // Packed numeric channels must not be color-dithered.
    const next = 1 - current;
    gl.bindFramebuffer(gl.FRAMEBUFFER, buffers[next]);
    gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, textures[next], 0);
    if (reset && gl.checkFramebufferStatus(gl.FRAMEBUFFER) !== gl.FRAMEBUFFER_COMPLETE) {
      gl.bindFramebuffer(gl.FRAMEBUFFER,null); throw new Error('Processing field framebuffer unavailable');
    }
    gl.viewport(0,0,width,height); gl.useProgram(program);
    gl.activeTexture(gl.TEXTURE0); gl.bindTexture(gl.TEXTURE_2D,textures[current]);
    gl.activeTexture(gl.TEXTURE1); gl.bindTexture(gl.TEXTURE_2D,textures[2]);
    gl.uniform1i(uniforms.previous,0); gl.uniform1i(uniforms.target,1);
    gl.uniform1f(uniforms.elapsed,seconds); gl.uniform1f(uniforms.reset,Number(reset));
    gl.drawArrays(gl.TRIANGLES,0,3); current = next;
    gl.bindFramebuffer(gl.FRAMEBUFFER,null); gl.activeTexture(gl.TEXTURE0);
  }
  return {
    get texture() { return textures[current]; },
    upload(data, now) {
      if (ready && last !== null) integrate(Math.max(0,(now-last)/1000),false);
      const initialize = !ready || width !== data.width || height !== data.height;
      width = data.width; height = data.height;
      gl.activeTexture(gl.TEXTURE0);
      for (let i = initialize ? 0 : 2; i < 3; i++) {
        gl.bindTexture(gl.TEXTURE_2D,textures[i]);
        gl.texImage2D(gl.TEXTURE_2D,0,gl.RGBA,width,height,0,gl.RGBA,gl.UNSIGNED_BYTE,data.pixels);
      }
      if (initialize) integrate(0,true);
      if (gl.getError() !== gl.NO_ERROR) throw new Error('Processing field allocation failed');
      ready = true; last = now; until = now + 1600;
    },
    advance(now, reduced) {
      if (!ready || last === null) return;
      // The analytic spring is stable for the real elapsed time, including a
      // missed frame. Clamping time would lag behind and snap at the deadline.
      integrate(Math.max(0,(now-last)/1000), reduced || now >= until);
      last = reduced || now >= until ? null : now;
    },
    destroy: cleanup
  };
};
