//! Le parti del processo che non dipendono dallo stato: bande, inviluppi, guadagni, metriche.

use crate::fft::{K, N};

/// Arrotondamento come `Math.round` di JavaScript (le metà vanno verso +∞).
pub fn js_round(x: f64) -> f64 {
    (x + 0.5).floor()
}

/// Percentile "basso" come nella versione JS: ordina e prende l'elemento floor(q·(n−1)).
fn percentile_f32(values: &[f32], q: f64) -> f32 {
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    sorted[(q * (sorted.len() - 1) as f64).floor() as usize]
}

/// Bande da 1/3 d'ottava: una banda unica sotto i 60 Hz, poi terzi d'ottava fino a Nyquist.
#[derive(Clone, Debug)]
pub struct Bands {
    pub nb: usize,
    /// Banda di appartenenza di ogni bin FFT.
    pub band_of: Vec<usize>,
    /// Numero di bin in ogni banda.
    pub count: Vec<f64>,
    /// Interpolazione logaritmica tra i centri banda: bande vicine e peso.
    pub i0: Vec<usize>,
    pub i1: Vec<usize>,
    pub weight: Vec<f64>,
    /// Frequenze di confine (nb + 1 valori).
    pub edges: Vec<f64>,
}

impl Bands {
    pub fn new(sr: f64) -> Self {
        let mut e = vec![0.0, 60.0];
        let third = 2f64.powf(1.0 / 3.0);
        while *e.last().unwrap() < sr / 2.0 {
            let next = e.last().unwrap() * third;
            e.push(next);
        }
        e.pop();
        e.push(sr / 2.0 + 1.0);
        let nb = e.len() - 1;

        let mut band_of = vec![0usize; K];
        let mut count = vec![0.0; nb];
        for (k, slot) in band_of.iter_mut().enumerate() {
            let f = k as f64 * sr / N as f64;
            let mut b = 0;
            while b < nb - 1 && f >= e[b + 1] {
                b += 1;
            }
            *slot = b;
            count[b] += 1.0;
        }

        let centre: Vec<f64> = (0..nb)
            .map(|b| {
                if b == 0 {
                    30.0
                } else {
                    (e[b] * e[b + 1].min(sr / 2.0)).sqrt()
                }
            })
            .collect();
        let mut i0 = vec![0usize; K];
        let mut i1 = vec![0usize; K];
        let mut weight = vec![0.0; K];
        for k in 0..K {
            let f = (k as f64 * sr / N as f64).max(1e-3);
            if f <= centre[0] {
                continue;
            }
            if f >= centre[nb - 1] {
                i0[k] = nb - 1;
                i1[k] = nb - 1;
                continue;
            }
            let mut b = 0;
            while centre[b + 1] <= f {
                b += 1;
            }
            i0[k] = b;
            i1[k] = b + 1;
            weight[k] = (f.ln() - centre[b].ln()) / (centre[b + 1].ln() - centre[b].ln());
        }
        Self {
            nb,
            band_of,
            count,
            i0,
            i1,
            weight,
            edges: e,
        }
    }
}

/// Come agisce il processo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Attenua dove i riferimenti (gli altri strumenti) coprono lo stem.
    Sidechain,
    /// Espansore verso il basso stile Dolby/DNR, senza riferimenti.
    Expander,
    /// Le due cose insieme: vince l'attenuazione più forte.
    Both,
}

impl Mode {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "sc" | "sidechain" => Some(Self::Sidechain),
            "exp" | "espansore" => Some(Self::Expander),
            "both" | "combinata" => Some(Self::Both),
            _ => None,
        }
    }
    pub fn code(self) -> u32 {
        match self {
            Self::Sidechain => 0,
            Self::Expander => 1,
            Self::Both => 2,
        }
    }
    pub fn from_code(c: u32) -> Self {
        match c {
            1 => Self::Expander,
            2 => Self::Both,
            _ => Self::Sidechain,
        }
    }
}

/// Parametri del processo (gli stessi cursori della pagina).
#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub mode: Mode,
    /// dB di margine: quanto lo stem deve superare i riferimenti prima di essere lasciato intatto.
    pub margin: f64,
    /// Pendenza dell'attenuazione sidechain (dB tolti per dB sotto il margine).
    pub slope: f64,
    /// Attenuazione massima (dB, positivo).
    pub cut: f64,
    /// Soglia dell'espansore sopra il fondo della banda (dB).
    pub thr: f64,
    /// Rapporto di espansione (1:ratio).
    pub ratio: f64,
    /// Attacco e rilascio degli inviluppi (ms).
    pub att: f64,
    pub rel: f64,
}

