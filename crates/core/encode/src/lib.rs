//! The canonical encoding a world's state, its action journal and its scenes are hashed
//! in: every value as tagged bytes streamed into a [`Sink`] (a hasher), with nothing
//! built in between.
//!
//! The hashes used to be taken over serde_json's text, and over a `serde_json::Value`
//! tree of the whole world for the state. That put three copies of every state type's
//! serialization in the engine (one per serde_json serializer), each formatting numbers
//! and escaping strings, and built a tree of the world on every hash. This encoder is
//! one serializer, not generic over its output (the sink is `dyn`), so each type's
//! `Serialize` is instantiated for it once, and each method is a tag and a few bytes.
//!
//! **It encodes the JSON a value serializes to.** A value and its JSON text read back
//! in order encode alike: a struct is a map of its fields in the order it writes them,
//! a map's entries are in the order it writes them, an `Option` is its value or null, a unit variant its name, a variant
//! with data a map of one entry, an integer is a number whatever its width, a float
//! that is not finite is null, and an `f32` is the `f64` its shortest decimal reads as.
//! That is what lets an application module hold its state as the JSON its application
//! wrote and hash it as a build carrying the application hashes the state itself
//! (`cw_applications::apps::module`).
//!
//! It is unambiguous: every value starts with a tag, lengths and numbers are LEB128,
//! and an array and a map end with [`END`]. It is platform-independent (no `usize`
//! reaches the bytes), so every build hashes alike. It is a hash input, not a format
//! anything reads back.
use serde::ser::{self, Serialize};
use std::fmt;

/// Where the encoding goes.
pub trait Sink {
    fn put(&mut self, bytes: &[u8]);
}

/// Encode `value` into `sink`. Fails only where the value's own `Serialize` does, or
/// where serde_json would (a map key that is not a string, a number or a bool).
pub fn encode<T: Serialize + ?Sized>(value: &T, sink: &mut dyn Sink) -> Result<(), Error> {
    value.serialize(&mut Encoder { sink, key: false })
}

const NULL: u8 = 0;
const FALSE: u8 = 1;
const TRUE: u8 = 2;
/// A number of 0 or more.
const UINT: u8 = 3;
/// A negative number, as its magnitude less one.
const NEG: u8 = 4;
const FLOAT: u8 = 5;
const STR: u8 = 6;
/// Opens an array; a caller that writes one element by element writes this, the
/// elements' encodings, then [`END`].
pub const SEQ: u8 = 7;
const MAP: u8 = 8;
/// An integer beyond 64 bits (serde_json writes it in full; nothing here has one).
const BIG: u8 = 9;
/// Closes an array and a map.
pub const END: u8 = 0xff;

/// A `Serialize` impl's own failure (`ser::Error::custom`), or a map key JSON cannot have.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Error {}
impl ser::Error for Error {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        Error(msg.to_string())
    }
}

pub struct Encoder<'a> {
    sink: &'a mut dyn Sink,
    /// Writing a map key: JSON's keys are strings, so a number or a bool is its text.
    key: bool,
}

impl Encoder<'_> {
    fn tag(&mut self, tag: u8) -> Result<(), Error> {
        if self.key {
            return Err(Error(
                "a map key must be a string, a number or a bool".into(),
            ));
        }
        self.sink.put(&[tag]);
        Ok(())
    }
    fn leb(&mut self, mut v: u64) {
        let mut buf = [0u8; 10];
        let mut n = 0;
        loop {
            let byte = (v & 0x7f) as u8;
            v >>= 7;
            if v == 0 {
                buf[n] = byte;
                n += 1;
                break;
            }
            buf[n] = byte | 0x80;
            n += 1;
        }
        self.sink.put(&buf[..n]);
    }
    fn text(&mut self, text: &str) {
        self.sink.put(&[STR]);
        self.leb(text.len() as u64);
        self.sink.put(text.as_bytes());
    }
    fn number(&mut self, v: i128) -> Result<(), Error> {
        if self.key {
            self.text(&v.to_string());
            return Ok(());
        }
        if let Ok(v) = u64::try_from(v) {
            self.tag(UINT)?;
            self.leb(v);
        } else if let Ok(v) = i64::try_from(v) {
            self.tag(NEG)?;
            self.leb(!(v as u64));
        } else {
            self.tag(BIG)?;
            self.sink.put(&v.to_le_bytes());
        }
        Ok(())
    }
    fn begin_map(&mut self) -> Result<(), Error> {
        self.tag(MAP)
    }
    fn entry_key(&mut self, key: &str) {
        self.text(key);
    }
    fn end(&mut self) -> Result<(), Error> {
        self.sink.put(&[END]);
        Ok(())
    }
}

