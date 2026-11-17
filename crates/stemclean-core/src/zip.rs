//! Archivio ZIP senza compressione (serve solo per consegnare più WAV in un file).

fn crc_table() -> [u32; 256] {
    let mut t = [0u32; 256];
    for (n, slot) in t.iter_mut().enumerate() {
        let mut c = n as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
        *slot = c;
    }
    t
}

pub fn crc32(data: &[u8]) -> u32 {
    let t = crc_table();
    let mut c = 0xFFFF_FFFFu32;
    for &b in data {
        c = t[((c ^ b as u32) & 255) as usize] ^ (c >> 8);
    }
    c ^ 0xFFFF_FFFF
}

#[derive(Default)]
pub struct ZipWriter {
    out: Vec<u8>,
    central: Vec<u8>,
    count: u16,
}

impl ZipWriter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Aggiunge un file (nome in UTF-8).
    pub fn add(&mut self, name: &str, data: &[u8]) {
        let name = name.as_bytes();
        let crc = crc32(data);
        let size = data.len() as u32;
        let offset = self.out.len() as u32;
        // intestazione locale
        let o = &mut self.out;
        o.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        o.extend_from_slice(&20u16.to_le_bytes());
        o.extend_from_slice(&0x0800u16.to_le_bytes()); // nomi in UTF-8
        o.extend_from_slice(&0u16.to_le_bytes()); // nessuna compressione
        o.extend_from_slice(&0u16.to_le_bytes()); // ora
        o.extend_from_slice(&33u16.to_le_bytes()); // data: 1 gennaio 1980
        o.extend_from_slice(&crc.to_le_bytes());
        o.extend_from_slice(&size.to_le_bytes());
        o.extend_from_slice(&size.to_le_bytes());
        o.extend_from_slice(&(name.len() as u16).to_le_bytes());
        o.extend_from_slice(&0u16.to_le_bytes());
        o.extend_from_slice(name);
        o.extend_from_slice(data);
        // voce della directory centrale
        let c = &mut self.central;
        c.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        c.extend_from_slice(&20u16.to_le_bytes());
        c.extend_from_slice(&20u16.to_le_bytes());
        c.extend_from_slice(&0x0800u16.to_le_bytes());
        c.extend_from_slice(&0u16.to_le_bytes());
        c.extend_from_slice(&0u16.to_le_bytes());
        c.extend_from_slice(&33u16.to_le_bytes());
        c.extend_from_slice(&crc.to_le_bytes());
        c.extend_from_slice(&size.to_le_bytes());
        c.extend_from_slice(&size.to_le_bytes());
        c.extend_from_slice(&(name.len() as u16).to_le_bytes());
        c.extend_from_slice(&[0u8; 12]); // extra, commento, disco, attributi interni ed esterni
        c.extend_from_slice(&offset.to_le_bytes());
        c.extend_from_slice(name);
        self.count += 1;
    }

    pub fn finish(mut self) -> Vec<u8> {
        let cd_offset = self.out.len() as u32;
        let cd_size = self.central.len() as u32;
        self.out.extend_from_slice(&self.central);
        self.out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        self.out.extend_from_slice(&[0u8; 4]);
        self.out.extend_from_slice(&self.count.to_le_bytes());
        self.out.extend_from_slice(&self.count.to_le_bytes());
        self.out.extend_from_slice(&cd_size.to_le_bytes());
        self.out.extend_from_slice(&cd_offset.to_le_bytes());
        self.out.extend_from_slice(&0u16.to_le_bytes());
        self.out
    }
}
