//! Lettura e scrittura di file WAV (PCM 16/24/32 bit intero, 32/64 bit float).

use crate::dsp::js_round;

/// Profondità del WAV in uscita.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bits {
    /// PCM intero a 24 bit.
    Int24,
    /// Virgola mobile a 32 bit.
    Float32,
}

impl Bits {
    pub fn from_number(n: u32) -> Option<Self> {
        match n {
            24 => Some(Self::Int24),
            32 => Some(Self::Float32),
            _ => None,
        }
    }
}

/// WAV stereo interleaved, con intestazione standard da 44 byte.
pub fn encode(l: &[f32], r: &[f32], sr: u32, bits: Bits) -> Vec<u8> {
    let n = l.len();
    let bps: usize = match bits {
        Bits::Int24 => 3,
        Bits::Float32 => 4,
    };
    let data = n * 2 * bps;
    let mut out = Vec::with_capacity(44 + data);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&((36 + data) as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&(if bits == Bits::Float32 { 3u16 } else { 1u16 }).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&sr.to_le_bytes());
    out.extend_from_slice(&(sr * 2 * bps as u32).to_le_bytes());
    out.extend_from_slice(&((2 * bps) as u16).to_le_bytes());
    out.extend_from_slice(&((bps * 8) as u16).to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data as u32).to_le_bytes());
    match bits {
        Bits::Float32 => {
            for i in 0..n {
                out.extend_from_slice(&l[i].to_le_bytes());
                out.extend_from_slice(&r[i].to_le_bytes());
            }
        }
        Bits::Int24 => {
            for i in 0..n {
                for x in [l[i], r[i]] {
                    let s = js_round((x as f64).clamp(-1.0, 1.0) * 8_388_607.0) as i32;
                    out.extend_from_slice(&s.to_le_bytes()[..3]);
                }
            }
        }
    }
    out
}

/// Audio letto da un WAV: canali separati in f32, al massimo i primi due.
pub struct Decoded {
    pub sample_rate: u32,
    pub left: Vec<f32>,
    pub right: Vec<f32>,
    pub channels: u16,
}

fn u16_at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

/// Legge un WAV in memoria. Un file mono diventa stereo con i due canali uguali.
pub fn decode(bytes: &[u8]) -> Result<Decoded, String> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err("non è un file WAV".into());
    }
    let mut pos = 12;
    let mut fmt: Option<(u16, u16, u32, u16)> = None;
    let mut data: Option<&[u8]> = None;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = u32_at(bytes, pos + 4) as usize;
        let body_end = (pos + 8 + size).min(bytes.len());
        let body = &bytes[pos + 8..body_end];
        match id {
            b"fmt " if body.len() >= 16 => {
                let mut tag = u16_at(body, 0);
                if tag == 0xFFFE && body.len() >= 26 {
                    tag = u16_at(body, 24); // WAVE_FORMAT_EXTENSIBLE: il formato vero è nel sottotipo
                }
                fmt = Some((tag, u16_at(body, 2), u32_at(body, 4), u16_at(body, 14)));
            }
            b"data" => data = Some(body),
            _ => {}
        }
        pos += 8 + size + (size & 1);
    }
    let (tag, channels, sample_rate, bits) = fmt.ok_or("manca il blocco fmt")?;
    let data = data.ok_or("manca il blocco data")?;
    if channels == 0 {
        return Err("numero di canali non valido".into());
    }
    let bps = (bits as usize).div_ceil(8);
    let frame = bps * channels as usize;
    let frames = data.len() / frame;
    let read: Box<dyn Fn(&[u8]) -> f32> = match (tag, bits) {
        (1, 16) => Box::new(|s| i16::from_le_bytes([s[0], s[1]]) as f32 / 32768.0),
        (1, 24) => {
            Box::new(|s| (i32::from_le_bytes([0, s[0], s[1], s[2]]) >> 8) as f32 / 8_388_608.0)
        }
        (1, 32) => {
            Box::new(|s| i32::from_le_bytes([s[0], s[1], s[2], s[3]]) as f32 / 2_147_483_648.0)
        }
        (1, 8) => Box::new(|s| (s[0] as f32 - 128.0) / 128.0),
        (3, 32) => Box::new(|s| f32::from_le_bytes([s[0], s[1], s[2], s[3]])),
        (3, 64) => Box::new(|s| f64::from_le_bytes(s[..8].try_into().unwrap()) as f32),
        _ => {
            return Err(format!(
                "formato WAV non supportato (codice {tag}, {bits} bit)"
            ));
        }
    };
    let mut left = Vec::with_capacity(frames);
    let mut right = Vec::with_capacity(frames);
    for f in 0..frames {
        let o = f * frame;
        let a = read(&data[o..o + bps]);
        let b = if channels > 1 {
            read(&data[o + bps..o + 2 * bps])
        } else {
            a
        };
        left.push(a);
        right.push(b);
    }
    Ok(Decoded {
        sample_rate,
        left,
        right,
        channels,
    })
}