impl Params {
    pub fn preset(name: &str) -> Option<Self> {
        let p = |mode, margin, slope, cut| Params {
            mode,
            margin,
            slope,
            cut,
            thr: 12.0,
            ratio: 2.0,
            att: 5.0,
            rel: 80.0,
        };
        match name {
            "delicato" => Some(p(Mode::Sidechain, -6.0, 1.5, 18.0)),
            "consigliato" => Some(p(Mode::Sidechain, 3.0, 1.5, 30.0)),
            "estremo" => Some(p(Mode::Sidechain, 12.0, 2.0, 40.0)),
            "dnr" | "espansore" => Some(p(Mode::Expander, 3.0, 1.5, 24.0)),
            _ => None,
        }
    }
}

/// Inviluppo attacco/rilascio per ogni banda, su livelli in dB (righe da `frames` valori).
pub fn envelope(
    raw: &[f32],
    nb: usize,
    frames: usize,
    fps: f64,
    att_ms: f64,
    rel_ms: f64,
) -> Vec<f32> {
    let mut out = vec![0f32; raw.len()];
    let a = (-1.0 / (fps * att_ms / 1000.0)).exp();
    let r = (-1.0 / (fps * rel_ms / 1000.0)).exp();
    for b in 0..nb {
        let row = &raw[b * frames..(b + 1) * frames];
        let dst = &mut out[b * frames..(b + 1) * frames];
        let mut y = row[0] as f64;
        dst[0] = y as f32;
        for t in 1..frames {
            let x = row[t] as f64;
            let c = if x > y { a } else { r };
            y = c * y + (1.0 - c) * x;
            dst[t] = y as f32;
        }
    }
    out
}

/// Guadagno in dB (≤ 0) per banda e per finestra.
pub fn gains(lv: &[f32], lo: Option<&[f32]>, nb: usize, frames: usize, p: &Params) -> Vec<f32> {
    let use_sc = lo.is_some() && p.mode != Mode::Expander;
    let use_exp = lo.is_none() || p.mode != Mode::Sidechain;
    let floor: Vec<f32> = if use_exp {
        (0..nb)
            .map(|b| percentile_f32(&lv[b * frames..(b + 1) * frames], 0.15))
            .collect()
    } else {
        Vec::new()
    };
    let mut g = vec![0f32; nb * frames];
    for b in 0..nb {
        for t in 0..frames {
            let i = b * frames + t;
            let v = lv[i] as f64;
            let mut gd: f64 = 0.0;
            if use_sc {
                let o = lo.unwrap()[i] as f64;
                gd = gd.min(((v - o - p.margin) * p.slope).max(-p.cut));
            }
            if use_exp {
                gd = gd.min(((v - floor[b] as f64 - p.thr) * (p.ratio - 1.0)).max(-p.cut));
            }
            g[i] = gd as f32;
        }
    }
    g
}

/// Misure del risultato (sul segnale mono L+R/2, finestre da 50 ms).
#[derive(Clone, Copy, Debug, Default)]
pub struct Metrics {
    /// Energia tolta rispetto all'originale (dB).
    pub removed_db: f64,
    /// Variazione media di livello nei passaggi forti dello stem (dB).
    pub sing_mean: f64,
    /// Variazione nell'1% peggiore dei passaggi forti (dB).
    pub sing_worst: f64,
}

pub fn metrics(l: &[f32], r: &[f32], ol: &[f32], or: &[f32], sr: f64) -> Metrics {
    let len = l.len();
    let w = js_round(0.05 * sr) as usize;
    let n = len / w.max(1);
    if n == 0 {
        return Metrics::default();
    }
    let mut lv = vec![0f32; n];
    let mut ly = vec![0f32; n];
    let (mut ev, mut er) = (0.0f64, 0.0f64);
    for f in 0..n {
        let (mut a, mut b) = (0.0f64, 0.0f64);
        for i in f * w..(f + 1) * w {
            let m = (l[i] as f64 + r[i] as f64) / 2.0;
            let y = (ol[i] as f64 + or[i] as f64) / 2.0;
            a += m * m;
            b += y * y;
            er += (m - y) * (m - y);
        }
        ev += a;
        lv[f] = (10.0 * (a / w as f64 + 1e-12).log10()) as f32;
        ly[f] = (10.0 * (b / w as f64 + 1e-12).log10()) as f32;
    }
    let p95 = percentile_f32(&lv, 0.95) as f64;
    let mut diffs: Vec<f64> = (0..n)
        .filter(|&f| (lv[f] as f64) > p95 - 12.0)
        .map(|f| ly[f] as f64 - lv[f] as f64)
        .collect();
    diffs.sort_by(|a, b| a.total_cmp(b));
    let mean = diffs.iter().sum::<f64>() / diffs.len().max(1) as f64;
    Metrics {
        removed_db: 10.0 * (er / ev.max(1e-20) + 1e-20).log10(),
        sing_mean: mean,
        sing_worst: if diffs.is_empty() {
            0.0
        } else {
            diffs[(0.01 * (diffs.len() - 1) as f64).floor() as usize]
        },
    }
}
