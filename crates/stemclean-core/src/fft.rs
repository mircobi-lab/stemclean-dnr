//! FFT radix-2 complessa a 2048 punti, in f64.
//!
//! È la stessa FFT della versione JavaScript di StemClean, riga per riga:
//! così i due motori danno risultati identici fino all'arrotondamento.

/// Lunghezza della finestra di analisi (campioni).
pub const N: usize = 2048;
/// Passo tra una finestra e la successiva (campioni): sovrapposizione del 75%.
pub const H: usize = 512;
/// Campioni di anticipo della prima finestra, così l'inizio del file è coperto per intero.
pub const OFF: usize = N - H;
/// Numero di bin di frequenza utili (da 0 Hz a Nyquist).
pub const K: usize = N / 2 + 1;

pub struct Fft {
    rev: Vec<u32>,
    cs: Vec<f64>,
    sn: Vec<f64>,
}

impl Default for Fft {
    fn default() -> Self {
        Self::new()
    }
}

impl Fft {
    pub fn new() -> Self {
        let levels = N.trailing_zeros();
        let rev = (0..N as u32)
            .map(|i| {
                let mut r = 0u32;
                let mut x = i;
                for _ in 0..levels {
                    r = (r << 1) | (x & 1);
                    x >>= 1;
                }
                r
            })
            .collect();
        let step = 2.0 * std::f64::consts::PI / N as f64;
        let cs = (0..N / 2).map(|i| (step * i as f64).cos()).collect();
        let sn = (0..N / 2).map(|i| (step * i as f64).sin()).collect();
        Self { rev, cs, sn }
    }

    /// Trasformata sul posto. `inverse = true` divide anche per N.
    pub fn run(&self, re: &mut [f64], im: &mut [f64], inverse: bool) {
        debug_assert!(re.len() == N && im.len() == N);
        for i in 0..N {
            let j = self.rev[i] as usize;
            if j > i {
                re.swap(i, j);
                im.swap(i, j);
            }
        }
        let sign = if inverse { 1.0 } else { -1.0 };
        let mut size = 2;
        while size <= N {
            let half = size >> 1;
            let step = N / size;
            for i in (0..N).step_by(size) {
                let mut k = 0;
                for j in 0..half {
                    let a = i + j;
                    let b = a + half;
                    let c = self.cs[k];
                    let s = sign * self.sn[k];
                    let tr = re[b] * c - im[b] * s;
                    let ti = re[b] * s + im[b] * c;
                    re[b] = re[a] - tr;
                    im[b] = im[a] - ti;
                    re[a] += tr;
                    im[a] += ti;
                    k += step;
                }
            }
            size <<= 1;
        }
        if inverse {
            let n = N as f64;
            for v in re.iter_mut() {
                *v /= n;
            }
            for v in im.iter_mut() {
                *v /= n;
            }
        }
    }
}

/// Finestra di Hann periodica.
pub fn hann() -> Vec<f64> {
    (0..N)
        .map(|n| 0.5 - 0.5 * (2.0 * std::f64::consts::PI * n as f64 / N as f64).cos())
        .collect()
}
