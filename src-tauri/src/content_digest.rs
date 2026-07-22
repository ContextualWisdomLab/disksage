use base64::Engine;
use sha2::Digest;
use std::fmt::Write;

const QUICK_XOR_WIDTH_BITS: usize = 160;
const QUICK_XOR_SHIFT: usize = 11;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ContentDigests {
    pub blake3: String,
    pub sha256: String,
    pub quick_xor_base64: String,
}

#[derive(Default)]
struct QuickXorHasher {
    cells: [u64; 3],
    length: u64,
    shift: usize,
}

impl QuickXorHasher {
    fn update(&mut self, bytes: &[u8]) {
        let mut cell_index = self.shift / 64;
        let mut cell_offset = self.shift % 64;
        let iterations = bytes.len().min(QUICK_XOR_WIDTH_BITS);

        for index in 0..iterations {
            let is_last_cell = cell_index == self.cells.len() - 1;
            let bits_in_cell = if is_last_cell { 32 } else { 64 };
            let xored_byte = bytes[index..]
                .iter()
                .step_by(QUICK_XOR_WIDTH_BITS)
                .fold(0_u8, |value, byte| value ^ byte);

            if cell_offset <= bits_in_cell - 8 {
                self.cells[cell_index] ^= u64::from(xored_byte) << cell_offset;
            } else {
                let next_cell = if is_last_cell { 0 } else { cell_index + 1 };
                let low_bits = bits_in_cell - cell_offset;
                self.cells[cell_index] ^= u64::from(xored_byte) << cell_offset;
                self.cells[next_cell] ^= u64::from(xored_byte) >> low_bits;
            }

            cell_offset += QUICK_XOR_SHIFT;
            while cell_offset >= bits_in_cell {
                cell_index = if is_last_cell { 0 } else { cell_index + 1 };
                cell_offset -= bits_in_cell;
            }
        }

        self.shift = (self.shift + QUICK_XOR_SHIFT * (bytes.len() % QUICK_XOR_WIDTH_BITS))
            % QUICK_XOR_WIDTH_BITS;
        self.length = self.length.wrapping_add(bytes.len() as u64);
    }

    fn finalize(self) -> [u8; 20] {
        let mut output = [0_u8; 20];
        output[..8].copy_from_slice(&self.cells[0].to_le_bytes());
        output[8..16].copy_from_slice(&self.cells[1].to_le_bytes());
        output[16..20].copy_from_slice(&self.cells[2].to_le_bytes()[..4]);
        for (output_byte, length_byte) in output[12..].iter_mut().zip(self.length.to_le_bytes()) {
            *output_byte ^= length_byte;
        }
        output
    }
}

pub struct ContentHasher {
    blake3: blake3::Hasher,
    sha256: sha2::Sha256,
    quick_xor: QuickXorHasher,
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}

impl Default for ContentHasher {
    fn default() -> Self {
        Self {
            blake3: blake3::Hasher::new(),
            sha256: sha2::Sha256::new(),
            quick_xor: QuickXorHasher::default(),
        }
    }
}

impl ContentHasher {
    pub fn update(&mut self, bytes: &[u8]) {
        self.blake3.update(bytes);
        self.sha256.update(bytes);
        self.quick_xor.update(bytes);
    }

    pub fn finalize(self) -> ContentDigests {
        let sha256 = self.sha256.finalize();
        ContentDigests {
            blake3: self.blake3.finalize().to_hex().to_string(),
            sha256: hex_lower(&sha256),
            quick_xor_base64: base64::engine::general_purpose::STANDARD
                .encode(self.quick_xor.finalize()),
        }
    }
}

pub fn digest_bytes(bytes: &[u8]) -> ContentDigests {
    let mut hasher = ContentHasher::default();
    hasher.update(bytes);
    hasher.finalize()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quick_xor_matches_microsoft_reference_vectors() {
        let sequence: Vec<u8> = (0..=255).map(|value| value as u8).collect();
        let repeated: Vec<u8> = (0..600).map(|value| (value % 251) as u8).collect();
        for (input, expected) in [
            (&b""[..], "AAAAAAAAAAAAAAAAAAAAAAAAAAA="),
            (&b"a"[..], "YQAAAAAAAAAAAAAAAQAAAAAAAAA="),
            (&b"hello"[..], "aCgDG9jwBgAAAAAABQAAAAAAAAA="),
            (&sequence[..], "QkGEfSisZcA7k+FCh71r2dbCayY="),
            (&repeated[..], "z0rVUrguclftJj1osaWH0yosV44="),
        ] {
            assert_eq!(digest_bytes(input).quick_xor_base64, expected);
        }
    }

    #[test]
    fn streaming_chunks_match_single_update_across_cell_boundaries() {
        let input: Vec<u8> = (0..977).map(|value| (value % 253) as u8).collect();
        let expected = digest_bytes(&input);
        let mut streamed = ContentHasher::default();
        for chunk in input.chunks(73) {
            streamed.update(chunk);
        }
        assert_eq!(streamed.finalize(), expected);
    }

    #[test]
    fn cryptographic_digests_match_known_hello_values() {
        let digests = digest_bytes(b"hello");
        assert_eq!(
            digests.sha256,
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
        assert_eq!(digests.blake3, blake3::hash(b"hello").to_hex().to_string());
    }
}
