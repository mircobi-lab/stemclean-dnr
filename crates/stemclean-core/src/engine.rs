//! Il motore con stato: tiene in memoria lo stem, i riferimenti e i risultati.
//!
//! Le fasi lunghe (analisi e applicazione) lavorano a blocchi di finestre, così
//! la pagina web può aggiornare la barra di avanzamento tra un blocco e l'altro.

use crate::dsp::{self, Bands, Metrics, Params};
use crate::fft::{self, Fft, H, K, N, OFF};

/// Quale segnale si vuole leggere o analizzare.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Original,
    Clean,
    Removed,
}

impl Source {
    pub fn from_code(c: u32) -> Self {
        match c {
            1 => Self::Clean,
            2 => Self::Removed,
            _ => Self::Original,
        }
    }
}

pub struct Engine {
    pub sr: f64,
    pub len: usize,
    pub frames: usize,
    pub bands: Bands,
    fft: Fft,
    win: Vec<f64>,
    re: Vec<f64>,
    im: Vec<f64>,

    /// Stem da pulire (stereo; un file mono ha `r` uguale a `l`).
    pub l: Vec<f32>,
    pub r: Vec<f32>,
    /// Somma dei riferimenti (vuota quando non serve più).
    pub ref_l: Vec<f32>,
    pub ref_r: Vec<f32>,
    /// Livelli grezzi per banda e finestra, in dB.
    pub raw_target: Vec<f32>,
    pub raw_ref: Option<Vec<f32>>,

    /// Risultato dell'ultima elaborazione.
    pub gains: Vec<f32>,
    pub cut: f64,
    pub out_l: Vec<f32>,
    pub out_r: Vec<f32>,
    ws: Vec<f32>,
    pub rem_l: Vec<f32>,
    pub rem_r: Vec<f32>,
    pub metrics: Metrics,
}

impl Engine {
    pub fn new(sr: f64, len: usize) -> Self {
        Self {
            sr,
            len,
            frames: (len + OFF).div_ceil(H),
            bands: Bands::new(sr),
            fft: Fft::new(),
            win: fft::hann(),
            re: vec![0.0; N],
            im: vec![0.0; N],
            l: vec![0.0; len],
            r: vec![0.0; len],
            ref_l: Vec::new(),
            ref_r: Vec::new(),
            raw_target: Vec::new(),
            raw_ref: None,
            gains: Vec::new(),
            cut: 0.0,
            out_l: Vec::new(),
            out_r: Vec::new(),
            ws: Vec::new(),
            rem_l: Vec::new(),
            rem_r: Vec::new(),
            metrics: Metrics::default(),
        }
    }

    /// Crea il motore da due canali già in memoria.
    pub fn with_stem(sr: f64, l: &[f32], r: &[f32]) -> Self {
        let mut e = Self::new(sr, l.len());
        e.l.copy_from_slice(l);
        e.r.copy_from_slice(&r[..l.len()]);
        e
    }

    /// Aggiunge uno stem di riferimento alla somma (tagliato o completato alla lunghezza dello stem).
    pub fn add_reference(&mut self, l: &[f32], r: &[f32]) {
        if self.ref_l.is_empty() {
            self.ref_l = vec![0.0; self.len];
            self.ref_r = vec![0.0; self.len];
        }
        let n = self.len.min(l.len()).min(r.len());
        for j in 0..n {
            self.ref_l[j] += l[j];
            self.ref_r[j] += r[j];
        }
    }

    pub fn has_reference(&self) -> bool {
        !self.ref_l.is_empty() || self.raw_ref.is_some()
    }

    /// Libera la memoria della somma dei riferimenti (dopo l'analisi non serve più).
    pub fn release_reference_audio(&mut self) {
        self.ref_l = Vec::new();
        self.ref_r = Vec::new();
    }