impl ser::Serializer for &mut Encoder<'_> {
    type Ok = ();
    type Error = Error;
    type SerializeSeq = Self;
    type SerializeTuple = Self;
    type SerializeTupleStruct = Self;
    type SerializeTupleVariant = Self;
    type SerializeMap = Self;
    type SerializeStruct = Self;
    type SerializeStructVariant = Self;

    fn serialize_bool(self, v: bool) -> Result<(), Error> {
        if self.key {
            self.text(if v { "true" } else { "false" });
            return Ok(());
        }
        self.tag(if v { TRUE } else { FALSE })
    }
    fn serialize_i8(self, v: i8) -> Result<(), Error> {
        self.number(v.into())
    }
    fn serialize_i16(self, v: i16) -> Result<(), Error> {
        self.number(v.into())
    }
    fn serialize_i32(self, v: i32) -> Result<(), Error> {
        self.number(v.into())
    }
    fn serialize_i64(self, v: i64) -> Result<(), Error> {
        self.number(v.into())
    }
    fn serialize_i128(self, v: i128) -> Result<(), Error> {
        self.number(v)
    }
    fn serialize_u8(self, v: u8) -> Result<(), Error> {
        self.number(v.into())
    }
    fn serialize_u16(self, v: u16) -> Result<(), Error> {
        self.number(v.into())
    }
    fn serialize_u32(self, v: u32) -> Result<(), Error> {
        self.number(v.into())
    }
    fn serialize_u64(self, v: u64) -> Result<(), Error> {
        self.number(v.into())
    }
    fn serialize_u128(self, v: u128) -> Result<(), Error> {
        match i128::try_from(v) {
            Ok(v) => self.number(v),
            Err(_) => {
                self.tag(BIG)?;
                self.sink.put(&v.to_le_bytes());
                Ok(())
            }
        }
    }
    fn serialize_f32(self, v: f32) -> Result<(), Error> {
        // serde_json writes an f32 as its shortest decimal, which reads back as the f64
        // nearest that decimal (0.1f32 is 0.1, not 0.100000001490116…).
        if !v.is_finite() {
            return self.serialize_unit();
        }
        self.serialize_f64(v.to_string().parse().unwrap_or(f64::from(v)))
    }
    fn serialize_f64(self, v: f64) -> Result<(), Error> {
        if !v.is_finite() {
            return self.serialize_unit();
        }
        self.tag(FLOAT)?;
        self.sink.put(&v.to_bits().to_le_bytes());
        Ok(())
    }
    fn serialize_char(self, v: char) -> Result<(), Error> {
        self.text(v.encode_utf8(&mut [0; 4]));
        Ok(())
    }
    fn serialize_str(self, v: &str) -> Result<(), Error> {
        self.text(v);
        Ok(())
    }
    fn serialize_bytes(self, v: &[u8]) -> Result<(), Error> {
        self.tag(SEQ)?;
        for &b in v {
            self.number(b.into())?;
        }
        self.end()
    }
    fn serialize_none(self) -> Result<(), Error> {
        self.serialize_unit()
    }
    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<(), Error> {
        value.serialize(self)
    }
    fn serialize_unit(self) -> Result<(), Error> {
        self.tag(NULL)
    }
    fn serialize_unit_struct(self, _name: &'static str) -> Result<(), Error> {
        self.serialize_unit()
    }
    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
    ) -> Result<(), Error> {
        self.text(variant);
        Ok(())
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _name: &'static str,
        value: &T,
    ) -> Result<(), Error> {
        value.serialize(self)
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<(), Error> {
        self.begin_map()?;
        self.entry_key(variant);
        value.serialize(&mut *self)?;
        self.end()
    }
    fn serialize_seq(self, _len: Option<usize>) -> Result<Self, Error> {
        self.tag(SEQ)?;
        Ok(self)
    }
    fn serialize_tuple(self, _len: usize) -> Result<Self, Error> {
        self.tag(SEQ)?;
        Ok(self)
    }
    fn serialize_tuple_struct(self, _name: &'static str, _len: usize) -> Result<Self, Error> {
        self.tag(SEQ)?;
        Ok(self)
    }
    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
        _len: usize,
    ) -> Result<Self, Error> {
        // {"variant": [ ... ]}: the array's end and the map's are both written by `end`.
        self.begin_map()?;
        self.entry_key(variant);
        self.tag(SEQ)?;
        Ok(self)
    }
    fn serialize_map(self, _len: Option<usize>) -> Result<Self, Error> {
        self.begin_map()?;
        Ok(self)
    }
    fn serialize_struct(self, _name: &'static str, _len: usize) -> Result<Self, Error> {
        self.begin_map()?;
        Ok(self)
    }
    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
        _len: usize,
    ) -> Result<Self, Error> {
        // {"variant": { ... }}
        self.begin_map()?;
        self.entry_key(variant);
        self.begin_map()?;
        Ok(self)
    }
}

