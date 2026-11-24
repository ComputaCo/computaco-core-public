//! The portable snapshot format: a world's whole state as bytes another build reads back.
//!
//! Each type writes itself with [`Encode`] and reads itself with [`Decode`], written by
//! hand for the standard types here and derived (`cw_snapshot_derive`) for the
//! engine's own. Neither is generic over its output, so a type has one encoder and one
//! decoder in a build, and none of serde's.
//!
//! The bytes:
//!
//! - an unsigned integer is LEB128 (a `u8` is its byte), a signed one LEB128 of its
//!   zigzag; a `usize` is a `u64`, so no width of the writing platform reaches them;
//! - a float is its IEEE bits, little-endian, so NaN payloads and -0.0 survive;
//! - a `bool` is 0 or 1, an `Option` 0 (none) or 1 and the value, a `Result` 0 (ok) or
//!   1 (error) and the value;
//! - a string or byte string is its length and its bytes, a sequence its length and
//!   its elements, a map its length and its entries in ascending key order;
//! - a struct is its fields in declaration order; an enum its variant's number, or
//!   its name where builds differ in variants, then the variant's fields;
//! - a JSON value is a tag and its content (an application holds its state as the
//!   JSON it wrote, and a service holds JSON).
//!
//! There is no version and no header: one build's format is the format. A reader
//! refuses what a writer could not have written: a truncated or trailing input, a
//! length longer than the bytes left, a key out of order, a tag or a number out of
//! range, a string that is not UTF-8, or nesting deeper than [`MAX_DEPTH`]. Every
//! failure is an [`Error`] naming the offset; nothing in a reader panics on its input.
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;
use std::rc::Rc;
use std::sync::Arc;

pub use cw_snapshot_derive::{Decode, Encode};
// The derives name `::cw_snapshot`, which this crate's own tests use too.
extern crate self as cw_snapshot;

/// How deeply containers may nest in an input. A reader recurses once per level, so
/// this is what keeps a hostile input from overflowing the stack.
pub const MAX_DEPTH: u32 = 256;

/// Why an input is not a snapshot, and where in it reading stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    pub offset: usize,
    pub message: String,
}
impl Error {
    pub fn at(offset: usize, message: impl Into<String>) -> Self {
        Self {
            offset,
            message: message.into(),
        }
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid snapshot at byte {}: {}",
            self.offset, self.message
        )
    }
}
impl std::error::Error for Error {}

/// Where encoded bytes go.
#[derive(Default)]
pub struct Writer {
    buf: Vec<u8>,
    /// The first value that could not be written (an application state JSON cannot
    /// hold); `finish` reports it.
    failed: Option<String>,
}
impl Writer {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            buf: Vec::with_capacity(capacity),
            failed: None,
        }
    }
    /// Record that a value could not be written; the bytes are then not a snapshot.
    pub fn fail(&mut self, message: impl Into<String>) {
        self.failed.get_or_insert_with(|| message.into());
    }
    /// The bytes, or why some value could not be written.
    pub fn finish(self) -> Result<Vec<u8>, Error> {
        match self.failed {
            None => Ok(self.buf),
            Some(message) => Err(Error::at(self.buf.len(), message)),
        }
    }
    pub fn len(&self) -> usize {
        self.buf.len()
    }
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }
    #[inline]
    pub fn byte(&mut self, b: u8) {
        self.buf.push(b);
    }
    #[inline]
    pub fn raw(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }
    #[inline]
    pub fn leb(&mut self, mut v: u64) {
        while v >= 0x80 {
            self.buf.push((v as u8) | 0x80);
            v >>= 7;
        }
        self.buf.push(v as u8);
    }
    #[inline]
    pub fn sleb(&mut self, v: i64) {
        self.leb(((v << 1) ^ (v >> 63)) as u64);
    }
    #[inline]
    pub fn len_prefix(&mut self, len: usize) {
        self.leb(len as u64);
    }
    #[inline]
    pub fn bytes(&mut self, bytes: &[u8]) {
        self.len_prefix(bytes.len());
        self.raw(bytes);
    }
    #[inline]
    pub fn str(&mut self, s: &str) {
        self.bytes(s.as_bytes());
    }
}