    /// Carica una finestra (L nella parte reale, R nella parte immaginaria).
    fn load_frame(&mut self, reference: bool, start: isize) {
        let (l, r) = if reference {
            (&self.ref_l, &self.ref_r)
        } else {
            (&self.l, &self.r)
        };
        for n in 0..N {
            let i = start + n as isize;
            if i >= 0 && (i as usize) < self.len {
                let i = i as usize;
                self.re[n] = l[i] as f64 * self.win[n];
                self.im[n] = r[i] as f64 * self.win[n];
            } else {
                self.re[n] = 0.0;
                self.im[n] = 0.0;
            }
        }
    }

    fn analyze_into(&mut self, reference: bool, t0: usize, t1: usize) {
        let nb = self.bands.nb;
        let frames = self.frames;
        let norm = 1.0 / ((N / 2) as f64 * (N / 2) as f64);
        let mut acc = vec![0.0f64; nb];
        let mut out = if reference {
            self.raw_ref.take().unwrap()
        } else {
            std::mem::take(&mut self.raw_target)
        };
        for t in t0..t1.min(frames) {
            self.load_frame(reference, (t * H) as isize - OFF as isize);
            self.fft.run(&mut self.re, &mut self.im, false);
            acc.iter_mut().for_each(|v| *v = 0.0);
            // L e R sono impacchettati in una sola FFT complessa: qui si separano le due potenze.
            for k in 0..K {
                let j = (N - k) % N;
                let (a, b, c, d) = (self.re[k], self.im[k], self.re[j], self.im[j]);
                acc[self.bands.band_of[k]] +=
                    ((a + c) * (a + c) + (b - d) * (b - d) + (b + d) * (b + d) + (a - c) * (a - c))
                        / 8.0
                        * norm;
            }
            for b in 0..nb {
                out[b * frames + t] =
                    (10.0 * (acc[b] / self.bands.count[b].max(1.0) + 1e-12).log10()) as f32;
            }
        }
        if reference {
            self.raw_ref = Some(out);
        } else {
            self.raw_target = out;
        }
    }

    /// Analizza le finestre `t0..t1` dello stem.
    pub fn analyze_target(&mut self, t0: usize, t1: usize) {
        if self.raw_target.is_empty() {
            self.raw_target = vec![0.0; self.bands.nb * self.frames];
        }
        self.analyze_into(false, t0, t1);
    }

    /// Analizza le finestre `t0..t1` della somma dei riferimenti.
    pub fn analyze_reference(&mut self, t0: usize, t1: usize) {
        if self.ref_l.is_empty() {
            return;
        }
        if self.raw_ref.is_none() {
            self.raw_ref = Some(vec![0.0; self.bands.nb * self.frames]);
        }
        self.analyze_into(true, t0, t1);
    }

    /// Calcola inviluppi e guadagni e prepara i buffer di uscita.
    pub fn prepare(&mut self, p: &Params) {
        let fps = self.sr / H as f64;
        let (nb, frames) = (self.bands.nb, self.frames);
        let lv = dsp::envelope(&self.raw_target, nb, frames, fps, p.att, p.rel);
        let lo = self
            .raw_ref
            .as_ref()
            .map(|raw| dsp::envelope(raw, nb, frames, fps, p.att, p.rel));
        self.gains = dsp::gains(&lv, lo.as_deref(), nb, frames, p);
        self.cut = p.cut;
        self.out_l = vec![0.0; self.len];
        self.out_r = vec![0.0; self.len];
        self.ws = vec![0.0; self.len];
        self.rem_l = Vec::new();
        self.rem_r = Vec::new();
    }