impl ser::SerializeSeq for &mut Encoder<'_> {
    type Ok = ();
    type Error = Error;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Error> {
        value.serialize(&mut **self)
    }
    fn end(self) -> Result<(), Error> {
        Encoder::end(self)
    }
}
impl ser::SerializeTuple for &mut Encoder<'_> {
    type Ok = ();
    type Error = Error;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Error> {
        value.serialize(&mut **self)
    }
    fn end(self) -> Result<(), Error> {
        Encoder::end(self)
    }
}
impl ser::SerializeTupleStruct for &mut Encoder<'_> {
    type Ok = ();
    type Error = Error;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Error> {
        value.serialize(&mut **self)
    }
    fn end(self) -> Result<(), Error> {
        Encoder::end(self)
    }
}
impl ser::SerializeTupleVariant for &mut Encoder<'_> {
    type Ok = ();
    type Error = Error;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Error> {
        value.serialize(&mut **self)
    }
    fn end(self) -> Result<(), Error> {
        Encoder::end(self)?;
        Encoder::end(self)
    }
}
impl ser::SerializeMap for &mut Encoder<'_> {
    type Ok = ();
    type Error = Error;
    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<(), Error> {
        self.key = true;
        let written = key.serialize(&mut **self);
        self.key = false;
        written
    }
    fn serialize_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Error> {
        value.serialize(&mut **self)
    }
    fn end(self) -> Result<(), Error> {
        Encoder::end(self)
    }
}
impl ser::SerializeStruct for &mut Encoder<'_> {
    type Ok = ();
    type Error = Error;
    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), Error> {
        self.entry_key(key);
        value.serialize(&mut **self)
    }
    fn end(self) -> Result<(), Error> {
        Encoder::end(self)
    }
}
impl ser::SerializeStructVariant for &mut Encoder<'_> {
    type Ok = ();
    type Error = Error;
    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), Error> {
        self.entry_key(key);
        value.serialize(&mut **self)
    }
    fn end(self) -> Result<(), Error> {
        Encoder::end(self)?;
        Encoder::end(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;
    use std::collections::BTreeMap;

    impl Sink for Vec<u8> {
        fn put(&mut self, bytes: &[u8]) {
            self.extend_from_slice(bytes);
        }
    }
    fn bytes<T: Serialize + ?Sized>(value: &T) -> Vec<u8> {
        let mut out = Vec::new();
        encode(value, &mut out).unwrap();
        out
    }
    /// What the value's JSON text, read back, encodes as. A `Value`'s map is in key
    /// order, so the values here write their maps in that order (the encoder keeps the
    /// order a map is written in, as the module's transcoder does).
    fn via_json<T: Serialize + ?Sized>(value: &T) -> Vec<u8> {
        let text = serde_json::to_string(value).unwrap();
        bytes(&serde_json::from_str::<serde_json::Value>(&text).unwrap())
    }

    #[derive(Serialize)]
    struct Fields {
        #[serde(skip_serializing_if = "Option::is_none")]
        a: Option<u8>,
        #[serde(skip_serializing_if = "Option::is_none")]
        b: Option<u8>,
        c: Vec<Shape>,
        d: BTreeMap<u32, (f32, f64, i64, char)>,
        e: Option<String>,
        f: (),
        #[serde(with = "serde_bytes_like")]
        g: Vec<u8>,
    }
    mod serde_bytes_like {
        pub fn serialize<S: serde::Serializer>(v: &[u8], s: S) -> Result<S::Ok, S::Error> {
            s.serialize_bytes(v)
        }
    }
    #[derive(Serialize)]
    enum Shape {
        Unit,
        New(u8),
        Pair(i8, u16),
        Named { x: f32 },
    }

    #[test]
    fn a_value_encodes_as_its_json_read_back() {
        // An application module holds its state as JSON text, and hashes it as a build
        // carrying the application hashes the state itself: the two must agree.
        let mut d = BTreeMap::new();
        d.insert(7, (0.1f32, 0.1f64, -3, 'é'));
        d.insert(8, (f32::NAN, -0.0f64, i64::MIN, 'x'));
        d.insert(9, (1.0f32, f64::INFINITY, i64::MAX, '\u{10ffff}'));
        let value = Fields {
            a: Some(1),
            b: None,
            c: vec![
                Shape::Unit,
                Shape::New(2),
                Shape::Pair(-1, 65535),
                Shape::Named { x: 2.5 },
            ],
            d,
            e: None,
            f: (),
            g: vec![0, 255],
        };
        assert_eq!(bytes(&value), via_json(&value));
        assert_eq!(
            bytes(&(1u8, -1i32, u64::MAX, "s", [true, false])),
            via_json(&(1u8, -1i32, u64::MAX, "s", [true, false]))
        );
    }

    #[test]
    fn values_that_differ_encode_differently() {
        // A skipped field is not taken for another one: fields are written by name.
        let one = |a, b| Fields {
            a,
            b,
            c: vec![],
            d: BTreeMap::new(),
            e: None,
            f: (),
            g: vec![],
        };
        assert_ne!(bytes(&one(Some(1), None)), bytes(&one(None, Some(1))));
        // Where an array ends is part of the value.
        assert_ne!(
            bytes(&(vec!["ab"], vec![""])),
            bytes(&(vec!["ab", ""], Vec::<&str>::new()))
        );
        assert_ne!(
            bytes(&vec![vec![1u8], vec![]]),
            bytes(&vec![vec![], vec![1u8]])
        );
        assert_ne!(bytes(&-1i64), bytes(&1u64));
        assert_ne!(bytes(&0.0f64), bytes(&-0.0f64));
        assert_ne!(bytes(&1.0f64), bytes(&1u64));
        assert_ne!(bytes(&"1"), bytes(&1u64));
    }

    #[test]
    fn a_map_key_is_a_string() {
        let mut numbers = BTreeMap::new();
        numbers.insert(-5i32, true);
        let mut strings = BTreeMap::new();
        strings.insert("-5", true);
        assert_eq!(bytes(&numbers), bytes(&strings));
        let mut floats = Vec::new();
        let mut keyed = BTreeMap::new();
        keyed.insert(vec![1u8], 1u8);
        assert!(encode(&keyed, &mut floats).is_err());
    }

    #[test]
    fn the_encoding_is_fixed() {
        // The bytes a value hashes as are part of every recorded hash: a change here
        // changes every state hash, so it is deliberate or it is a bug.
        let mut map = BTreeMap::new();
        map.insert(9u8, (300u32, -2i8, true, 1.5f64, None::<u8>));
        assert_eq!(
            bytes(&(map, Some("é"), Shape::Named { x: 7.0 }, Shape::Unit)),
            [
                7, 8, 6, 1, b'9', 7, 3, 172, 2, 4, 1, 2, 5, 0, 0, 0, 0, 0, 0, 248, 63, 0, 255, 255,
                6, 2, 0xc3, 0xa9, 8, 6, 5, b'N', b'a', b'm', b'e', b'd', 8, 6, 1, b'x', 5, 0, 0, 0,
                0, 0, 0, 28, 64, 255, 255, 6, 4, b'U', b'n', b'i', b't', 255
            ]
        );
    }
}
