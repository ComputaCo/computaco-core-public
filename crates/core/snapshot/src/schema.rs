//! A snapshot's schema: the shape of every type its body holds, by name.
//!
//! The body is written positionally (a struct is its fields, an enum its variant's
//! number and fields), which is what keeps it small and fast. The schema in front of
//! it says what those positions are: each struct's field names and types, each enum's
//! variants by number and name. A reader whose own schema is the snapshot's reads the
//! body directly; any other converts it first (`convert`), matching fields and
//! variants by name.
use super::*;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// A type's index in a [`Schema`].
pub type Ty = u32;

/// The values with no parts a reader could match by name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Prim {
    /// One byte (`u8`).
    Byte,
    /// LEB128 (the other unsigned integers, `usize`).
    Uleb,
    /// LEB128 of the zigzag (the signed integers but 128-bit ones).
    Sleb,
    F32,
    F64,
    U128,
    I128,
    Bool,
    Char,
    Unit,
    /// A length and UTF-8.
    Str,
    /// A JSON value, tagged.
    Json,
}
const PRIMS: [Prim; 12] = [
    Prim::Byte,
    Prim::Uleb,
    Prim::Sleb,
    Prim::F32,
    Prim::F64,
    Prim::U128,
    Prim::I128,
    Prim::Bool,
    Prim::Char,
    Prim::Unit,
    Prim::Str,
    Prim::Json,
];

/// A type with no name: what it is follows from what it holds.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Key {
    Prim(Prim),
    Option(Ty),
    Result(Ty, Ty),
    Seq(Ty),
    Set(Ty),
    Map(Ty, Ty),
    Tuple(Box<[Ty]>),
    Array(u64, Ty),
    /// A length, then a value of the type in that many bytes.
    Nested(Ty),
}

#[derive(Clone, Debug)]
pub(crate) struct Field {
    pub name: u32,
    pub ty: Ty,
    /// Writes the value a reader fills the field with when a snapshot lacks it; only a
    /// build's own schema has these.
    pub default: Option<fn(&mut Writer)>,
}
#[derive(Clone, Debug)]
pub(crate) struct Variant {
    pub number: u64,
    pub name: u32,
    pub fields: Box<[Field]>,
}
#[derive(Clone, Debug)]
pub(crate) enum Kind {
    Key(Key),
    /// A struct: `label` is the type's name in this build (empty in a schema read from
    /// a snapshot), for messages.
    Struct {
        label: &'static str,
        fields: Box<[Field]>,
    },
    Enum {
        label: &'static str,
        variants: Box<[Variant]>,
    },
}

/// One field of a struct or variant as a type describes it.
pub struct FieldSpec {
    pub name: &'static str,
    pub ty: fn(&mut Schema) -> Ty,
    /// The bytes of the value a snapshot without this field reads as; `None` makes the
    /// field required.
    pub default: Option<fn(&mut Writer)>,
}
impl FieldSpec {
    /// A field every snapshot must hold.
    pub fn required(name: &'static str, ty: fn(&mut Schema) -> Ty) -> Self {
        Self {
            name,
            ty,
            default: None,
        }
    }
    /// A field a snapshot may lack, read then as the value `default` writes.
    pub fn defaulted(
        name: &'static str,
        ty: fn(&mut Schema) -> Ty,
        default: fn(&mut Writer),
    ) -> Self {
        Self {
            name,
            ty,
            default: Some(default),
        }
    }
}
pub struct VariantSpec {
    /// What the body writes for this variant.
    pub number: u64,
    pub name: &'static str,
    pub fields: Vec<FieldSpec>,
}

