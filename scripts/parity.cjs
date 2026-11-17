// Confronta il motore Rust (WebAssembly) con il motore JavaScript della versione 1.
// Uso: node scripts/parity.cjs <stemclean-v1.html> <stemclean_wasm.wasm> [secondi]
const fs = require('fs');
const path = require('path');
const StemCleanRust = require(path.join(__dirname, '..', 'web', 'stemclean-engine.js'));

const [htmlPath, wasmPath, secArg] = process.argv.slice(2);
const html = fs.readFileSync(htmlPath, 'utf8');
const dspCode = html.split('<script id="dsp">')[1].split('</script>')[0];
const DSP = new Function(dspCode + '\nreturn DSP;')();

// --- segnali di prova deterministici: voce (armoniche con vibrato e pause) + rientri di basso e batteria
const sr = 44100, secs = +(secArg || 30), len = sr * secs;
let seed = 12345;
const rnd = () => { seed = (seed * 1103515245 + 12345) & 0x7fffffff; return seed / 0x7fffffff * 2 - 1; };
const voice = new Float32Array(len), bass = new Float32Array(len), drums = new Float32Array(len);
let ph = 0, bph = 0, hit = 0;
for (let i = 0; i < len; i++) {
  const t = i / sr;
  const note = [220, 247, 262, 294, 330][Math.floor(t / 1.5) % 5];
  const f0 = note * (1 + 0.01 * Math.sin(2 * Math.PI * 5.5 * t));
  ph += 2 * Math.PI * f0 / sr;
  const gate = (t % 3) < 2.2 ? Math.min(1, (t % 3) * 10) : 0; // frasi con pause
  let v = 0;
  for (let h = 1; h <= 12; h++) v += Math.sin(h * ph) / h;
  voice[i] = 0.25 * gate * v;
  bph += 2 * Math.PI * [55, 55, 73.4, 65.4][Math.floor(t / 2) % 4] / sr;
  bass[i] = 0.4 * Math.sin(bph);
  if (i % Math.round(sr * 0.25) === 0) hit = 1;
  drums[i] = 0.5 * hit * rnd(); hit *= 0.9993;
}
const bleed = 0.06;
const stemL = new Float32Array(len), stemR = new Float32Array(len);
for (let i = 0; i < len; i++) {
  stemL[i] = voice[i] + bleed * (bass[i] + drums[i]);
  stemR[i] = 0.9 * voice[i] + bleed * (0.8 * bass[i] + drums[i]);
}
const PRESETS = {
  delicato: { mode: 'sc', margin: -6, slope: 1.5, cut: 18, thr: 12, ratio: 2, att: 5, rel: 80 },
  consigliato: { mode: 'sc', margin: 3, slope: 1.5, cut: 30, thr: 12, ratio: 2, att: 5, rel: 80 },
  estremo: { mode: 'sc', margin: 12, slope: 2, cut: 40, thr: 12, ratio: 2, att: 5, rel: 80 },
  dnr: { mode: 'exp', margin: 3, slope: 1.5, cut: 24, thr: 12, ratio: 2, att: 5, rel: 80 },
  combinata: { mode: 'both', margin: 3, slope: 1.5, cut: 30, thr: 12, ratio: 2, att: 5, rel: 80 },
};

const maxDiff = (a, b) => { let m = 0; for (let i = 0; i < a.length; i++) m = Math.max(m, Math.abs(a[i] - b[i])); return m; };

(async () => {
  // --- motore JavaScript (v1)
  const t0 = Date.now();
  const T = DSP.frameCount(len), bd = DSP.bands(sr);
  const rawV = await DSP.analyze(stemL, stemR, len, T, bd);
  const rawO = await DSP.analyze(bass.map((b, i) => b + drums[i]), bass.map((b, i) => 0.8 * b + drums[i]), len, T, bd);
  const tJsAnalyze = Date.now() - t0;

  // --- motore Rust
  await StemCleanRust.init(fs.readFileSync(wasmPath));
  const t1 = Date.now();
  const s = new StemCleanRust.Session(sr, stemL, stemR);
  s.addReference(bass, bass.map((b) => 0.8 * b));
  s.addReference(drums, drums);
  await s.analyzeTarget();
  await s.analyzeReference();
  const tRsAnalyze = Date.now() - t1;
  console.log(`audio di prova: ${secs} s stereo a ${sr} Hz · finestre ${T} (Rust ${s.frames}) · bande ${bd.nb} (Rust ${s.nb})`);
  console.log(`analisi: JavaScript ${tJsAnalyze} ms · Rust ${tRsAnalyze} ms`);

  let worst = 0;
  for (const [name, p] of Object.entries(PRESETS)) {
    const a = Date.now();
    const fps = sr / DSP.H;
    const Lv = DSP.envelope(rawV, bd.nb, T, fps, p.att, p.rel);
    const Lo = DSP.envelope(rawO, bd.nb, T, fps, p.att, p.rel);
    const G = DSP.gains(Lv, Lo, bd.nb, T, p);
    const [oL, oR] = await DSP.apply(stemL, stemR, len, T, bd, G);
    const mJs = DSP.metrics(stemL, stemR, oL, oR, len, sr);
    const tJs = Date.now() - a;
    const b = Date.now();
    const mRs = await s.process(p);
    const tRs = Date.now() - b;
    const dL = maxDiff(oL, s.channel('clean', 0)), dR = maxDiff(oR, s.channel('clean', 1));
    worst = Math.max(worst, dL, dR);
    const fmt = (m) => `${m.removedDb.toFixed(2)} / ${m.singMean.toFixed(3)} / ${m.singWorst.toFixed(2)}`;
    console.log(`\n${name.padEnd(11)} JS ${tJs} ms · Rust ${tRs} ms (${(tJs / tRs).toFixed(1)}× più veloce)`);
    console.log(`  metriche (tolto dB / media voce / 1% peggiore)  JS: ${fmt(mJs)}   Rust: ${fmt(mRs)}`);
    console.log(`  differenza massima campione per campione: L ${dL.toExponential(2)} · R ${dR.toExponential(2)}`);
  }
  // WAV: stessi byte della v1?
  const wav = s.wav('clean', 24);
  console.log(`\nWAV 24 bit dal motore Rust: ${wav.length} byte, intestazione ${Buffer.from(wav.slice(0, 4)).toString()}/${Buffer.from(wav.slice(8, 12)).toString()}`);
  const zip = s.zip([{ name: 'prova_pulito.wav', src: 'clean' }, { name: 'prova_tolto.wav', src: 'rem' }], 32);
  fs.writeFileSync('/tmp/parity.zip', zip);
  console.log(`ZIP con due WAV 32 bit: ${zip.length} byte → /tmp/parity.zip`);
  s.free();
  console.log(`\nDIFFERENZA MASSIMA SU TUTTI I PRESET: ${worst.toExponential(2)} (${(20 * Math.log10(worst + 1e-30)).toFixed(1)} dBFS)`);
  process.exit(worst < 1e-5 ? 0 : 1);
})();
