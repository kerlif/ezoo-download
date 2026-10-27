'use strict';
/* Ezoo Download project page — interactions. Original code, vanilla JS, no dependencies. */
document.documentElement.classList.add('js');
const reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
const finePointer = window.matchMedia('(hover: hover) and (pointer: fine)').matches;

/* Ripple on buttons */
document.addEventListener('click', (e) => {
  const btn = e.target.closest('.btn, .copy-btn');
  if (!btn || reducedMotion) return;
  const rect = btn.getBoundingClientRect();
  const ripple = document.createElement('span');
  ripple.className = 'ripple';
  const size = Math.max(rect.width, rect.height);
  ripple.style.width = ripple.style.height = size + 'px';
  ripple.style.left = (e.clientX - rect.left - size / 2) + 'px';
  ripple.style.top = (e.clientY - rect.top - size / 2) + 'px';
  btn.appendChild(ripple);
  ripple.addEventListener('animationend', () => ripple.remove());
});

/* Click packet burst: little download arrows float up and fade */
if (!reducedMotion) {
  document.addEventListener('pointerdown', (e) => {
    if (e.target.closest('input, select, textarea')) return;
    const colors = ['#8a57f5', '#2e7bf6', '#16a34a'];
    for (let i = 0; i < 7; i++) {
      const p = document.createElement('span');
      p.className = 'packet';
      p.textContent = '↓';
      p.style.left = e.clientX + 'px';
      p.style.top = e.clientY + 'px';
      p.style.color = colors[i % colors.length];
      p.style.fontSize = (14 + Math.random() * 12) + 'px';
      document.body.appendChild(p);
      const dx = (Math.random() - 0.5) * 120;
      const dy = -(50 + Math.random() * 90);
      const rot = (Math.random() - 0.5) * 60;
      p.animate([
        { transform: 'translate(-50%, -50%) rotate(0deg)', opacity: 1 },
        { transform: 'translate(calc(-50% + ' + dx + 'px), calc(-50% + ' + dy + 'px)) rotate(' + rot + 'deg)', opacity: 0 }
      ], { duration: 750 + Math.random() * 350, easing: 'cubic-bezier(.2,.7,.3,1)' }).onfinish = () => p.remove();
    }
  });
}

/* Card tilt (fine pointers only) */
if (finePointer && !reducedMotion) {
  document.querySelectorAll('.card, .dl-card, .shots figure').forEach((el) => {
    el.addEventListener('pointermove', (e) => {
      const r = el.getBoundingClientRect();
      const rx = ((e.clientY - r.top) / r.height - 0.5) * -5;
      const ry = ((e.clientX - r.left) / r.width - 0.5) * 5;
      el.style.setProperty('--rx', rx.toFixed(2) + 'deg');
      el.style.setProperty('--ry', ry.toFixed(2) + 'deg');
    });
    el.addEventListener('pointerleave', () => {
      el.style.setProperty('--rx', '0deg');
      el.style.setProperty('--ry', '0deg');
    });
  });
}

/* Scroll reveal */
(function () {
  const els = document.querySelectorAll('.card, .step, .dl-card, .shots figure, .demo, .install-cmds, .compare');
  els.forEach((el) => el.classList.add('reveal'));
  if ('IntersectionObserver' in window && !reducedMotion) {
    const io = new IntersectionObserver((entries) => {
      entries.forEach((en) => {
        if (en.isIntersecting) { en.target.classList.add('visible'); io.unobserve(en.target); }
      });
    }, { threshold: 0.12 });
    els.forEach((el) => io.observe(el));
  } else {
    els.forEach((el) => el.classList.add('visible'));
  }
})();

/* Copy buttons */
document.querySelectorAll('.copy-btn').forEach((btn) => {
  btn.addEventListener('click', async () => {
    const text = btn.getAttribute('data-copy') || '';
    let ok = false;
    try { await navigator.clipboard.writeText(text); ok = true; }
    catch (_) {
      const ta = document.createElement('textarea');
      ta.value = text; ta.style.position = 'fixed'; ta.style.opacity = '0';
      document.body.appendChild(ta); ta.select();
      try { ok = document.execCommand('copy'); } catch (_) { ok = false; }
      ta.remove();
    }
    const old = btn.textContent;
    btn.textContent = ok ? '✓ تم النسخ' : 'انسخ يدويًا';
    btn.disabled = true;
    setTimeout(() => { btn.textContent = old; btn.disabled = false; }, 1600);
  });
});

