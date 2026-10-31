//! Content hashes: SHA-256 for identities that leave the process, FNV-1a for in-memory
//! subtree hashing where speed matters and collisions only cost a closer look.
use sha2::{Digest, Sha256};

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("{:x}", h.finalize())
}

pub const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

#[derive(Clone, Copy)]
pub struct Fnv(pub u64);
impl Default for Fnv {
    fn default() -> Self {
        Fnv(FNV_OFFSET)
    }
}
impl Fnv {
    pub fn bytes(&mut self, b: &[u8]) -> &mut Self {
        for &x in b {
            self.0 ^= x as u64;
            self.0 = self.0.wrapping_mul(FNV_PRIME);
        }
        self
    }
    pub fn str(&mut self, s: &str) -> &mut Self {
        self.bytes(s.as_bytes()).bytes(&[0xff])
    }
    pub fn u64(&mut self, v: u64) -> &mut Self {
        self.bytes(&v.to_le_bytes())
    }
    pub fn i32(&mut self, v: i32) -> &mut Self {
        self.bytes(&v.to_le_bytes())
    }
    pub fn finish(&self) -> u64 {
        // Final avalanche so that small inputs spread across all bits.
        let mut x = self.0;
        x ^= x >> 33;
        x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
        x ^= x >> 33;
        x = x.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
        x ^= x >> 33;
        x
    }
}
pub fn fnv_bytes(b: &[u8]) -> u64 {
    Fnv::default().bytes(b).finish()
}