/// The types one root type holds, each once, in the order they are first reached.
#[derive(Default, Debug)]
pub struct Schema {
    pub(crate) names: Vec<Box<str>>,
    name_ix: HashMap<Box<str>, u32>,
    pub(crate) kinds: Vec<Kind>,
    keys: HashMap<Key, Ty>,
    named: HashMap<&'static str, Ty>,
}
impl Schema {
    fn push(&mut self, kind: Kind) -> Ty {
        self.kinds.push(kind);
        (self.kinds.len() - 1) as Ty
    }
    fn key(&mut self, key: Key) -> Ty {
        if let Some(&t) = self.keys.get(&key) {
            return t;
        }
        let t = self.push(Kind::Key(key.clone()));
        self.keys.insert(key, t);
        t
    }
    pub(crate) fn name(&mut self, name: &str) -> u32 {
        if let Some(&n) = self.name_ix.get(name) {
            return n;
        }
        let n = self.names.len() as u32;
        self.names.push(name.into());
        self.name_ix.insert(name.into(), n);
        n
    }
    pub fn prim(&mut self, p: Prim) -> Ty {
        self.key(Key::Prim(p))
    }
    pub fn option(&mut self, t: Ty) -> Ty {
        self.key(Key::Option(t))
    }
    pub fn result(&mut self, t: Ty, e: Ty) -> Ty {
        self.key(Key::Result(t, e))
    }
    pub fn seq(&mut self, t: Ty) -> Ty {
        self.key(Key::Seq(t))
    }
    pub fn set(&mut self, t: Ty) -> Ty {
        self.key(Key::Set(t))
    }
    pub fn map(&mut self, k: Ty, v: Ty) -> Ty {
        self.key(Key::Map(k, v))
    }
    pub fn tuple(&mut self, items: &[Ty]) -> Ty {
        self.key(Key::Tuple(items.into()))
    }
    pub fn array(&mut self, n: usize, t: Ty) -> Ty {
        self.key(Key::Array(n as u64, t))
    }
    /// A value written as its length and its bytes (`Writer::bytes` of its encoding).
    pub fn nested(&mut self, t: Ty) -> Ty {
        self.key(Key::Nested(t))
    }
    /// The type already described under `key` (a type's `type_name`), if any.
    pub fn known(&self, key: &'static str) -> Option<Ty> {
        self.named.get(key).copied()
    }
    fn fields(&mut self, specs: &[FieldSpec]) -> Box<[Field]> {
        specs
            .iter()
            .map(|f| Field {
                name: self.name(f.name),
                ty: (f.ty)(self),
                default: f.default,
            })
            .collect()
    }
    /// A struct, by `key`, with these fields in the order the body writes them.
    pub fn record(&mut self, key: &'static str, fields: &[FieldSpec]) -> Ty {
        if let Some(t) = self.known(key) {
            return t;
        }
        // Registered before its fields, so a type that holds itself refers to itself.
        let t = self.push(Kind::Key(Key::Prim(Prim::Unit)));
        self.named.insert(key, t);
        let fields = self.fields(fields);
        self.kinds[t as usize] = Kind::Struct { label: key, fields };
        t
    }
    /// An enum, by `key`, with the variants this build has.
    pub fn enumeration(&mut self, key: &'static str, variants: &[VariantSpec]) -> Ty {
        if let Some(t) = self.known(key) {
            return t;
        }
        let t = self.push(Kind::Key(Key::Prim(Prim::Unit)));
        self.named.insert(key, t);
        let variants = variants
            .iter()
            .map(|v| Variant {
                number: v.number,
                name: self.name(v.name),
                fields: self.fields(&v.fields),
            })
            .collect();
        self.kinds[t as usize] = Kind::Enum {
            label: key,
            variants,
        };
        t
    }

