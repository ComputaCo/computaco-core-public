//! The bytes calls cross a module boundary in: little-endian integers, lengths as
//! `u32`, strings as UTF-8. Small, so a module carries no serializer for it.

#[derive(Default)]
pub struct Writer(pub Vec<u8>);
impl Writer {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn u8(&mut self, v: u8) -> &mut Self {
        self.0.push(v);
        self
    }
    pub fn bool(&mut self, v: bool) -> &mut Self {
        self.u8(u8::from(v))
    }
    pub fn u16(&mut self, v: u16) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    pub fn u32(&mut self, v: u32) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    pub fn i32(&mut self, v: i32) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    pub fn u64(&mut self, v: u64) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    pub fn i64(&mut self, v: i64) -> &mut Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    pub fn bytes(&mut self, v: &[u8]) -> &mut Self {
        self.u32(v.len() as u32);
        self.0.extend_from_slice(v);
        self
    }
    pub fn str(&mut self, v: &str) -> &mut Self {
        self.bytes(v.as_bytes())
    }
    pub fn opt_str(&mut self, v: Option<&str>) -> &mut Self {
        match v {
            Some(s) => self.u8(1).str(s),
            None => self.u8(0),
        }
    }
    pub fn strs(&mut self, v: &[String]) -> &mut Self {
        self.u32(v.len() as u32);
        v.iter().for_each(|s| {
            self.str(s);
        });
        self
    }
    pub fn pairs(&mut self, v: &[(String, String)]) -> &mut Self {
        self.u32(v.len() as u32);
        v.iter().for_each(|(a, b)| {
            self.str(a).str(b);
        });
        self
    }
    pub fn finish(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.0)
    }
}

/// Reads what a [`Writer`] wrote. A short or malformed buffer reads as zeros and empty
/// strings: both sides are one build's code, so it never happens between them.
pub struct Reader<'a>(&'a [u8]);
impl<'a> Reader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self(bytes)
    }
    fn take(&mut self, n: usize) -> &'a [u8] {
        let n = n.min(self.0.len());
        let (head, rest) = self.0.split_at(n);
        self.0 = rest;
        head
    }
    fn array<const N: usize>(&mut self) -> [u8; N] {
        let mut out = [0; N];
        let head = self.take(N);
        out[..head.len()].copy_from_slice(head);
        out
    }
    pub fn u8(&mut self) -> u8 {
        self.array::<1>()[0]
    }
    pub fn bool(&mut self) -> bool {
        self.u8() != 0
    }
    pub fn u16(&mut self) -> u16 {
        u16::from_le_bytes(self.array())
    }
    pub fn u32(&mut self) -> u32 {
        u32::from_le_bytes(self.array())
    }
    pub fn i32(&mut self) -> i32 {
        i32::from_le_bytes(self.array())
    }
    pub fn u64(&mut self) -> u64 {
        u64::from_le_bytes(self.array())
    }
    pub fn i64(&mut self) -> i64 {
        i64::from_le_bytes(self.array())
    }
    pub fn bytes(&mut self) -> Vec<u8> {
        let n = self.u32() as usize;
        self.take(n).to_vec()
    }
    pub fn str(&mut self) -> String {
        String::from_utf8_lossy(&self.bytes()).into_owned()
    }
    pub fn opt_str(&mut self) -> Option<String> {
        self.bool().then(|| self.str())
    }
    pub fn strs(&mut self) -> Vec<String> {
        let n = self.u32();
        (0..n).map(|_| self.str()).collect()
    }
    pub fn pairs(&mut self) -> Vec<(String, String)> {
        let n = self.u32();
        (0..n).map(|_| (self.str(), self.str())).collect()
    }
}