    /// Applica i guadagni alle finestre `t0..t1` (sovrapposizione e somma).
    pub fn apply(&mut self, t0: usize, t1: usize) {
        let (nb, frames) = (self.bands.nb, self.frames);
        let mut gk = vec![0.0f64; K];
        let mut gb = vec![0.0f64; nb];
        for t in t0..t1.min(frames) {
            let s = (t * H) as isize - OFF as isize;
            self.load_frame(false, s);
            self.fft.run(&mut self.re, &mut self.im, false);
            for b in 0..nb {
                gb[b] = self.gains[b * frames + t] as f64;
            }
            let bd = &self.bands;
            for k in 0..K {
                let interp = gb[bd.i0[k]] * (1.0 - bd.weight[k]) + gb[bd.i1[k]] * bd.weight[k];
                let db = gb[bd.band_of[k]].max(interp);
                gk[k] = if db == 0.0 {
                    1.0
                } else {
                    10f64.powf(db / 20.0)
                };
            }
            let (re, im) = (&mut self.re, &mut self.im);
            re[0] *= gk[0];
            im[0] *= gk[0];
            re[N / 2] *= gk[N / 2];
            im[N / 2] *= gk[N / 2];
            for k in 1..N / 2 {
                let g = gk[k];
                re[k] *= g;
                im[k] *= g;
                re[N - k] *= g;
                im[N - k] *= g;
            }
            self.fft.run(&mut self.re, &mut self.im, true);
            for n in 0..N {
                let i = s + n as isize;
                if i >= 0 && (i as usize) < self.len {
                    let i = i as usize;
                    let w = self.win[n];
                    self.out_l[i] = (self.out_l[i] as f64 + self.re[n] * w) as f32;
                    self.out_r[i] = (self.out_r[i] as f64 + self.im[n] * w) as f32;
                    self.ws[i] = (self.ws[i] as f64 + w * w) as f32;
                }
            }
        }
    }

    /// Normalizza la somma delle finestre, calcola "solo tolto" e le metriche.
    pub fn finish(&mut self) {
        for i in 0..self.len {
            let w = self.ws[i] as f64;
            if w > 1e-9 {
                self.out_l[i] = (self.out_l[i] as f64 / w) as f32;
                self.out_r[i] = (self.out_r[i] as f64 / w) as f32;
            }
        }
        self.ws = Vec::new();
        self.rem_l = (0..self.len)
            .map(|i| (self.l[i] as f64 - self.out_l[i] as f64) as f32)
            .collect();
        self.rem_r = (0..self.len)
            .map(|i| (self.r[i] as f64 - self.out_r[i] as f64) as f32)
            .collect();
        self.metrics = dsp::metrics(&self.l, &self.r, &self.out_l, &self.out_r, self.sr);
    }

    /// Tutto il processo in un colpo solo (per il programma da Terminale).
    pub fn process(&mut self, p: &Params, mut progress: impl FnMut(&str, f64)) {
        const BLOCK: usize = 512;
        let frames = self.frames;
        let mut t = 0;
        while t < frames {
            self.analyze_target(t, t + BLOCK);
            t += BLOCK;
            progress("analisi dello stem", t.min(frames) as f64 / frames as f64);
        }
        if !self.ref_l.is_empty() {
            let mut t = 0;
            while t < frames {
                self.analyze_reference(t, t + BLOCK);
                t += BLOCK;
                progress(
                    "analisi dei riferimenti",
                    t.min(frames) as f64 / frames as f64,
                );
            }
            self.release_reference_audio();
        }
        self.prepare(p);
        let mut t = 0;
        while t < frames {
            self.apply(t, t + BLOCK);
            t += BLOCK;
            progress("elaborazione", t.min(frames) as f64 / frames as f64);
        }
        self.finish();
    }

    pub fn channels(&self, src: Source) -> (&[f32], &[f32]) {
        match src {
            Source::Original => (&self.l, &self.r),
            Source::Clean => (&self.out_l, &self.out_r),
            Source::Removed => (&self.rem_l, &self.rem_r),
        }
    }

    /// Spettrogramma per il disegno (`pw` colonne × `ph` righe, righe dall'alto = frequenze alte).
    pub fn spectrogram(&self, src: Source, pw: usize, ph: usize) -> Vec<f32> {
        let (l, r) = self.channels(src);
        crate::view::spectrogram(&self.fft, &self.win, l, r, self.sr, pw, ph)
    }

    /// Mappa della riduzione applicata (stesse dimensioni dello spettrogramma).
    pub fn reduction_map(&self, pw: usize, ph: usize) -> Vec<f32> {
        crate::view::reduction(&self.gains, &self.bands, self.frames, self.sr, pw, ph)
    }
}