/// Bytes being decoded.
pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
    depth: u32,
}
impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            depth: 0,
        }
    }
    pub fn position(&self) -> usize {
        self.pos
    }
    pub fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }
    pub fn error(&self, message: impl Into<String>) -> Error {
        Error::at(self.pos, message)
    }
    /// Refuse input left after the value.
    pub fn finish(&self) -> Result<(), Error> {
        if self.pos == self.data.len() {
            Ok(())
        } else {
            Err(self.error(format!("{} bytes after the end", self.remaining())))
        }
    }
    #[inline]
    pub fn byte(&mut self) -> Result<u8, Error> {
        let b = *self
            .data
            .get(self.pos)
            .ok_or_else(|| self.error("input ends early"))?;
        self.pos += 1;
        Ok(b)
    }
    #[inline]
    pub fn raw(&mut self, n: usize) -> Result<&'a [u8], Error> {
        if n > self.remaining() {
            return Err(self.error(format!("{n} bytes wanted, {} left", self.remaining())));
        }
        let out = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(out)
    }
    #[inline]
    pub fn leb(&mut self) -> Result<u64, Error> {
        let start = self.pos;
        let mut out = 0u64;
        let mut shift = 0;
        loop {
            let b = self.byte()?;
            if shift == 63 && b > 1 {
                return Err(Error::at(start, "number beyond 64 bits"));
            }
            out |= u64::from(b & 0x7f) << shift;
            if b & 0x80 == 0 {
                // One encoding per number: no trailing zero groups.
                if b == 0 && shift > 0 {
                    return Err(Error::at(start, "number not in its shortest form"));
                }
                return Ok(out);
            }
            shift += 7;
        }
    }
    #[inline]
    pub fn sleb(&mut self) -> Result<i64, Error> {
        let v = self.leb()?;
        Ok(((v >> 1) as i64) ^ -((v & 1) as i64))
    }
    /// A length of things each at least one byte long, so at most what is left.
    #[inline]
    pub fn len_prefix(&mut self) -> Result<usize, Error> {
        let at = self.pos;
        let n = self.leb()?;
        if n > self.remaining() as u64 {
            return Err(Error::at(
                at,
                format!("length {n} beyond the {} bytes left", self.remaining()),
            ));
        }
        Ok(n as usize)
    }
    #[inline]
    pub fn bytes(&mut self) -> Result<&'a [u8], Error> {
        let n = self.len_prefix()?;
        self.raw(n)
    }
    #[inline]
    pub fn str(&mut self) -> Result<&'a str, Error> {
        let at = self.pos;
        let bytes = self.bytes()?;
        std::str::from_utf8(bytes).map_err(|e| Error::at(at, format!("text is not UTF-8: {e}")))
    }
    /// Run `f` one level deeper.
    #[inline]
    pub fn nested<T>(&mut self, f: impl FnOnce(&mut Self) -> Result<T, Error>) -> Result<T, Error> {
        if self.depth >= MAX_DEPTH {
            return Err(self.error("nested too deeply"));
        }
        self.depth += 1;
        let out = f(self);
        self.depth -= 1;
        out
    }
}