    /// Whether every body of this schema's `t` is, byte for byte, a body of `rs`'s `rt`:
    /// the same fields in the same places, and every variant it has at the number `rs`
    /// gives it. Another build with the same types, or with variants this one also has,
    /// is read without converting.
    pub(crate) fn reads_as(&self, t: Ty, rs: &Schema, rt: Ty) -> bool {
        let mut seen = std::collections::HashSet::new();
        // A worklist, not recursion: a schema from a snapshot may chain any number of
        // types. A pair already being checked is assumed to hold, which is what makes a
        // type that holds itself check.
        let mut work = vec![(t, rt)];
        let fields = |a: &[Field], b: &[Field], work: &mut Vec<(Ty, Ty)>| {
            a.len() == b.len()
                && a.iter().zip(b).all(|(x, y)| {
                    work.push((x.ty, y.ty));
                    self.names[x.name as usize] == rs.names[y.name as usize]
                })
        };
        while let Some((w, r)) = work.pop() {
            if !seen.insert((w, r)) {
                continue;
            }
            let ok = match (self.kind(w), rs.kind(r)) {
                (Kind::Key(a), Kind::Key(b)) => match (a, b) {
                    (Key::Prim(a), Key::Prim(b)) => a == b,
                    (Key::Option(a), Key::Option(b))
                    | (Key::Seq(a), Key::Seq(b))
                    | (Key::Set(a), Key::Set(b))
                    | (Key::Nested(a), Key::Nested(b)) => {
                        work.push((*a, *b));
                        true
                    }
                    (Key::Result(a, e), Key::Result(b, f)) | (Key::Map(a, e), Key::Map(b, f)) => {
                        work.extend([(*a, *b), (*e, *f)]);
                        true
                    }
                    (Key::Tuple(a), Key::Tuple(b)) => {
                        work.extend(a.iter().copied().zip(b.iter().copied()));
                        a.len() == b.len()
                    }
                    (Key::Array(n, a), Key::Array(m, b)) => {
                        work.push((*a, *b));
                        n == m
                    }
                    _ => false,
                },
                (Kind::Struct { fields: a, .. }, Kind::Struct { fields: b, .. }) => {
                    fields(a, b, &mut work)
                }
                (Kind::Enum { variants: a, .. }, Kind::Enum { variants: b, .. }) => {
                    a.iter().all(|v| {
                        b.iter().find(|u| u.number == v.number).is_some_and(|u| {
                            self.names[v.name as usize] == rs.names[u.name as usize]
                                && fields(&v.fields, &u.fields, &mut work)
                        })
                    })
                }
                _ => false,
            };
            if !ok {
                return false;
            }
        }
        true
    }

    pub(crate) fn kind(&self, t: Ty) -> &Kind {
        &self.kinds[t as usize]
    }

    /// The schema's bytes: its names, its types, then the root's index.
    fn write(&self, root: Ty, w: &mut Writer) {
        w.len_prefix(self.names.len());
        for n in &self.names {
            w.str(n);
        }
        w.len_prefix(self.kinds.len());
        let fields = |w: &mut Writer, fields: &[Field]| {
            w.len_prefix(fields.len());
            for f in fields {
                w.leb(f.name.into());
                w.leb(f.ty.into());
            }
        };
        for kind in &self.kinds {
            match kind {
                Kind::Key(Key::Prim(p)) => w.byte(*p as u8),
                Kind::Key(Key::Option(t)) => {
                    w.byte(OPTION);
                    w.leb((*t).into());
                }
                Kind::Key(Key::Result(t, e)) => {
                    w.byte(RESULT);
                    w.leb((*t).into());
                    w.leb((*e).into());
                }
                Kind::Key(Key::Seq(t)) => {
                    w.byte(SEQ);
                    w.leb((*t).into());
                }
                Kind::Key(Key::Set(t)) => {
                    w.byte(SET);
                    w.leb((*t).into());
                }
                Kind::Key(Key::Map(k, v)) => {
                    w.byte(MAP);
                    w.leb((*k).into());
                    w.leb((*v).into());
                }
                Kind::Key(Key::Tuple(items)) => {
                    w.byte(TUPLE);
                    w.len_prefix(items.len());
                    for t in items.iter() {
                        w.leb((*t).into());
                    }
                }
                Kind::Key(Key::Array(n, t)) => {
                    w.byte(ARRAY);
                    w.leb(*n);
                    w.leb((*t).into());
                }
                Kind::Key(Key::Nested(t)) => {
                    w.byte(NESTED);
                    w.leb((*t).into());
                }
                Kind::Struct { fields: f, .. } => {
                    w.byte(STRUCT);
                    fields(w, f);
                }
                Kind::Enum { variants, .. } => {
                    w.byte(ENUM);
                    w.len_prefix(variants.len());
                    for v in variants.iter() {
                        w.leb(v.number);
                        w.leb(v.name.into());
                        fields(w, &v.fields);
                    }
                }
            }
        }
        w.leb(root.into());
    }

