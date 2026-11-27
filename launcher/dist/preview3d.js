// A small 3D player for the Cosmetics tab: the player's own skin with a cape, wings and head piece, drawn with WebGL.
// It uses the same shapes, sizes and motion as the Anti-Horizon client mod draws in game (CosmeticsLayer.java), so
// what you see here is what others see. Model space is Minecraft's: 1 unit = 1 skin pixel, y down, the face at -z.
"use strict";

const Preview3D = (() => {
  // ---------------------------------------------------------------- maths (column-major 4x4)
  const I = () => new Float32Array([1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]);
  function mul(a, b) {
    const o = new Float32Array(16);
    for (let c = 0; c < 4; c++)
      for (let r = 0; r < 4; r++)
        o[c * 4 + r] = a[r] * b[c * 4] + a[4 + r] * b[c * 4 + 1] + a[8 + r] * b[c * 4 + 2] + a[12 + r] * b[c * 4 + 3];
    return o;
  }
  const T = (x, y, z) => { const m = I(); m[12] = x; m[13] = y; m[14] = z; return m; };
  const S = (x, y, z) => { const m = I(); m[0] = x; m[5] = y; m[10] = z; return m; };
  const RX = (a) => { const c = Math.cos(a), s = Math.sin(a), m = I(); m[5] = c; m[6] = s; m[9] = -s; m[10] = c; return m; };
  const RY = (a) => { const c = Math.cos(a), s = Math.sin(a), m = I(); m[0] = c; m[2] = -s; m[8] = s; m[10] = c; return m; };
  const RZ = (a) => { const c = Math.cos(a), s = Math.sin(a), m = I(); m[0] = c; m[1] = s; m[4] = -s; m[5] = c; return m; };
  const chain = (...ms) => ms.reduce((a, b) => mul(a, b));
  const deg = (d) => (d * Math.PI) / 180;
  function point(m, x, y, z) {
    return [m[0] * x + m[4] * y + m[8] * z + m[12], m[1] * x + m[5] * y + m[9] * z + m[13], m[2] * x + m[6] * y + m[10] * z + m[14]];
  }
  function dir(m, x, y, z) {
    const v = [m[0] * x + m[4] * y + m[8] * z, m[1] * x + m[5] * y + m[9] * z, m[2] * x + m[6] * y + m[10] * z];
    const l = Math.hypot(v[0], v[1], v[2]) || 1;
    return [v[0] / l, v[1] / l, v[2] / l];
  }
  function perspective(fovy, aspect, near, far) {
    const f = 1 / Math.tan(fovy / 2), m = new Float32Array(16);
    m[0] = f / aspect; m[5] = f; m[10] = (far + near) / (near - far); m[11] = -1; m[14] = (2 * far * near) / (near - far);
    return m;
  }
  // Minecraft's model space (y down, face at -z) into WebGL's (y up, the camera looking down -z at the face).
  const FLIP = S(1, -1, -1);

  // ---------------------------------------------------------------- drawing batches

  class Batch {
    constructor(tex, mode) { this.tex = tex; this.mode = mode; this.v = []; }
    // Four corners (a, b, c, d) in the given matrix, with their texture coordinates, at an opacity.
    quad(m, a, b, c, d, ua, ub, uc, ud, n, alpha = 1) {
      const nn = dir(m, n[0], n[1], n[2]);
      const pa = point(m, ...a), pb = point(m, ...b), pc = point(m, ...c), pd = point(m, ...d);
      for (const [p, uv] of [[pa, ua], [pb, ub], [pc, uc], [pa, ua], [pc, uc], [pd, ud]]) this.v.push(p[0], p[1], p[2], uv[0], uv[1], nn[0], nn[1], nn[2], alpha);
    }
  }

  /**
   * A Minecraft box (as in a skin) of w x h x d at (x, y, z), its texture laid out from (u, v) on a texW x texH sheet,
   * grown by `grow` on every side (the skin's outer layer).
   */
  function skinBox(b, m, x, y, z, w, h, d, u, v, grow = 0, texW = 64, texH = 64) {
    const x0 = x - grow, y0 = y - grow, z0 = z - grow, x1 = x + w + grow, y1 = y + h + grow, z1 = z + d + grow;
    const uv = (pu, pv) => [pu / texW, pv / texH];
    const r = (pu, pv, pw, ph) => [uv(pu, pv), uv(pu + pw, pv), uv(pu + pw, pv + ph), uv(pu, pv + ph)];
    let q;
    q = r(u + d, v + d, w, h); b.quad(m, [x0, y0, z0], [x1, y0, z0], [x1, y1, z0], [x0, y1, z0], ...q, [0, 0, -1]); // front
    q = r(u + d + w + d, v + d, w, h); b.quad(m, [x1, y0, z1], [x0, y0, z1], [x0, y1, z1], [x1, y1, z1], ...q, [0, 0, 1]); // back
    q = r(u, v + d, d, h); b.quad(m, [x0, y0, z1], [x0, y0, z0], [x0, y1, z0], [x0, y1, z1], ...q, [-1, 0, 0]); // right side
    q = r(u + d + w, v + d, d, h); b.quad(m, [x1, y0, z0], [x1, y0, z1], [x1, y1, z1], [x1, y1, z0], ...q, [1, 0, 0]); // left side
    q = r(u + d, v, w, d); b.quad(m, [x0, y0, z1], [x1, y0, z1], [x1, y0, z0], [x0, y0, z0], ...q, [0, -1, 0]); // top
    q = r(u + d + w, v, w, d); b.quad(m, [x0, y1, z0], [x1, y1, z0], [x1, y1, z1], [x0, y1, z1], ...q, [0, 1, 0]); // bottom
  }

  // ---------------------------------------------------------------- the player and the cosmetics

  function player(batches, skinTex, slim, t) {
    const base = batches.get(skinTex, 0);
    const swing = Math.sin(t * 0.067) * 0.05, out = Math.cos(t * 0.09) * 0.05 + 0.05;
    const aw = slim ? 3 : 4;
    const parts = [
      // matrix, box, inner uv, outer uv
      [I(), [-4, -8, -4, 8, 8, 8], [0, 0], [32, 0], 0.5], // head + hat
      [I(), [-4, 0, -2, 8, 12, 4], [16, 16], [16, 32], 0.25], // body + jacket
      [chain(T(-5, 2, 0), RX(swing), RZ(out)), [slim ? -2 : -3, -2, -2, aw, 12, 4], [40, 16], [40, 32], 0.25], // right arm
      [chain(T(5, 2, 0), RX(-swing), RZ(-out)), [-1, -2, -2, aw, 12, 4], [32, 48], [48, 48], 0.25], // left arm
      [T(-1.9, 12, 0), [-2, 0, -2, 4, 12, 4], [0, 16], [0, 32], 0.25], // right leg
      [T(1.9, 12, 0), [-2, 0, -2, 4, 12, 4], [16, 48], [0, 48], 0.25], // left leg
    ];
    const outer = batches.get(skinTex, 0);
    for (const [m, [x, y, z, w, h, d], [u, v], [ou, ov], grow] of parts) {
      skinBox(base, mul(FLIP, m), x, y, z, w, h, d, u, v);
      skinBox(outer, mul(FLIP, m), x, y, z, w, h, d, ou, ov, grow);
    }
  }

  // The cape, as the client mod draws it (CosmeticsLayer.java): hung 2 pixels behind the body and swung like
  // Minecraft's, the inside and edges still, the outside the sharp face picture with a second copy over it that turns
  // around the black hole, shown only where the disk is.
  const COLS = 24, ROWS = 38, TURN_SECONDS = 8;
  const smooth = (x) => { x = Math.max(0, Math.min(1, x)); return x * x * (3 - 2 * x); };
  function disk(f, s, t, turn) {
    const aspect = 1.6, dx = s - f.x, dy = (t - f.y) * aspect;
    const tilt = deg(f.tilt), ca = Math.cos(tilt), sa = Math.sin(tilt);
    const ex = dx * ca + dy * sa, ey = (-dx * sa + dy * ca) / f.ratio;
    const r = Math.hypot(ex, ey);
    const inner = smooth((Math.hypot(dx, dy) - f.hole * 1.15) / (f.hole * 0.45));
    const outer = 1 - smooth((r - f.outer * 0.6) / (f.outer * 0.4));
    const c = Math.cos(turn), sn = Math.sin(turn);
    const rx = ex * c - ey * sn, ry = (ex * sn + ey * c) * f.ratio;
    const u = f.x + rx * ca - ry * sa, v = f.y + (rx * sa + ry * ca) / aspect;
    const edge = Math.min(u, 1 - u, v, 1 - v);
    return [u, v, inner * outer * smooth(edge / 0.22)];
  }

  function cape(batches, sheet, face, f, t, seconds) {
    if (!sheet || !face || !f) return;
    // The game swings it from the player's movement; here it sways gently.
    const lean = 5 + Math.sin(t * 0.05) * 3, lean2 = Math.sin(t * 0.031) * 5;
    const m = chain(FLIP, T(0, 0, 2), RX(deg(6 + lean / 2)), RZ(deg(lean2 / 2)), RY(deg(180 - lean2 / 2)));
    const U = (u) => u / 64, V = (v) => v / 32;
    const r = (u, v, w, h) => [[U(u), V(v)], [U(u + w), V(v)], [U(u + w), V(v + h)], [U(u), V(v + h)]];
    const sb = batches.get(sheet, 0);
    sb.quad(m, [5, 0, 0], [-5, 0, 0], [-5, 16, 0], [5, 16, 0], ...r(12, 1, 10, 16), [0, 0, 1]); // inside
    sb.quad(m, [-5, 0, -1], [-5, 0, 0], [-5, 16, 0], [-5, 16, -1], ...r(0, 1, 1, 16), [-1, 0, 0]);
    sb.quad(m, [5, 0, 0], [5, 0, -1], [5, 16, -1], [5, 16, 0], ...r(11, 1, 1, 16), [1, 0, 0]);
    sb.quad(m, [-5, 0, 0], [5, 0, 0], [5, 0, -1], [-5, 0, -1], ...r(1, 0, 10, 1), [0, -1, 0]);
    sb.quad(m, [-5, 16, -1], [5, 16, -1], [5, 16, 0], [-5, 16, 0], ...r(11, 0, 10, 1), [0, 1, 0]);
    const turn = 2 * Math.PI * ((seconds / TURN_SECONDS) % 1) * f.spin;
    const at = [];
    for (let j = 0; j <= ROWS; j++) for (let i = 0; i <= COLS; i++) at.push(disk(f, i / COLS, j / ROWS, turn));
    // The still picture, dimmed where the disk is; then the turning copy added on top as light, only there.
    const still = batches.get(face, 0), glow = batches.get(face, 2);
    const nn = dir(m, 0, 0, -1);
    for (let j = 0; j < ROWS; j++)
      for (let i = 0; i < COLS; i++) {
        const k = j * (COLS + 1) + i, p = [at[k], at[k + 1], at[k + COLS + 2], at[k + COLS + 1]];
        const x0 = -5 + (10 * i) / COLS, x1 = -5 + (10 * (i + 1)) / COLS, y0 = (16 * j) / ROWS, y1 = (16 * (j + 1)) / ROWS;
        const pts = [[x0, y0], [x1, y0], [x1, y1], [x0, y1]];
        const any = p[0][2] + p[1][2] + p[2][2] + p[3][2] > 0;
        for (const [i1, i2, i3] of [[0, 1, 2], [0, 2, 3]]) {
          for (const q of [i1, i2, i3]) {
            const mm = p[q][2] * (f.mix ?? 1);
            const w = point(m, pts[q][0], pts[q][1], -1);
            still.v.push(w[0], w[1], w[2], (pts[q][0] + 5) / 10, pts[q][1] / 16, nn[0], nn[1], nn[2], 1 - mm);
            if (any) {
              const g = point(m, pts[q][0], pts[q][1], -1.02);
              glow.v.push(g[0], g[1], g[2], p[q][0], p[q][1], nn[0], nn[1], nn[2], mm);
            }
          }
        }
      }
  }

  /** A live cape: the same hang and sway, its outside a clip (the `crop` part of it: [x0, y0, x1, y1]). */
  function liveCape(batches, sheet, face, crop, t) {
    if (!sheet || !face) return;
    const lean = 5 + Math.sin(t * 0.05) * 3, lean2 = Math.sin(t * 0.031) * 5;
    const m = chain(FLIP, T(0, 0, 2), RX(deg(6 + lean / 2)), RZ(deg(lean2 / 2)), RY(deg(180 - lean2 / 2)));
    const U = (u) => u / 64, V = (v) => v / 32;
    const r = (u, v, w, h) => [[U(u), V(v)], [U(u + w), V(v)], [U(u + w), V(v + h)], [U(u), V(v + h)]];
    const sb = batches.get(sheet, 0);
    sb.quad(m, [5, 0, 0], [-5, 0, 0], [-5, 16, 0], [5, 16, 0], ...r(12, 1, 10, 16), [0, 0, 1]);
    sb.quad(m, [-5, 0, -1], [-5, 0, 0], [-5, 16, 0], [-5, 16, -1], ...r(0, 1, 1, 16), [-1, 0, 0]);
    sb.quad(m, [5, 0, 0], [5, 0, -1], [5, 16, -1], [5, 16, 0], ...r(11, 1, 1, 16), [1, 0, 0]);
    sb.quad(m, [-5, 0, 0], [5, 0, 0], [5, 0, -1], [-5, 0, -1], ...r(1, 0, 10, 1), [0, -1, 0]);
    sb.quad(m, [-5, 16, -1], [5, 16, -1], [5, 16, 0], [-5, 16, 0], ...r(11, 0, 10, 1), [0, 1, 0]);
    const [x0, y0, x1, y1] = crop;
    batches.get(face, 0).quad(m, [-5, 0, -1], [5, 0, -1], [5, 16, -1], [-5, 16, -1], [x0, y0], [x1, y0], [x1, y1], [x0, y1], [0, 0, -1]);
  }

  // Jointed pixel wings, as the client mod draws them (CosmeticsLayer.java): shoulder, forearm and hand.
  const PIECES = [[6, 12, 0, 12, 24], [8, 15, 12, 16, 30], [11, 18, 28, 22, 36]];
  const REST = [[30, 64], [10, 20], [12, -28]];
  function wings(batches, tex, t) {
    if (!tex) return;
    const b = batches.get(tex, 0);
    const rate = 0.07, beat = 3;
    for (const side of [-1, 1]) {
      let m = chain(FLIP, T(side * 1.5, 1.0, 2.4));
      for (let k = 0; k < 3; k++) {
        const yaw = REST[k][0], roll = REST[k][1] + Math.sin(t * rate - k * 0.7) * beat * (k === 0 ? 1 : 0.7);
        if (k > 0) m = mul(m, T(side * PIECES[k - 1][0], 0, 0));
        m = chain(m, RY(deg(-side * yaw)), RZ(deg(-side * roll)));
        const [w, h, ax, aw, ah] = PIECES[k];
        const x = side * w, u0 = ax / 64, u1 = (ax + aw) / 64, v1 = ah / 64;
        b.quad(m, [0, -0.5, 0], [x, -0.5, 0], [x, h, 0], [0, h, 0], [u0, 0], [u1, 0], [u1, v1], [u0, v1], [0, 0, 1]);
      }
    }
  }

  function head(batches, kind, tex, t) {
    if (!tex) return;
    if (kind === "halo") {
      const b = batches.get(tex, 2);
      const m = chain(FLIP, T(0, -11.6 + Math.sin(t * 0.08) * 0.35, 0));
      const n = 40, k = 8, r = 4.4, a = 0.55;
      const at = (i, j) => {
        const u = (2 * Math.PI * i) / n, v = (2 * Math.PI * j) / k;
        return [(r + a * Math.cos(v)) * Math.cos(u), a * Math.sin(v), (r + a * Math.cos(v)) * Math.sin(u)];
      };
      for (let i = 0; i < n; i++)
        for (let j = 0; j < k; j++)
          b.quad(m, at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1), [i / n, j / k], [(i + 1) / n, j / k], [(i + 1) / n, (j + 1) / k], [i / n, (j + 1) / k], [0, -1, 0]);
    } else if (kind === "accretion") {
      const b = batches.get(tex, 2);
      const m = chain(FLIP, T(0, -4.5, 0), RX(deg(18)), RZ(deg(-9)), RY(deg(t * 1.8)));
      const n = 48, r = 11, inner = 0.6 * r, k = inner / r;
      for (let i = 0; i < n; i++) {
        const a0 = (2 * Math.PI * i) / n, a1 = (2 * Math.PI * (i + 1)) / n;
        const c0 = Math.cos(a0), s0 = Math.sin(a0), c1 = Math.cos(a1), s1 = Math.sin(a1);
        b.quad(m, [c0 * r, 0, s0 * r], [c1 * r, 0, s1 * r], [c1 * inner, 0, s1 * inner], [c0 * inner, 0, s0 * inner],
          [0.5 + c0 / 2, 0.5 + s0 / 2], [0.5 + c1 / 2, 0.5 + s1 / 2], [0.5 + (c1 * k) / 2, 0.5 + (s1 * k) / 2], [0.5 + (c0 * k) / 2, 0.5 + (s0 * k) / 2], [0, -1, 0]);
      }
    }
  }

  // ---------------------------------------------------------------- WebGL

  const VS = `attribute vec3 p; attribute vec2 uv; attribute vec3 n; attribute float al; uniform mat4 vp;
    varying vec2 vUv; varying vec3 vN; varying float vA;
    void main() { vUv = uv; vN = n; vA = al; gl_Position = vp * vec4(p, 1.0); }`;
  const FS = `precision mediump float; varying vec2 vUv; varying vec3 vN; varying float vA; uniform sampler2D tex; uniform int mode;
    void main() {
      vec4 c = texture2D(tex, vUv);
      if (mode == 2) { gl_FragColor = vec4(c.rgb * c.a * vA, c.a); return; }
      if (c.a < (mode == 0 ? 0.5 : 0.04)) discard;
      // Both faces lit, like the game: flip the normal on the side facing away from it.
      vec3 n = normalize(gl_FrontFacing ? vN : -vN);
      float l = 0.58 + 0.42 * max(dot(n, normalize(vec3(-0.4, 0.8, 0.6))), 0.0) + 0.12 * max(dot(n, normalize(vec3(0.6, 0.2, -0.8))), 0.0);
      // Solid parts use the corner value as a brightness (the cape dims where its disk turns), others as opacity.
      gl_FragColor = mode == 0 ? vec4(c.rgb * min(l, 1.1) * vA, 1.0) : vec4(c.rgb * min(l, 1.1), c.a * vA);
    }`;

  class Viewer {
    constructor(canvas, opts = {}) {
      this.canvas = canvas;
      const gl = (this.gl = canvas.getContext("webgl", { alpha: true, antialias: true, premultipliedAlpha: false, preserveDrawingBuffer: !!opts.still }));
      if (!gl) throw new Error("WebGL isn't available");
      const sh = (type, src) => { const s = gl.createShader(type); gl.shaderSource(s, src); gl.compileShader(s); return s; };
      const prog = (this.prog = gl.createProgram());
      gl.attachShader(prog, sh(gl.VERTEX_SHADER, VS));
      gl.attachShader(prog, sh(gl.FRAGMENT_SHADER, FS));
      gl.linkProgram(prog);
      this.buf = gl.createBuffer();
      this.textures = new Map();
      this.look = {};
      this.flows = opts.flows || {};
      this.clips = opts.clips || {};
      this.videoUrl = opts.video;
      this.videos = new Map();
      this.slim = false;
      this.skin = null;
      this.yaw = opts.yaw ?? 155;
      this.pitch = opts.pitch ?? 8;
      this.spin = opts.spin ?? true;
      this.zoom = opts.zoom ?? 1;
      this.target = opts.target ?? 8;
      this.started = performance.now();
      if (!opts.still) this.interact();
    }

    texture(url, smooth) {
      if (!url) return null;
      let t = this.textures.get(url);
      if (t) return t.ready ? t : null;
      const gl = this.gl;
      t = { tex: gl.createTexture(), ready: false };
      this.textures.set(url, t);
      const img = new Image();
      img.onload = () => {
        gl.bindTexture(gl.TEXTURE_2D, t.tex);
        gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL, false);
        gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, img);
        const f = smooth ? gl.LINEAR : gl.NEAREST;
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, f);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, f);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
        gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
        t.ready = true;
        t.height = img.height;
        this.onReady?.();
      };
      img.src = url;
      return null;
    }

    set(skin, slim, look) {
      this.skin = skin;
      this.slim = !!slim;
      this.look = look || {};
    }

    interact() {
      let drag = null;
      this.canvas.addEventListener("pointerdown", (ev) => {
        drag = { x: ev.clientX, y: ev.clientY, yaw: this.yaw, pitch: this.pitch };
        this.canvas.setPointerCapture(ev.pointerId);
        this.spin = false;
      });
      this.canvas.addEventListener("pointermove", (ev) => {
        if (!drag) return;
        this.yaw = drag.yaw + (ev.clientX - drag.x) * 0.6;
        this.pitch = Math.max(-30, Math.min(40, drag.pitch + (ev.clientY - drag.y) * 0.3));
      });
      const end = () => (drag = null);
      this.canvas.addEventListener("pointerup", end);
      this.canvas.addEventListener("pointercancel", end);
    }

    /** Draws one frame; `t` is the game time in ticks (20 a second). */
    draw(t) {
      const gl = this.gl, cv = this.canvas;
      const w = cv.width, h = cv.height;
      gl.viewport(0, 0, w, h);
      gl.clearColor(0, 0, 0, 0);
      gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
      const skin = this.texture(this.skin, false);
      if (!skin) return;
      const L = this.look;
      const tex = (kind, id, smooth = true) => (id ? this.texture(`cosmetics/${kind}/${id}.png`, smooth) : null);
      const groups = new Map();
      const batches = { get: (tx, mode) => { const k = `${mode}|${this.textureKey(tx)}`; if (!groups.has(k)) groups.set(k, new Batch(tx, mode)); return groups.get(k); } };
      player(batches, skin, this.slim, t);
      const clip = this.clips[L.cape];
      const playing = clip ? this.video(clip.video) : null;
      if (clip) liveCape(batches, tex("cape", L.cape), playing || this.texture(`cosmetics/cape/${L.cape}-face.png`, true), playing ? clip.crop : [0, 0, 1, 1], t);
      else cape(batches, tex("cape", L.cape), L.cape ? this.texture(`cosmetics/cape/${L.cape}-face.png`, true) : null, this.flows[L.cape], t, (performance.now() - this.started) / 1000);
      wings(batches, tex("wings", L.wings), t);
      head(batches, L.head, tex("head", L.head), t);

      const dist = 96 / this.zoom;
      const view = chain(T(0, 0, -dist), RX(deg(this.pitch)), RY(deg(this.yaw)), T(0, this.target, 0));
      const vp = mul(perspective(deg(30), w / h, 1, 400), view);
      gl.useProgram(this.prog);
      gl.uniformMatrix4fv(gl.getUniformLocation(this.prog, "vp"), false, vp);
      gl.enable(gl.DEPTH_TEST);
      gl.depthFunc(gl.LEQUAL);
      gl.disable(gl.CULL_FACE);
      gl.bindBuffer(gl.ARRAY_BUFFER, this.buf);
      const attr = (name, size, off) => {
        const loc = gl.getAttribLocation(this.prog, name);
        gl.enableVertexAttribArray(loc);
        gl.vertexAttribPointer(loc, size, gl.FLOAT, false, 36, off);
      };
      // Solid first, then see-through, then glowing (added on top, not hiding what's behind).
      const order = [...groups.values()].sort((a, b) => a.mode - b.mode);
      for (const g of order) {
        if (!g.v.length) continue;
        gl.bufferData(gl.ARRAY_BUFFER, new Float32Array(g.v), gl.STREAM_DRAW);
        attr("p", 3, 0); attr("uv", 2, 12); attr("n", 3, 20); attr("al", 1, 32);
        gl.activeTexture(gl.TEXTURE0);
        gl.bindTexture(gl.TEXTURE_2D, g.tex.tex);
        gl.uniform1i(gl.getUniformLocation(this.prog, "tex"), 0);
        gl.uniform1i(gl.getUniformLocation(this.prog, "mode"), g.mode);
        if (g.mode === 0) { gl.disable(gl.BLEND); gl.depthMask(true); }
        else if (g.mode === 1) { gl.enable(gl.BLEND); gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA); gl.depthMask(true); }
        else { gl.enable(gl.BLEND); gl.blendFunc(gl.ONE, gl.ONE); gl.depthMask(false); }
        gl.drawArrays(gl.TRIANGLES, 0, g.v.length / 9);
      }
      gl.depthMask(true);
    }

    /** A clip playing as a texture (its newest frame each draw), or null until it has a frame. */
    video(id) {
      if (!this.videoUrl) return null;
      let v = this.videos.get(id);
      if (!v) {
        const gl = this.gl;
        v = { tex: gl.createTexture(), ready: false, el: document.createElement("video") };
        this.videos.set(id, v);
        this.textures.set(`video:${id}`, v);
        Object.assign(v.el, { muted: true, loop: true, playsInline: true });
        this.videoUrl(id).then((src) => { v.el.src = src; v.el.play().catch(() => {}); }).catch(() => {});
      }
      if (v.el.readyState < 2) return v.ready ? v : null;
      const gl = this.gl;
      gl.bindTexture(gl.TEXTURE_2D, v.tex);
      gl.pixelStorei(gl.UNPACK_FLIP_Y_WEBGL, false);
      gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, gl.RGBA, gl.UNSIGNED_BYTE, v.el);
      if (!v.ready) {
        for (const [k, val] of [[gl.TEXTURE_MIN_FILTER, gl.LINEAR], [gl.TEXTURE_MAG_FILTER, gl.LINEAR], [gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE], [gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE]]) gl.texParameteri(gl.TEXTURE_2D, k, val);
        v.ready = true;
      }
      return v;
    }

    textureKey(t) {
      for (const [k, v] of this.textures) if (v === t) return k;
      return "?";
    }

    /** Keeps drawing while the canvas is on the page. */
    run() {
      let last = performance.now();
      const frame = (now) => {
        if (!this.canvas.isConnected) return;
        if (this.spin) this.yaw += ((now - last) / 1000) * 14;
        last = now;
        this.draw((now - this.started) / 50);
        requestAnimationFrame(frame);
      };
      requestAnimationFrame(frame);
    }
  }

  /** A plain dark suit with a gold trim, for previewing before an account is added. */
  function mannequin() {
    const c = document.createElement("canvas");
    c.width = 64;
    c.height = 64;
    const g = c.getContext("2d");
    const fill = (col, x, y, w, h) => { g.fillStyle = col; g.fillRect(x, y, w, h); };
    fill("#8592a5", 0, 0, 32, 16); // head
    fill("#33435b", 16, 16, 24, 16); // body
    fill("#33435b", 40, 16, 16, 16); // right arm
    fill("#33435b", 32, 48, 16, 16); // left arm
    fill("#27344a", 0, 16, 16, 16); // right leg
    fill("#27344a", 16, 48, 16, 16); // left leg
    fill("#d8bc8d", 20, 20, 8, 1); // collar
    fill("#d8bc8d", 4, 20, 4, 1); fill("#d8bc8d", 20, 52, 4, 1); // belt line on legs
    return c.toDataURL("image/png");
  }

  return { Viewer, mannequin };
})();