pub trait Encode {
    fn encode(&self, w: &mut Writer);
    /// A sequence of these: its length, then each. Bytes are copied whole.
    #[doc(hidden)]
    fn encode_slice(items: &[Self], w: &mut Writer)
    where
        Self: Sized,
    {
        w.len_prefix(items.len());
        for item in items {
            item.encode(w);
        }
    }
}
pub trait Decode: Sized {
    fn decode(r: &mut Reader<'_>) -> Result<Self, Error>;
    /// `n` of these, the length already read. Bytes are copied whole.
    #[doc(hidden)]
    fn decode_vec(r: &mut Reader<'_>, n: usize) -> Result<Vec<Self>, Error> {
        r.nested(|r| {
            let mut out = Vec::with_capacity(n);
            for _ in 0..n {
                out.push(Self::decode(r)?);
            }
            Ok(out)
        })
    }
}

/// `value`'s bytes, or why some part of it could not be written.
pub fn to_bytes<T: Encode + ?Sized>(value: &T) -> Result<Vec<u8>, Error> {
    let mut w = Writer::new();
    value.encode(&mut w);
    w.finish()
}
/// The value `bytes` hold, all of them.
pub fn from_bytes<T: Decode>(bytes: &[u8]) -> Result<T, Error> {
    let mut r = Reader::new(bytes);
    let value = T::decode(&mut r)?;
    r.finish()?;
    Ok(value)
}

macro_rules! unsigned {
    ($($t:ty),*) => {$(
        impl Encode for $t {
            #[inline]
            fn encode(&self, w: &mut Writer) {
                w.leb(*self as u64);
            }
        }
        impl Decode for $t {
            #[inline]
            fn decode(r: &mut Reader<'_>) -> Result<Self, Error> {
                let at = r.position();
                let v = r.leb()?;
                <$t>::try_from(v).map_err(|_| Error::at(at, format!("{v} is out of range for {}", stringify!($t))))
            }
        }
    )*};
}
unsigned!(u16, u32, u64, usize);
macro_rules! signed {
    ($($t:ty),*) => {$(
        impl Encode for $t {
            #[inline]
            fn encode(&self, w: &mut Writer) {
                w.sleb(*self as i64);
            }
        }
        impl Decode for $t {
            #[inline]
            fn decode(r: &mut Reader<'_>) -> Result<Self, Error> {
                let at = r.position();
                let v = r.sleb()?;
                <$t>::try_from(v).map_err(|_| Error::at(at, format!("{v} is out of range for {}", stringify!($t))))
            }
        }
    )*};
}
signed!(i8, i16, i32, i64, isize);
impl Encode for u8 {
    #[inline]
    fn encode(&self, w: &mut Writer) {
        w.byte(*self);
    }
    fn encode_slice(items: &[Self], w: &mut Writer) {
        w.bytes(items);
    }
}
impl Decode for u8 {
    #[inline]
    fn decode(r: &mut Reader<'_>) -> Result<Self, Error> {
        r.byte()
    }
    fn decode_vec(r: &mut Reader<'_>, n: usize) -> Result<Vec<Self>, Error> {
        r.raw(n).map(<[u8]>::to_vec)
    }
}
impl Encode for u128 {
    fn encode(&self, w: &mut Writer) {
        w.raw(&self.to_le_bytes());
    }
}
impl Decode for u128 {
    fn decode(r: &mut Reader<'_>) -> Result<Self, Error> {
        Ok(u128::from_le_bytes(
            r.raw(16)?.try_into().expect("16 bytes"),
        ))
    }
}
impl Encode for i128 {
    fn encode(&self, w: &mut Writer) {
        w.raw(&self.to_le_bytes());
    }
}
impl Decode for i128 {
    fn decode(r: &mut Reader<'_>) -> Result<Self, Error> {
        Ok(i128::from_le_bytes(
            r.raw(16)?.try_into().expect("16 bytes"),
        ))
    }
}
impl Encode for f32 {
    #[inline]
    fn encode(&self, w: &mut Writer) {
        w.raw(&self.to_bits().to_le_bytes());
    }
}
impl Decode for f32 {
    #[inline]
    fn decode(r: &mut Reader<'_>) -> Result<Self, Error> {
        Ok(f32::from_bits(u32::from_le_bytes(
            r.raw(4)?.try_into().expect("4 bytes"),
        )))
    }
}
impl Encode for f64 {
    #[inline]
    fn encode(&self, w: &mut Writer) {
        w.raw(&self.to_bits().to_le_bytes());
    }
}
impl Decode for f64 {
    #[inline]
    fn decode(r: &mut Reader<'_>) -> Result<Self, Error> {
        Ok(f64::from_bits(u64::from_le_bytes(
            r.raw(8)?.try_into().expect("8 bytes"),
        )))
    }
}
impl Encode for bool {
    #[inline]
    fn encode(&self, w: &mut Writer) {
        w.byte(u8::from(*self));
    }
}
impl Decode for bool {
    #[inline]
    fn decode(r: &mut Reader<'_>) -> Result<Self, Error> {
        match r.byte()? {
            0 => Ok(false),
            1 => Ok(true),
            b => Err(Error::at(r.position() - 1, format!("{b} is not a bool"))),
        }
    }
}
impl Encode for char {
    fn encode(&self, w: &mut Writer) {
        w.leb(u64::from(u32::from(*self)));
    }
}
impl Decode for char {
    fn decode(r: &mut Reader<'_>) -> Result<Self, Error> {
        let at = r.position();
        let v = r.leb()?;
        u32::try_from(v)
            .ok()
            .and_then(char::from_u32)
            .ok_or_else(|| Error::at(at, format!("{v} is not a character")))
    }
}
impl Encode for () {
    fn encode(&self, _: &mut Writer) {}
}
impl Decode for () {
    fn decode(_: &mut Reader<'_>) -> Result<Self, Error> {
        Ok(())
    }
}
impl Encode for str {
    #[inline]
    fn encode(&self, w: &mut Writer) {
        w.str(self);
    }
}
impl Encode for String {
    #[inline]
    fn encode(&self, w: &mut Writer) {
        w.str(self);
    }
}
impl Decode for String {
    #[inline]
    fn decode(r: &mut Reader<'_>) -> Result<Self, Error> {
        r.str().map(str::to_owned)
    }
}
impl Decode for Box<str> {
    fn decode(r: &mut Reader<'_>) -> Result<Self, Error> {
        r.str().map(Into::into)
    }
}
impl Decode for Arc<str> {
    fn decode(r: &mut Reader<'_>) -> Result<Self, Error> {
        r.str().map(Into::into)
    }
}
impl Decode for Rc<str> {
    fn decode(r: &mut Reader<'_>) -> Result<Self, Error> {
        r.str().map(Into::into)
    }
}

impl<T: Encode + ?Sized> Encode for &T {
    #[inline]
    fn encode(&self, w: &mut Writer) {
        (**self).encode(w);
    }
}
impl<T: Encode + ?Sized> Encode for Box<T> {
    #[inline]
    fn encode(&self, w: &mut Writer) {
        (**self).encode(w);
    }
}
impl<T: Decode> Decode for Box<T> {
    #[inline]
    fn decode(r: &mut Reader<'_>) -> Result<Self, Error> {
        r.nested(|r| T::decode(r).map(Box::new))
    }
}
impl<T: Encode + ?Sized> Encode for Arc<T> {
    #[inline]
    fn encode(&self, w: &mut Writer) {
        (**self).encode(w);
    }
}
impl<T: Decode> Decode for Arc<T> {
    #[inline]
    fn decode(r: &mut Reader<'_>) -> Result<Self, Error> {
        r.nested(|r| T::decode(r).map(Arc::new))
    }
}
impl<T: Encode + ?Sized> Encode for Rc<T> {
    #[inline]
    fn encode(&self, w: &mut Writer) {
        (**self).encode(w);
    }
}
impl<T: Decode> Decode for Rc<T> {
    #[inline]
    fn decode(r: &mut Reader<'_>) -> Result<Self, Error> {
        r.nested(|r| T::decode(r).map(Rc::new))
    }
}
impl<T: Encode> Encode for Option<T> {
    #[inline]
    fn encode(&self, w: &mut Writer) {
        match self {
            None => w.byte(0),
            Some(v) => {
                w.byte(1);
                v.encode(w);
            }
        }
    }
}
impl<T: Decode> Decode for Option<T> {
    #[inline]
    fn decode(r: &mut Reader<'_>) -> Result<Self, Error> {
        match r.byte()? {
            0 => Ok(None),
            1 => r.nested(T::decode).map(Some),
            b => Err(Error::at(
                r.position() - 1,
                format!("{b} is not an option tag"),
            )),
        }
    }
}
impl<T: Encode, E: Encode> Encode for Result<T, E> {
    fn encode(&self, w: &mut Writer) {
        match self {
            Ok(v) => {
                w.byte(0);
                v.encode(w);
            }
            Err(e) => {
                w.byte(1);
                e.encode(w);
            }
        }
    }
}
impl<T: Decode, E: Decode> Decode for Result<T, E> {
    fn decode(r: &mut Reader<'_>) -> Result<Self, Error> {
        match r.byte()? {
            0 => r.nested(T::decode).map(Ok),
            1 => r.nested(E::decode).map(Err),
            b => Err(Error::at(
                r.position() - 1,
                format!("{b} is not a result tag"),
            )),
        }
    }
}

fn encode_seq<'a, T: Encode + 'a>(w: &mut Writer, len: usize, items: impl Iterator<Item = &'a T>) {
    w.len_prefix(len);
    for item in items {
        item.encode(w);
    }
}
impl<T: Encode> Encode for [T] {
    #[inline]
    fn encode(&self, w: &mut Writer) {
        T::encode_slice(self, w);
    }
}
impl<T: Encode> Encode for Vec<T> {
    #[inline]
    fn encode(&self, w: &mut Writer) {
        self.as_slice().encode(w);
    }
}
impl<T: Decode> Decode for Vec<T> {
    #[inline]
    fn decode(r: &mut Reader<'_>) -> Result<Self, Error> {
        let n = r.len_prefix()?;
        T::decode_vec(r, n)
    }
}
impl<T: Decode> Decode for Box<[T]> {
    fn decode(r: &mut Reader<'_>) -> Result<Self, Error> {
        Vec::decode(r).map(Into::into)
    }
}
impl<T: Decode> Decode for Arc<[T]> {
    fn decode(r: &mut Reader<'_>) -> Result<Self, Error> {
        Vec::decode(r).map(Into::into)
    }
}
impl<T: Encode> Encode for VecDeque<T> {
    fn encode(&self, w: &mut Writer) {
        // The same bytes as a slice of these, wherever the ring is split.
        encode_seq(w, self.len(), self.iter());
    }
}
impl<T: Decode> Decode for VecDeque<T> {
    fn decode(r: &mut Reader<'_>) -> Result<Self, Error> {
        Vec::decode(r).map(Into::into)
    }
}
impl<T: Encode, const N: usize> Encode for [T; N] {
    fn encode(&self, w: &mut Writer) {
        for item in self {
            item.encode(w);
        }
    }
}
impl<T: Decode, const N: usize> Decode for [T; N] {
    fn decode(r: &mut Reader<'_>) -> Result<Self, Error> {
        let items = r.nested(|r| (0..N).map(|_| T::decode(r)).collect::<Result<Vec<_>, _>>())?;
        Ok(items
            .try_into()
            .unwrap_or_else(|_| unreachable!("N items were read")))
    }
}
impl<K: Encode, V: Encode> Encode for BTreeMap<K, V> {
    fn encode(&self, w: &mut Writer) {
        w.len_prefix(self.len());
        for (k, v) in self {
            k.encode(w);
            v.encode(w);
        }
    }
}
impl<K: Decode + Ord, V: Decode> Decode for BTreeMap<K, V> {
    fn decode(r: &mut Reader<'_>) -> Result<Self, Error> {
        let n = r.len_prefix()?;
        r.nested(|r| {
            // Ascending keys are what a map writes; reading them in that order lets the
            // map be built in one pass instead of searched once per key.
            let mut entries: Vec<(K, V)> = Vec::with_capacity(n);
            for _ in 0..n {
                let at = r.position();
                let k = K::decode(r)?;
                if entries.last().is_some_and(|(last, _)| *last >= k) {
                    return Err(Error::at(at, "map keys out of order"));
                }
                let v = V::decode(r)?;
                entries.push((k, v));
            }
            Ok(entries.into_iter().collect())
        })
    }
}
impl<T: Encode> Encode for BTreeSet<T> {
    fn encode(&self, w: &mut Writer) {
        encode_seq(w, self.len(), self.iter());
    }
}
impl<T: Decode + Ord> Decode for BTreeSet<T> {
    fn decode(r: &mut Reader<'_>) -> Result<Self, Error> {
        let n = r.len_prefix()?;
        r.nested(|r| {
            let mut items: Vec<T> = Vec::with_capacity(n);
            for _ in 0..n {
                let at = r.position();
                let v = T::decode(r)?;
                if items.last().is_some_and(|last| *last >= v) {
                    return Err(Error::at(at, "set items out of order"));
                }
                items.push(v);
            }
            Ok(items.into_iter().collect())
        })
    }
}

macro_rules! tuples {
    ($(($($n:tt $t:ident),+)),*) => {$(
        impl<$($t: Encode),+> Encode for ($($t,)+) {
            #[inline]
            fn encode(&self, w: &mut Writer) {
                $(self.$n.encode(w);)+
            }
        }
        impl<$($t: Decode),+> Decode for ($($t,)+) {
            #[inline]
            fn decode(r: &mut Reader<'_>) -> Result<Self, Error> {
                Ok(($($t::decode(r)?,)+))
            }
        }
    )*};
}
tuples!(
    (0 A),
    (0 A, 1 B),
    (0 A, 1 B, 2 C),
    (0 A, 1 B, 2 C, 3 D),
    (0 A, 1 B, 2 C, 3 D, 4 E),
    (0 A, 1 B, 2 C, 3 D, 4 E, 5 F)
);

/// JSON values, which services and applications hold their state in.
mod json {
    use super::*;
    use serde_json::{Map, Number, Value};
    const NULL: u8 = 0;
    const FALSE: u8 = 1;
    const TRUE: u8 = 2;
    const UINT: u8 = 3;
    const INT: u8 = 4;
    const FLOAT: u8 = 5;
    const STR: u8 = 6;
    const ARRAY: u8 = 7;
    const OBJECT: u8 = 8;
    /// An array whose every element is an integer 0..=255: its length and the bytes.
    /// The engine's JSON holds bytes this way (an HTTP body in an event), and one tag
    /// for the whole array instead of one per byte is what keeps them cheap to read.
    const BYTES: u8 = 9;
    fn small(v: &Value) -> Option<u8> {
        v.as_u64().and_then(|n| u8::try_from(n).ok())
    }
    fn is_bytes(items: &[Value]) -> bool {
        !items.is_empty() && items.iter().all(|v| small(v).is_some())
    }
    impl Encode for Number {
        fn encode(&self, w: &mut Writer) {
            if let Some(v) = self.as_u64() {
                w.byte(UINT);
                w.leb(v);
            } else if let Some(v) = self.as_i64() {
                w.byte(INT);
                w.sleb(v);
            } else {
                w.byte(FLOAT);
                w.raw(&self.as_f64().unwrap_or(0.0).to_bits().to_le_bytes());
            }
        }
    }
    impl Encode for Value {
        fn encode(&self, w: &mut Writer) {
            match self {
                Value::Null => w.byte(NULL),
                Value::Bool(false) => w.byte(FALSE),
                Value::Bool(true) => w.byte(TRUE),
                Value::Number(n) => n.encode(w),
                Value::String(s) => {
                    w.byte(STR);
                    w.str(s);
                }
                Value::Array(items) if is_bytes(items) => {
                    w.byte(BYTES);
                    w.len_prefix(items.len());
                    for item in items {
                        w.byte(small(item).unwrap_or(0));
                    }
                }
                Value::Array(items) => {
                    w.byte(ARRAY);
                    items.encode(w);
                }
                Value::Object(map) => {
                    w.byte(OBJECT);
                    map.encode(w);
                }
            }
        }
    }
    impl Encode for Map<String, Value> {
        fn encode(&self, w: &mut Writer) {
            w.len_prefix(self.len());
            for (k, v) in self {
                w.str(k);
                v.encode(w);
            }
        }
    }
    impl Decode for Map<String, Value> {
        fn decode(r: &mut Reader<'_>) -> Result<Self, Error> {
            let n = r.len_prefix()?;
            r.nested(|r| {
                // An object's keys are written in the order its map holds them, which is
                // ascending: read in that order, the map is built in one pass.
                let mut entries: Vec<(String, Value)> = Vec::with_capacity(n);
                for _ in 0..n {
                    let at = r.position();
                    let k = r.str()?;
                    if entries.last().is_some_and(|(last, _)| last.as_str() >= k) {
                        return Err(Error::at(at, "object keys out of order"));
                    }
                    let k = k.to_owned();
                    entries.push((k, Value::decode(r)?));
                }
                Ok(entries.into_iter().collect())
            })
        }
    }
    impl Decode for Value {
        fn decode(r: &mut Reader<'_>) -> Result<Self, Error> {
            let at = r.position();
            Ok(match r.byte()? {
                NULL => Value::Null,
                FALSE => Value::Bool(false),
                TRUE => Value::Bool(true),
                UINT => Value::Number(r.leb()?.into()),
                INT => {
                    let v = r.sleb()?;
                    if v >= 0 {
                        return Err(Error::at(at, "a number of 0 or more is written unsigned"));
                    }
                    Value::Number(v.into())
                }
                FLOAT => {
                    let v = f64::decode(r)?;
                    Value::Number(
                        Number::from_f64(v)
                            .ok_or_else(|| Error::at(at, "JSON has no infinite or NaN number"))?,
                    )
                }
                STR => Value::String(r.str()?.to_owned()),
                ARRAY => {
                    let items = Vec::decode(r)?;
                    if is_bytes(&items) {
                        return Err(Error::at(at, "an array of bytes is written as bytes"));
                    }
                    Value::Array(items)
                }
                BYTES => {
                    let n = r.len_prefix()?;
                    if n == 0 {
                        return Err(Error::at(at, "an empty array is written as an array"));
                    }
                    Value::Array(r.raw(n)?.iter().map(|&b| Value::from(b)).collect())
                }
                OBJECT => Value::Object(Map::decode(r)?),
                tag => return Err(Error::at(at, format!("{tag} is not a JSON tag"))),
            })
        }
    }
    impl Decode for Number {
        fn decode(r: &mut Reader<'_>) -> Result<Self, Error> {
            let at = r.position();
            match Value::decode(r)? {
                Value::Number(n) => Ok(n),
                _ => Err(Error::at(at, "expected a number")),
            }
        }
    }
}

#[cfg(test)]
mod tests;