/* Download simulator: segmented download with live threads + speed chart.
   Simulation only — no real download happens. */
(function () {
  const panel = document.getElementById('demo-panel');
  if (!panel) return;
  const threadsInput = document.getElementById('threads');
  const threadsValue = document.getElementById('threads-value');
  const sizeSelect = document.getElementById('filesize');
  const startBtn = document.getElementById('sim-start');
  const resetBtn = document.getElementById('sim-reset');
  const statSpeed = document.getElementById('stat-speed');
  const statProgress = document.getElementById('stat-progress');
  const statEta = document.getElementById('stat-eta');
  const statActive = document.getElementById('stat-active');
  const lanesEl = document.getElementById('lanes');
  const resultEl = document.getElementById('demo-result');
  const canvas = document.getElementById('speed-chart');
  const ctx = canvas.getContext('2d');

  const SPEEDS = []; // aggregate speed samples (newest last)
  let lanes = [];
  let running = false;
  let timer = null;
  let startedAt = 0;
  let elapsedBefore = 0; // ms accumulated before a pause
  let sizeMB = parseFloat(sizeSelect.value);

  function buildLanes(progress) {
    const n = parseInt(threadsInput.value, 10);
    lanes = [];
    lanesEl.innerHTML = '';
    for (let i = 0; i < n; i++) {
      const lane = { progress: progress, speed: 0, base: 4 + Math.random() * 6, done: progress >= 1 };
      lane.speed = lane.base * 0.6;
      lanes.push(lane);
      const row = document.createElement('div');
      row.className = 'lane';
      row.innerHTML = '<span class="lane-name">اتصال ' + (i + 1) + '</span>' +
        '<div class="lane-track"><div class="lane-fill"></div></div>' +
        '<span class="lane-speed" dir="ltr">0.0 MB/s</span>';
      lanesEl.appendChild(row);
      lane.fillEl = row.querySelector('.lane-fill');
      lane.speedEl = row.querySelector('.lane-speed');
      renderLane(lane);
    }
  }

  function renderLane(lane) {
    const pct = Math.min(100, lane.progress * 100);
    lane.fillEl.style.width = pct + '%';
    lane.fillEl.classList.toggle('done', lane.done);
    lane.speedEl.textContent = lane.done ? '✓ اكتمل' : lane.speed.toFixed(1) + ' MB/s';
  }

  function totalProgress() {
    if (!lanes.length) return 0;
    return lanes.reduce((s, l) => s + Math.min(1, l.progress), 0) / lanes.length;
  }

  function fmtEta(seconds) {
    if (!isFinite(seconds) || seconds <= 0) return '—';
    const m = Math.floor(seconds / 60), s = Math.round(seconds % 60);
    return m > 0 ? m + ':' + String(s).padStart(2, '0') : s + ' ث';
  }

  function drawChart() {
    const dpr = window.devicePixelRatio || 1;
    const w = canvas.clientWidth, h = canvas.clientHeight || 120;
    if (canvas.width !== Math.round(w * dpr)) { canvas.width = Math.round(w * dpr); canvas.height = Math.round(h * dpr); }
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);
    ctx.strokeStyle = '#e7e8f0'; ctx.lineWidth = 1;
    for (let g = 1; g <= 3; g++) {
      const y = (h / 4) * g;
      ctx.beginPath(); ctx.moveTo(0, y); ctx.lineTo(w, y); ctx.stroke();
    }
    if (SPEEDS.length < 2) return;
    const max = Math.max.apply(null, SPEEDS.concat([10]));
    const grad = ctx.createLinearGradient(0, 0, 0, h);
    grad.addColorStop(0, 'rgba(138,87,245,.35)');
    grad.addColorStop(1, 'rgba(46,123,246,.02)');
    const stepX = w / 59;
    ctx.beginPath();
    SPEEDS.forEach((v, i) => {
      const x = w - (SPEEDS.length - 1 - i) * stepX;
      const y = h - (v / max) * (h - 14) - 4;
      if (i === 0) ctx.moveTo(x, y); else ctx.lineTo(x, y);
    });
    ctx.strokeStyle = '#8a57f5'; ctx.lineWidth = 2; ctx.stroke();
    ctx.lineTo(w, h);
    ctx.lineTo(w - (SPEEDS.length - 1) * stepX, h);
    ctx.closePath();
    ctx.fillStyle = grad; ctx.fill();
  }

  function tick() {
    const dt = 0.1; // seconds per tick
    let active = 0, agg = 0;
    lanes.forEach((lane) => {
      if (lane.done) { lane.speed = 0; return; }
      active++;
      lane.speed = Math.max(0.5, lane.speed + (Math.random() - 0.5) * 2.4);
      lane.speed += (lane.base - lane.speed) * 0.08;
      lane.progress += (lane.speed * dt) / (sizeMB / lanes.length);
      if (lane.progress >= 1) { lane.progress = 1; lane.done = true; lane.speed = 0; }
      agg += lane.speed;
      renderLane(lane);
    });
    const p = totalProgress();
    SPEEDS.push(agg);
    if (SPEEDS.length > 60) SPEEDS.shift();
    statSpeed.textContent = agg.toFixed(1);
    statProgress.textContent = Math.floor(p * 100) + '%';
    statActive.textContent = active;
    statEta.textContent = agg > 0.5 ? fmtEta((sizeMB * (1 - p)) / agg) : '—';
    drawChart();
    if (p >= 1) finish();
  }

  function elapsedMs() {
    return elapsedBefore + (running ? performance.now() - startedAt : 0);
  }

  function stopTimer() {
    if (timer) { clearInterval(timer); timer = null; }
    if (running) { elapsedBefore += performance.now() - startedAt; }
  }

  function finish() {
    stopTimer();
    running = false;
    startBtn.textContent = '▶ ابدأ المحاكاة';
    startBtn.disabled = true;
    threadsInput.disabled = false;
    sizeSelect.disabled = false;
    const secs = elapsedMs() / 1000;
    const avg = sizeMB / Math.max(secs, 0.1);
    // Single-connection estimate under the same simulated conditions:
    // the average of the lanes' base speeds, as if only one ran.
    const singleBase = lanes.length ? lanes.reduce((s, l) => s + l.base, 0) / lanes.length : 6;
    const singleSecs = sizeMB / singleBase;
    const faster = Math.max(1, Math.round(singleSecs / Math.max(secs, 0.1)));
    resultEl.hidden = false;
    resultEl.textContent = 'اكتمل التنزيل (محاكاة) في ' + secs.toFixed(1) + ' ثانية بمتوسط ' + avg.toFixed(1) +
      ' ميجابايت/ث — باتصال واحد كان سيستغرق نحو ' + Math.round(singleSecs) +
      ' ثانية بنفس الظروف، أي أسرع بنحو ' + faster + '× بفضل التقسيم.';
  }

  function start() {
    if (totalProgress() >= 1) reset(false);
    running = true;
    startedAt = performance.now();
    threadsInput.disabled = true;
    sizeSelect.disabled = true;
    startBtn.textContent = '⏸ إيقاف مؤقت';
    resultEl.hidden = true;
    timer = setInterval(tick, 100);
  }

  function pause() {
    stopTimer();
    running = false;
    startBtn.textContent = '▶ استئناف';
    threadsInput.disabled = false;
    sizeSelect.disabled = false;
  }

  function reset(clearResult) {
    stopTimer();
    running = false;
    elapsedBefore = 0;
    SPEEDS.length = 0;
    sizeMB = parseFloat(sizeSelect.value);
    buildLanes(0);
    statSpeed.textContent = '0.0';
    statProgress.textContent = '0%';
    statEta.textContent = '—';
    statActive.textContent = '0';
    startBtn.textContent = '▶ ابدأ المحاكاة';
    startBtn.disabled = false;
    threadsInput.disabled = false;
    sizeSelect.disabled = false;
    if (clearResult !== false) resultEl.hidden = true;
    drawChart();
  }

  startBtn.addEventListener('click', () => { if (running) pause(); else start(); });
  resetBtn.addEventListener('click', () => reset(true));
  threadsInput.addEventListener('input', () => {
    threadsValue.textContent = threadsInput.value;
    if (!running && totalProgress() < 1) buildLanes(totalProgress());
  });
  threadsInput.addEventListener('change', () => {
    if (running) return;
    if (totalProgress() >= 1) reset(true); else buildLanes(totalProgress());
  });
  sizeSelect.addEventListener('change', () => reset(true));
  window.addEventListener('resize', drawChart);

  threadsValue.textContent = threadsInput.value;
  buildLanes(0);
  drawChart();
})();
