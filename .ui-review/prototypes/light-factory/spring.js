(function () {
  const reduce = matchMedia('(prefers-reduced-motion: reduce)');
  const live = new Set();
  let raf = 0, last = 0;

  function tick(now) {
    const dt = Math.min((now - last) / 1000, 0.064);
    last = now;
    for (const g of [...live]) g._step(dt);
    raf = live.size ? requestAnimationFrame(tick) : 0;
  }

  /**
   * Springs in Apple's terms: `damping` is the damping ratio (1 = no overshoot),
   * `response` is seconds to reach the target. Re-targeting keeps the live value
   * and velocity, so every animation can be interrupted and reversed.
   */
  class Group {
    constructor(init, { damping = 1, response = 0.35, precision = 0.01 } = {}, onUpdate) {
      this.v = { ...init };
      this.t = { ...init };
      this.vel = {};
      for (const k in init) this.vel[k] = 0;
      this.precision = precision;
      this.onUpdate = onUpdate;
      this.onRest = null;
      this.tune(damping, response);
    }
    tune(damping, response) {
      this.k = (2 * Math.PI / response) ** 2;
      this.c = 4 * Math.PI * damping / response;
    }
    to(targets, { velocity, damping = 1, response, onRest } = {}) {
      Object.assign(this.t, targets);
      if (velocity) Object.assign(this.vel, velocity);
      if (response) this.tune(damping, response);
      this.onRest = onRest || null;
      if (reduce.matches) return this._settle();
      live.add(this);
      if (!raf) { last = performance.now(); raf = requestAnimationFrame(tick); }
      return this;
    }
    set(values) {
      Object.assign(this.v, values);
      Object.assign(this.t, values);
      for (const k in values) this.vel[k] = 0;
      live.delete(this);
      this.onUpdate(this.v);
      return this;
    }
    stop() { live.delete(this); this.onRest = null; }
    _settle() {
      live.delete(this);
      Object.assign(this.v, this.t);
      for (const k in this.vel) this.vel[k] = 0;
      this.onUpdate(this.v);
      const rest = this.onRest;
      this.onRest = null;
      if (rest) rest();
      return this;
    }
    _step(dt) {
      const n = Math.ceil(dt / 0.004), h = dt / n;
      let moving = false;
      for (const k in this.v) {
        let x = this.v[k], v = this.vel[k];
        const t = this.t[k];
        for (let i = 0; i < n; i++) {
          v += (-this.k * (x - t) - this.c * v) * h;
          x += v * h;
        }
        this.v[k] = x;
        this.vel[k] = v;
        if (Math.abs(x - t) > this.precision || Math.abs(v) > this.precision * 20) moving = true;
      }
      if (moving) this.onUpdate(this.v);
      else this._settle();
    }
  }

  /** Where a release at `velocity` px/s comes to rest, as scroll deceleration does. */
  function project(velocity, rate = 0.998) {
    return (velocity / 1000) * rate / (1 - rate);
  }

  function rubberband(overshoot, dimension, constant = 0.55) {
    return (overshoot * dimension * constant) / (dimension + constant * Math.abs(overshoot));
  }

  function tracker() {
    const pts = [];
    return {
      push(x, t) {
        pts.push([x, t]);
        while (pts.length > 2 && t - pts[0][1] > 100) pts.shift();
      },
      /** Zero when the pointer rested before letting go: a pause is not a throw. */
      velocity(now) {
        if (pts.length < 2 || now - pts[pts.length - 1][1] > 80) return 0;
        const [x0, t0] = pts[0], [x1, t1] = pts[pts.length - 1];
        return t1 > t0 ? (x1 - x0) / (t1 - t0) * 1000 : 0;
      },
    };
  }

  window.springs = (init, opts, onUpdate) => new Group(init, opts, onUpdate);
  window.project = project;
  window.rubberband = rubberband;
  window.tracker = tracker;
})();
