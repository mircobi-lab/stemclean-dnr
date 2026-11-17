//! Dati per il grafico: spettrogramma in dBFS, mappa della riduzione, colori.

use crate::dsp::Bands;
use crate::fft::{Fft, K, N};

/// Frequenza più bassa mostrata nel grafico (Hz).
pub const FMIN: f64 = 30.0;

/// Frequenza al centro della riga `y` (scala logaritmica, riga 0 in alto).
pub fn row_freq(y: usize, ph: usize, fmax: f64) -> f64 {
    FMIN * (fmax / FMIN).powf(1.0 - (y as f64 + 0.5) / ph as f64)
}

/// Spettrogramma del segnale mono (L+R)/2: una FFT per colonna, livelli in dBFS
/// (una sinusoide a fondo scala vale 0 dBFS). Valori riga per riga, `ph` righe × `pw` colonne.
pub fn spectrogram(
    fft: &Fft,
    win: &[f64],
    l: &[f32],
    r: &[f32],
    sr: f64,
    pw: usize,
    ph: usize,
) -> Vec<f32> {
    let len = l.len();
    let fmax = sr / 2.0;
    let scale = 2.0 / (N / 2) as f64;
    let mut re = vec![0.0f64; N];
    let mut im = vec![0.0f64; N];
    let mut db = vec![0f32; K];
    let bins = N as f64 / sr;
    let lo: Vec<f64> = (0..ph)
        .map(|y| FMIN * (fmax / FMIN).powf(1.0 - (y + 1) as f64 / ph as f64) * bins)
        .collect();
    let hi: Vec<f64> = (0..ph)
        .map(|y| FMIN * (fmax / FMIN).powf(1.0 - y as f64 / ph as f64) * bins)
        .collect();
    let mid: Vec<f64> = (0..ph).map(|y| row_freq(y, ph, fmax) * bins).collect();
    let mut vals = vec![0f32; pw * ph];
    for x in 0..pw {
        let s = ((x as f64 + 0.5) / pw as f64 * len as f64).floor() as isize - (N / 2) as isize;
        for n in 0..N {
            let i = s + n as isize;
            re[n] = if i >= 0 && (i as usize) < len {
                let i = i as usize;
                (l[i] as f64 + r[i] as f64) * 0.5 * win[n]
            } else {
                0.0
            };
            im[n] = 0.0;
        }
        fft.run(&mut re, &mut im, false);
        for k in 0..K {
            db[k] = (20.0 * (re[k].hypot(im[k]) * scale + 1e-9).log10()) as f32;
        }
        for y in 0..ph {
            let a = lo[y].ceil();
            let b = hi[y].floor();
            let v = if b >= a {
                let mut v = -200.0f32;
                for k in a as usize..=(b as usize).min(K - 1) {
                    v = v.max(db[k]);
                }
                v
            } else {
                let k0 = (K - 2).min(mid[y].floor() as usize);
                let u = mid[y] - k0 as f64;
                (db[k0] as f64 * (1.0 - u) + db[k0 + 1] as f64 * u) as f32
            };
            vals[y * pw + x] = v;
        }
    }
    vals
}

/// Riduzione applicata (dB ≤ 0) riportata sulla griglia del grafico: per ogni colonna
/// prende l'attenuazione più forte tra le finestre che cadono in quella colonna.
pub fn reduction(
    gains: &[f32],
    bands: &Bands,
    frames: usize,
    sr: f64,
    pw: usize,
    ph: usize,
) -> Vec<f32> {
    let nb = bands.nb;
    let fmax = sr / 2.0;
    let row_band: Vec<usize> = (0..ph)
        .map(|y| {
            let f = row_freq(y, ph, fmax);
            let mut b = 0;
            while b < nb - 1 && f >= bands.edges[b + 1] {
                b += 1;
            }
            b
        })
        .collect();
    let mut vals = vec![0f32; pw * ph];
    let mut col = vec![0f32; nb];
    for x in 0..pw {
        let t0 = x * frames / pw;
        let t1 = (t0 + 1).max((x + 1) * frames / pw);
        for b in 0..nb {
            let mut mn = 0f32;
            for t in t0..t1.min(frames) {
                mn = mn.min(gains[b * frames + t]);
            }
            col[b] = mn;
        }
        for y in 0..ph {
            vals[y * pw + x] = col[row_band[y]];
        }
    }
    vals
}

/// Tavolozze del grafico.
#[derive(Clone, Copy, Debug)]
pub enum Palette {
    /// "magma" per lo spettrogramma.
    Magma,
    /// Rosso → ambra → scuro per la riduzione (rosso = attenuato di più).
    Amber,
}

const MAGMA: [(f64, [f64; 3]); 9] = [
    (0.0, [0.0, 0.0, 4.0]),
    (0.13, [28.0, 16.0, 68.0]),
    (0.25, [79.0, 18.0, 123.0]),
    (0.38, [129.0, 37.0, 129.0]),
    (0.5, [181.0, 54.0, 122.0]),
    (0.63, [229.0, 80.0, 100.0]),
    (0.75, [251.0, 135.0, 97.0]),
    (0.88, [254.0, 194.0, 135.0]),
    (1.0, [252.0, 253.0, 191.0]),
];
const AMBER: [(f64, [f64; 3]); 3] = [
    (0.0, [255.0, 90.0, 74.0]),
    (0.5, [255.0, 176.0, 0.0]),
    (1.0, [20.0, 17.0, 14.0]),
];

impl Palette {
    pub fn from_code(c: u32) -> Self {
        if c == 1 { Self::Amber } else { Self::Magma }
    }
    fn stops(self) -> &'static [(f64, [f64; 3])] {
        match self {
            Self::Magma => &MAGMA,
            Self::Amber => &AMBER,
        }
    }
    /// Colore per un valore normalizzato 0..1.
    pub fn color(self, v: f64) -> [f64; 3] {
        let v = v.clamp(0.0, 1.0);
        let st = self.stops();
        for i in 1..st.len() {
            if v <= st[i].0 {
                let (a, ca) = st[i - 1];
                let (b, cb) = st[i];
                let u = (v - a) / (b - a);
                return [
                    ca[0] + (cb[0] - ca[0]) * u,
                    ca[1] + (cb[1] - ca[1]) * u,
                    ca[2] + (cb[2] - ca[2]) * u,
                ];
            }
        }
        st[st.len() - 1].1
    }
}

/// Converte i valori in pixel RGBA (pronti per `ImageData`).
pub fn rgba(vals: &[f32], vmin: f64, vmax: f64, pal: Palette) -> Vec<u8> {
    let mut out = vec![255u8; vals.len() * 4];
    let span = vmax - vmin;
    for (i, &v) in vals.iter().enumerate() {
        let c = pal.color((v as f64 - vmin) / span);
        out[i * 4] = c[0].round() as u8;
        out[i * 4 + 1] = c[1].round() as u8;
        out[i * 4 + 2] = c[2].round() as u8;
    }
    out
}
