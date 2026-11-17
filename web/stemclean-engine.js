// StemClean DNR — collegamento tra la pagina e il motore Rust (WebAssembly).
// Qui non c'è calcolo audio: tutto il lavoro lo fa il modulo .wasm.
const StemCleanRust = (() => {
  const VERSION = 20000;
  const BLOCK = 512; // finestre per blocco, tra un blocco e l'altro la pagina aggiorna la barra
  const MODES = { sc: 0, exp: 1, both: 2 };
  const SOURCES = { orig: 0, clean: 1, rem: 2 };
  let ex = null;

  // Le viste vanno ricreate dopo ogni chiamata che può allocare: la memoria del modulo può crescere.
  const f32 = (ptr, n) => new Float32Array(ex.memory.buffer, ptr, n);
  const u8 = (ptr, n) => new Uint8Array(ex.memory.buffer, ptr, n);

  const init = async (bytes) => {
    const { instance } = await WebAssembly.instantiate(bytes, {});
    ex = instance.exports;
    if (ex.sc_version() !== VERSION) throw new Error('versione del motore Rust inattesa');
    return api;
  };

  class Session {
    constructor(sr, L, R) {
      this.sr = sr;
      this.len = L.length;
      this.h = ex.sc_new(sr, this.len);
      f32(ex.sc_buffer(this.h, 0), this.len).set(L);
      f32(ex.sc_buffer(this.h, 1), this.len).set(R.subarray(0, this.len));
      this.frames = ex.sc_frames(this.h);
      this.nb = ex.sc_bands(this.h);
    }

    // Somma un riferimento (tagliato alla lunghezza dello stem).
    addReference(L, R) {
      const n = Math.min(this.len, L.length, R.length);
      ex.sc_stage(this.h, n);
      f32(ex.sc_buffer(this.h, 2), n).set(L.subarray(0, n));
      f32(ex.sc_buffer(this.h, 3), n).set(R.subarray(0, n));
      ex.sc_add_staged_reference(this.h);
    }

    get hasReference() { return ex.sc_has_reference(this.h) === 1; }

    async #blocks(fn, onProgress) {
      for (let t = 0; t < this.frames; t += BLOCK) {
        fn(this.h, t, Math.min(this.frames, t + BLOCK));
        if (onProgress) await onProgress(Math.min(1, (t + BLOCK) / this.frames));
      }
    }

    async analyzeTarget(onProgress) { await this.#blocks(ex.sc_analyze_target, onProgress); }

    async analyzeReference(onProgress) {
      await this.#blocks(ex.sc_analyze_reference, onProgress);
      ex.sc_release_reference(this.h);
    }

    // p = { mode, margin, slope, cut, thr, ratio, att, rel }
    async process(p, onProgress) {
      ex.sc_prepare(this.h, MODES[p.mode] ?? 0, p.margin, p.slope, p.cut, p.thr, p.ratio, p.att, p.rel);
      await this.#blocks(ex.sc_apply, onProgress);
      ex.sc_finish(this.h);
      return { removedDb: ex.sc_metric(this.h, 0), singMean: ex.sc_metric(this.h, 1), singWorst: ex.sc_metric(this.h, 2) };
    }

    // Vista temporanea su un canale ('orig' | 'clean' | 'rem', 0 | 1): da usare subito.
    channel(src, ch) {
      const base = src === 'orig' ? 0 : src === 'clean' ? 10 : 12;
      return f32(ex.sc_buffer(this.h, base + ch), this.len);
    }

    spectrogram(src, pw, ph) { return f32(ex.sc_spectrogram(this.h, SOURCES[src], pw, ph), pw * ph).slice(); }
    reduction(pw, ph) { return f32(ex.sc_reduction(this.h, pw, ph), pw * ph).slice(); }

    // Pixel RGBA per valori già calcolati. palette: 'magma' | 'amber'.
    pixels(vals, vmin, vmax, palette) {
      f32(ex.sc_view_buffer(this.h, vals.length), vals.length).set(vals);
      const ptr = ex.sc_pixels(this.h, vmin, vmax, palette === 'amber' ? 1 : 0);
      return new Uint8ClampedArray(ex.memory.buffer.slice(ptr, ptr + vals.length * 4));
    }

    wav(src, bits) {
      const n = ex.sc_wav(this.h, SOURCES[src], bits);
      const out = u8(ex.sc_export_ptr(this.h), n).slice();
      ex.sc_release_export(this.h);
      return out;
    }

    // files = [{ name, src }]: WAV impacchettati in uno zip non compresso.
    zip(files, bits) {
      const enc = new TextEncoder();
      ex.sc_zip_begin(this.h);
      for (const f of files) {
        ex.sc_wav(this.h, SOURCES[f.src], bits);
        const name = enc.encode(f.name);
        u8(ex.sc_text(this.h, name.length), name.length).set(name);
        ex.sc_zip_add_export(this.h);
      }
      const n = ex.sc_zip_finish(this.h);
      const out = u8(ex.sc_export_ptr(this.h), n).slice();
      ex.sc_release_export(this.h);
      return out;
    }

    free() { if (this.h) { ex.sc_free(this.h); this.h = 0; } }
  }

  // Colore della tavolozza per la barra della legenda.
  const paletteColor = (palette, v) => {
    const c = ex.sc_palette_color(palette === 'amber' ? 1 : 0, v);
    return [(c >> 16) & 255, (c >> 8) & 255, c & 255];
  };

  const api = { init, Session, paletteColor, get ready() { return !!ex; } };
  return api;
})();
if (typeof module !== 'undefined') module.exports = StemCleanRust;