    /// A schema a snapshot holds, and its root. Everything it refers to must be in it,
    /// and no struct or enum may name a field or variant twice.
    pub(crate) fn read(r: &mut Reader<'_>) -> Result<(Schema, Ty), Error> {
        let mut s = Schema::default();
        let n = r.len_prefix()?;
        for _ in 0..n {
            let at = r.position();
            let name = r.str()?;
            if s.name_ix.contains_key(name) {
                return Err(Error::at(at, format!("the schema names {name:?} twice")));
            }
            s.name(name);
        }
        let count = r.len_prefix()?;
        let ty = |r: &mut Reader<'_>| -> Result<Ty, Error> {
            let at = r.position();
            let t = r.leb()?;
            if t >= count as u64 {
                return Err(Error::at(at, format!("type {t} is not in the schema")));
            }
            Ok(t as Ty)
        };
        let names = s.names.len() as u64;
        let fields = |r: &mut Reader<'_>| -> Result<Box<[Field]>, Error> {
            let n = r.len_prefix()?;
            let mut out: Vec<Field> = Vec::with_capacity(n);
            for _ in 0..n {
                let at = r.position();
                let name = r.leb()?;
                if name >= names {
                    return Err(Error::at(at, format!("name {name} is not in the schema")));
                }
                if out.iter().any(|f| u64::from(f.name) == name) {
                    return Err(Error::at(at, "a struct names one field twice"));
                }
                out.push(Field {
                    name: name as u32,
                    ty: ty(r)?,
                    default: None,
                });
            }
            Ok(out.into())
        };
        for _ in 0..count {
            let at = r.position();
            let kind = match r.byte()? {
                p if (p as usize) < PRIMS.len() => Kind::Key(Key::Prim(PRIMS[p as usize])),
                OPTION => Kind::Key(Key::Option(ty(r)?)),
                RESULT => Kind::Key(Key::Result(ty(r)?, ty(r)?)),
                SEQ => Kind::Key(Key::Seq(ty(r)?)),
                SET => Kind::Key(Key::Set(ty(r)?)),
                MAP => Kind::Key(Key::Map(ty(r)?, ty(r)?)),
                TUPLE => {
                    let n = r.len_prefix()?;
                    Kind::Key(Key::Tuple((0..n).map(|_| ty(r)).collect::<Result<_, _>>()?))
                }
                ARRAY => Kind::Key(Key::Array(r.leb()?, ty(r)?)),
                NESTED => Kind::Key(Key::Nested(ty(r)?)),
                STRUCT => Kind::Struct {
                    label: "",
                    fields: fields(r)?,
                },
                ENUM => {
                    let n = r.len_prefix()?;
                    let mut variants: Vec<Variant> = Vec::with_capacity(n);
                    for _ in 0..n {
                        let at = r.position();
                        let number = r.leb()?;
                        let name = r.leb()?;
                        if name >= names {
                            return Err(Error::at(at, format!("name {name} is not in the schema")));
                        }
                        if variants
                            .iter()
                            .any(|v| v.number == number || u64::from(v.name) == name)
                        {
                            return Err(Error::at(at, "an enum has one variant twice"));
                        }
                        variants.push(Variant {
                            number,
                            name: name as u32,
                            fields: fields(r)?,
                        });
                    }
                    Kind::Enum {
                        label: "",
                        variants: variants.into(),
                    }
                }
                tag => return Err(Error::at(at, format!("{tag} is not a schema tag"))),
            };
            s.kinds.push(kind);
        }
        let root = ty(r)?;
        Ok((s, root))
    }
}
const OPTION: u8 = 16;
const RESULT: u8 = 17;
const SEQ: u8 = 18;
const SET: u8 = 19;
const MAP: u8 = 20;
const TUPLE: u8 = 21;
const ARRAY: u8 = 22;
const NESTED: u8 = 23;
const STRUCT: u8 = 24;
const ENUM: u8 = 25;

/// A type's shape, which its snapshot carries in front of the body.
pub trait Describe {
    /// Register this type (and what it holds) in `s`, and return its index.
    fn describe(s: &mut Schema) -> Ty;
}

/// A root type's schema in this build, and its bytes.
pub(crate) struct Layout {
    pub schema: Schema,
    pub root: Ty,
    pub header: Vec<u8>,
}
/// `T`'s layout, built once per process: a schema is a function of the build.
pub(crate) fn layout<T: Describe + ?Sized>() -> Arc<Layout> {
    static CACHE: OnceLock<Mutex<HashMap<&'static str, Arc<Layout>>>> = OnceLock::new();
    let key = std::any::type_name::<T>();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(l) = cache.lock().unwrap_or_else(|e| e.into_inner()).get(key) {
        return l.clone();
    }
    let mut schema = Schema::default();
    let root = T::describe(&mut schema);
    let mut w = Writer::new();
    schema.write(root, &mut w);
    let layout = Arc::new(Layout {
        schema,
        root,
        header: w.buf,
    });
    cache
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .entry(key)
        .or_insert(layout)
        .clone()
}

