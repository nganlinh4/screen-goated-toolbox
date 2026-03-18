// M3E Loading Indicator - standalone init (no React)
// Reads from global scope populated by m3e_loading_indicator.js (RoundedPolygon, Morph)
window.initM3ELoading = function(canvasEl, opts) {
  var size = (opts && opts.size) || 36;
  var isDark = (opts && opts.isDark !== undefined) ? opts.isDark : true;
  var showContainer = (opts && opts.showContainer !== undefined) ? opts.showContainer : false;
  var shapeColor = isDark ? '#485E92' : '#B0C6FF';
  if (showContainer) shapeColor = isDark ? '#D9E2FF' : '#324574';
  var containerColor = isDark ? '#2E4578' : '#ADC3FE';

  var dpr = window.devicePixelRatio || 1;
  var scaleFactor = size <= 24 ? 3.0 : size <= 48 ? 2.5 : 2.2;
  var canvasSize = Math.round(size * scaleFactor);
  canvasEl.width = canvasSize * dpr;
  canvasEl.height = canvasSize * dpr;
  canvasEl.style.width = size + 'px';
  canvasEl.style.height = size + 'px';
  var ctx = canvasEl.getContext('2d');
  ctx.scale(dpr, dpr);

  var state = {
    morphShapes: [], currentMorph: null, morphProgress: 0,
    rotationAngle: 0, pulseValue: 1, animationTime: 0,
    discreteSpinSpeed: 0, isAnimating: true,
    currentShapeIndex: 0, nextShapeIndex: 1, shapeOrder: []
  };

  function mkCircle(r, sides) {
    var v = new Float32Array(sides * 2);
    for (var i = 0; i < sides; i++) { var a = (i / sides) * 2 * Math.PI; v[i*2] = Math.cos(a)*r; v[i*2+1] = Math.sin(a)*r; }
    return new RoundedPolygon(v, 3);
  }
  function mkStar(r, pts) {
    var v = new Float32Array(pts * 4); var ir = r * 0.4; var vi = 0;
    for (var i = 0; i < pts; i++) {
      var oa = (i/pts)*2*Math.PI - Math.PI/2; v[vi++]=Math.cos(oa)*r; v[vi++]=Math.sin(oa)*r;
      var ia = ((i+0.5)/pts)*2*Math.PI - Math.PI/2; v[vi++]=Math.cos(ia)*ir; v[vi++]=Math.sin(ia)*ir;
    }
    return new RoundedPolygon(v, 2);
  }
  function mkGear(r, teeth) {
    var ir = r*0.6; var v = new Float32Array(teeth*4); var vi = 0;
    for (var i = 0; i < teeth; i++) {
      var ba = (i/teeth)*2*Math.PI; var ta = ((i+0.5)/teeth)*2*Math.PI;
      v[vi++]=Math.cos(ba)*ir; v[vi++]=Math.sin(ba)*ir;
      v[vi++]=Math.cos(ta)*r; v[vi++]=Math.sin(ta)*r;
    }
    return new RoundedPolygon(v, 2);
  }
  function mkFlower(r, petals) {
    var v = new Float32Array(petals*4); var vi = 0;
    for (var i = 0; i < petals; i++) {
      var a = (i/petals)*2*Math.PI;
      v[vi++]=Math.cos(a)*r; v[vi++]=Math.sin(a)*r;
      v[vi++]=Math.cos(a)*r*0.3; v[vi++]=Math.sin(a)*r*0.3;
    }
    return new RoundedPolygon(v, 6);
  }

  var shapes = [
    new RoundedPolygon(new Float32Array([0,-20,17,10,-17,10]), 6),
    new RoundedPolygon(new Float32Array([-15,-15,15,-15,15,15,-15,15]), 8),
    new RoundedPolygon(new Float32Array([0,-17,16,-5,10,14,-10,14,-16,-5]), 5),
    mkStar(15, 5),
    new RoundedPolygon(new Float32Array([20,0,10,17,-10,17,-20,0,-10,-17,10,-17]), 4),
    mkCircle(15, 8),
    mkStar(18, 6),
    new RoundedPolygon(new Float32Array([0,-18,18,0,0,18,-18,0]), 4),
    mkGear(16, 8),
    mkFlower(15, 8),
    mkStar(14, 4),
    mkStar(12, 8),
  ];
  state.morphShapes = shapes;

  function shuffle(arr) {
    var a = arr.slice(); for (var i=a.length-1;i>0;i--){var j=Math.floor(Math.random()*(i+1));var t=a[i];a[i]=a[j];a[j]=t;} return a;
  }
  state.shapeOrder = shuffle(Array.from({length:shapes.length},function(_,i){return i;}));

  function drawCubics(cubics, color) {
    if (!cubics || cubics.length === 0) return;
    ctx.fillStyle = color; ctx.beginPath();
    ctx.moveTo(cubics[0].anchor0X, cubics[0].anchor0Y);
    for (var i = 0; i < cubics.length; i++) {
      var c = cubics[i];
      ctx.bezierCurveTo(c.control0X, c.control0Y, c.control1X, c.control1Y, c.anchor1X, c.anchor1Y);
    }
    ctx.closePath(); ctx.fill();
  }

  function drawPoly(poly) { if (poly && poly.cubics) drawCubics(poly.cubics, shapeColor); }

  function applyEffects() {
    state.animationTime += 0.05;
    if (state.currentMorph && state.morphProgress < 1.0) {
      var mp = state.morphProgress;
      if (mp < 0.8) { state.discreteSpinSpeed = 6.0; }
      else { var bp=(mp-0.8)/0.2; var sf=1-bp; var b=Math.sin(bp*Math.PI*2.5); state.discreteSpinSpeed=6.0*sf+(-1.2)*b*sf; }
    } else { state.discreteSpinSpeed = 0.05; }
    state.rotationAngle += state.discreteSpinSpeed;
    ctx.rotate((state.rotationAngle * Math.PI) / 180);
    var baseScale = size <= 24 ? 1.5 : 2.5;
    var syncedScale;
    if (state.currentMorph && state.morphProgress < 1.0) {
      var mp2 = state.morphProgress;
      var sv = mp2 < 0.8 ? 0.015+Math.sin(state.animationTime*4)*0.005 : 0.015+Math.sin(((mp2-0.8)/0.2)*Math.PI)*0.025;
      syncedScale = baseScale + sv;
    } else { syncedScale = baseScale + Math.sin(state.animationTime*1.2)*0.05; }
    ctx.scale(syncedScale, syncedScale);
  }

  function render() {
    if (!state.isAnimating) return;
    ctx.clearRect(0, 0, canvasSize, canvasSize);
    if (showContainer) {
      ctx.save(); ctx.translate(canvasSize/2, canvasSize/2);
      ctx.beginPath(); ctx.arc(0,0,canvasSize*0.45,0,2*Math.PI);
      ctx.fillStyle = containerColor; ctx.fill(); ctx.restore();
    }
    ctx.save(); ctx.translate(canvasSize/2, canvasSize/2);
    applyEffects();

    if (!state.currentMorph && state.morphShapes.length > 0) {
      var ci = state.shapeOrder[state.currentShapeIndex];
      var ni = state.shapeOrder[state.nextShapeIndex];
      state.currentMorph = new Morph(state.morphShapes[ci], state.morphShapes[ni]);
    }
    if (state.currentMorph) {
      var inc = state.morphProgress < 0.8 ? 0.03 : Math.max(0.001, 0.03*(1-(state.morphProgress-0.8)/0.2));
      state.morphProgress += inc;
      if (state.morphProgress >= 1.0) {
        state.morphProgress = 0;
        state.currentShapeIndex = state.nextShapeIndex;
        state.nextShapeIndex = (state.nextShapeIndex + 1) % state.shapeOrder.length;
        if (state.nextShapeIndex === 0) {
          state.shapeOrder = shuffle(Array.from({length:state.morphShapes.length},function(_,i){return i;}));
          state.currentShapeIndex = 0; state.nextShapeIndex = 1;
        }
        var ci2 = state.shapeOrder[state.currentShapeIndex];
        var ni2 = state.shapeOrder[state.nextShapeIndex];
        state.currentMorph = new Morph(state.morphShapes[ci2], state.morphShapes[ni2]);
      }
      try { drawCubics(state.currentMorph.asCubics(state.morphProgress), shapeColor); }
      catch(e) { var sh = state.morphShapes[state.shapeOrder[state.currentShapeIndex]]; if(sh) drawPoly(sh); }
    } else {
      var sh2 = state.morphShapes[state.shapeOrder[state.currentShapeIndex]];
      if(sh2) drawPoly(sh2);
    }
    ctx.restore();
    requestAnimationFrame(render);
  }
  render();
  return { stop: function() { state.isAnimating = false; } };
};
