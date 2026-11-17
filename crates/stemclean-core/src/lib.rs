//! # StemClean DNR — motore in Rust
//!
//! Toglie da uno stem separato i rientri degli altri strumenti, banda per banda
//! (1/3 d'ottava), con un principio simile ai riduttori di rumore Dolby/DNR:
//! dove lo stem è coperto dai riferimenti (o è vicino al suo fondo) la banda
//! viene attenuata, altrimenti resta intatta.
//!
//! Lo stesso codice gira in tre posti:
//! - nella pagina web, compilato in WebAssembly (`stemclean-wasm`);
//! - nell'app per Mac fatta con Tauri, che usa la stessa pagina;
//! - nel programma da Terminale `stemclean` (`stemclean-cli`), in codice nativo.

pub mod dsp;
pub mod engine;
pub mod fft;
pub mod view;
pub mod wav;
pub mod zip;

pub use dsp::{Bands, Metrics, Mode, Params};
pub use engine::{Engine, Source};

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(len: usize, sr: f64, f: f64, amp: f32) -> Vec<f32> {
        (0..len)
            .map(|i| amp * (2.0 * std::f64::consts::PI * f * i as f64 / sr).sin() as f32)
            .collect()
    }

    #[test]
    fn fft_round_trip() {
        let fft = fft::Fft::new();
        let mut re: Vec<f64> = (0..fft::N)
            .map(|i| ((i * 7919) % 101) as f64 / 50.0 - 1.0)
            .collect();
        let orig = re.clone();
        let mut im = vec![0.0; fft::N];
        fft.run(&mut re, &mut im, false);
        fft.run(&mut re, &mut im, true);
        let err = re
            .iter()
            .zip(&orig)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f64::max);
        assert!(err < 1e-12, "errore {err}");
    }

    #[test]
    fn zero_gain_is_transparent() {
        let sr = 44100.0;
        let l = tone(30000, sr, 440.0, 0.5);
        let r = tone(30000, sr, 660.0, 0.3);
        let mut e = Engine::with_stem(sr, &l, &r);
        let mut p = Params::preset("consigliato").unwrap();
        p.cut = 0.0; // attenuazione massima nulla: il segnale deve uscire identico
        e.process(&p, |_, _| {});
        let err = e
            .out_l
            .iter()
            .zip(&l)
            .chain(e.out_r.iter().zip(&r))
            .map(|(a, b)| (a - b).abs())
            .fold(0f32, f32::max);
        assert!(err < 1e-6, "errore {err}");
    }

    #[test]
    fn sidechain_removes_bleed() {
        let sr = 44100.0;
        let n = 88200;
        // stem: voce a 440 Hz solo nella prima metà, più un rientro a 3 kHz per tutto il file
        let voice: Vec<f32> = tone(n, sr, 440.0, 0.5)
            .iter()
            .enumerate()
            .map(|(i, v)| if i < n / 2 { *v } else { 0.0 })
            .collect();
        let bleed = tone(n, sr, 3000.0, 0.05);
        let stem: Vec<f32> = voice.iter().zip(&bleed).map(|(a, b)| a + b).collect();
        let reference = tone(n, sr, 3000.0, 0.5);
        let mut e = Engine::with_stem(sr, &stem, &stem);
        e.add_reference(&reference, &reference);
        e.process(&Params::preset("consigliato").unwrap(), |_, _| {});
        assert!(
            e.metrics.removed_db < -15.0,
            "tolto solo {} dB",
            e.metrics.removed_db
        );
        assert!(
            e.metrics.sing_mean > -1.0,
            "voce attenuata di {} dB",
            e.metrics.sing_mean
        );
    }

    #[test]
    fn wav_round_trip() {
        let l = tone(1000, 48000.0, 1000.0, 0.9);
        let r = tone(1000, 48000.0, 500.0, 0.4);
        for bits in [wav::Bits::Int24, wav::Bits::Float32] {
            let d = wav::decode(&wav::encode(&l, &r, 48000, bits)).unwrap();
            assert_eq!(d.sample_rate, 48000);
            let err = d
                .left
                .iter()
                .zip(&l)
                .chain(d.right.iter().zip(&r))
                .map(|(a, b)| (a - b).abs())
                .fold(0f32, f32::max);
            assert!(err < 3e-7, "{bits:?}: errore {err}");
        }
    }

    #[test]
    fn zip_crc_known_value() {
        assert_eq!(zip::crc32(b"123456789"), 0xCBF4_3926);
    }
}