/// `T`'s schema bytes in this build: what a snapshot of a `T` starts with.
pub fn header<T: Describe + ?Sized>() -> Vec<u8> {
    layout::<T>().header.clone()
}

/// A type's name without its module paths, for messages.
pub(crate) fn short(label: &str) -> String {
    let mut out = String::new();
    let mut word = String::new();
    let mut chars = label.chars().peekable();
    while let Some(c) = chars.next() {
        if c.is_alphanumeric() || c == '_' {
            word.push(c);
        } else if c == ':' && chars.peek() == Some(&':') {
            chars.next();
            word.clear();
        } else {
            out.push_str(&word);
            word.clear();
            out.push(c);
        }
    }
    out + &word
}

macro_rules! prims {
    ($($t:ty => $p:ident),* $(,)?) => {$(
        impl Describe for $t {
            fn describe(s: &mut Schema) -> Ty {
                s.prim(Prim::$p)
            }
        }
    )*};
}
prims!(
    u8 => Byte, u16 => Uleb, u32 => Uleb, u64 => Uleb, usize => Uleb,
    i8 => Sleb, i16 => Sleb, i32 => Sleb, i64 => Sleb, isize => Sleb,
    u128 => U128, i128 => I128, f32 => F32, f64 => F64, bool => Bool, char => Char,
    () => Unit, str => Str, String => Str,
    serde_json::Value => Json, serde_json::Number => Json,
);
impl Describe for serde_json::Map<String, serde_json::Value> {
    fn describe(s: &mut Schema) -> Ty {
        let (k, v) = (s.prim(Prim::Str), s.prim(Prim::Json));
        s.map(k, v)
    }
}
macro_rules! transparent {
    ($($t:ident),*) => {$(
        impl<T: Describe + ?Sized> Describe for $t<T> {
            fn describe(s: &mut Schema) -> Ty {
                T::describe(s)
            }
        }
    )*};
}
transparent!(Box, Arc, Rc);
impl<T: Describe + ?Sized> Describe for &T {
    fn describe(s: &mut Schema) -> Ty {
        T::describe(s)
    }
}
impl<T: Describe> Describe for Option<T> {
    fn describe(s: &mut Schema) -> Ty {
        let t = T::describe(s);
        s.option(t)
    }
}
impl<T: Describe, E: Describe> Describe for Result<T, E> {
    fn describe(s: &mut Schema) -> Ty {
        let (t, e) = (T::describe(s), E::describe(s));
        s.result(t, e)
    }
}
macro_rules! seqs {
    ($($t:ty),*) => {$(
        impl<T: Describe> Describe for $t {
            fn describe(s: &mut Schema) -> Ty {
                let t = T::describe(s);
                s.seq(t)
            }
        }
    )*};
}
seqs!([T], Vec<T>, VecDeque<T>);
impl<T: Describe> Describe for BTreeSet<T> {
    fn describe(s: &mut Schema) -> Ty {
        let t = T::describe(s);
        s.set(t)
    }
}
impl<K: Describe, V: Describe> Describe for BTreeMap<K, V> {
    fn describe(s: &mut Schema) -> Ty {
        let (k, v) = (K::describe(s), V::describe(s));
        s.map(k, v)
    }
}
impl<T: Describe, const N: usize> Describe for [T; N] {
    fn describe(s: &mut Schema) -> Ty {
        let t = T::describe(s);
        s.array(N, t)
    }
}
macro_rules! tuples {
    ($(($($t:ident),+)),*) => {$(
        impl<$($t: Describe),+> Describe for ($($t,)+) {
            fn describe(s: &mut Schema) -> Ty {
                let items = [$($t::describe(s)),+];
                s.tuple(&items)
            }
        }
    )*};
}
tuples!(
    (A),
    (A, B),
    (A, B, C),
    (A, B, C, D),
    (A, B, C, D, E),
    (A, B, C, D, E, F)
);
