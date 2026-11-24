use super::*;
use proptest::prelude::*;
use serde_json::{json, Value};

#[derive(Debug, Clone, PartialEq, Encode, Decode)]
struct Fields {
    a: u8,
    b: Option<i32>,
    c: Vec<Shape>,
    d: BTreeMap<String, (f32, f64, char)>,
    #[snapshot(skip)]
    scratch: u64,
    e: Arc<Vec<u8>>,
    f: Value,
    g: BTreeSet<u16>,
}
#[derive(Debug, Clone, PartialEq, Encode, Decode)]
enum Shape {
    Unit,
    New(i64),
    Pair(i8, u16),
    Named { x: f32, tail: Option<Box<Shape>> },
}
#[derive(Debug, Clone, PartialEq, Encode, Decode)]
enum Featured {
    Always,
    #[cfg(not(any()))]
    Built(u8),
}

fn shape() -> impl Strategy<Value = Shape> {
    let leaf = prop_oneof![
        Just(Shape::Unit),
        any::<i64>().prop_map(Shape::New),
        (any::<i8>(), any::<u16>()).prop_map(|(a, b)| Shape::Pair(a, b)),
    ];
    leaf.prop_recursive(4, 16, 2, |inner| {
        (any::<f32>(), proptest::option::of(inner)).prop_map(|(x, t)| Shape::Named {
            x,
            tail: t.map(Box::new),
        })
    })
}
fn json() -> impl Strategy<Value = Value> {
    let leaf = prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        any::<u64>().prop_map(Value::from),
        any::<i64>().prop_map(Value::from),
        any::<f64>()
            .prop_filter("finite", |f| f.is_finite())
            .prop_map(Value::from),
        ".{0,8}".prop_map(Value::String),
    ];
    leaf.prop_recursive(4, 32, 4, |inner| {
        prop_oneof![
            proptest::collection::vec(inner.clone(), 0..4).prop_map(Value::Array),
            proptest::collection::btree_map(".{0,4}", inner, 0..4)
                .prop_map(|m| Value::Object(m.into_iter().collect())),
        ]
    })
}
fn fields() -> impl Strategy<Value = Fields> {
    (
        any::<u8>(),
        proptest::option::of(any::<i32>()),
        proptest::collection::vec(shape(), 0..4),
        proptest::collection::btree_map(
            ".{0,6}",
            (any::<f32>(), any::<f64>(), any::<char>()),
            0..4,
        ),
        proptest::collection::vec(any::<u8>(), 0..64),
        json(),
        proptest::collection::btree_set(any::<u16>(), 0..6),
    )
        .prop_map(|(a, b, c, d, e, f, g)| Fields {
            a,
            b,
            c,
            d,
            scratch: 0,
            e: Arc::new(e),
            f,
            g,
        })
}

/// Floats compare by bits, so a NaN round trip counts.
fn same(a: &Fields, b: &Fields) -> bool {
    to_bytes(a) == to_bytes(b)
}

proptest! {
    #[test]
    fn a_value_reads_back_as_itself(value in fields()) {
        let bytes = to_bytes(&value).unwrap();
        let back: Fields = from_bytes(&bytes).unwrap();
        prop_assert!(same(&value, &back));
        // One value, one encoding: what was read writes the same bytes.
        prop_assert_eq!(to_bytes(&back).unwrap(), bytes);
    }

    #[test]
    fn a_cut_or_padded_input_is_refused(value in fields(), cut in any::<prop::sample::Index>(), pad in 1u8..) {
        let bytes = to_bytes(&value).unwrap();
        let at = cut.index(bytes.len());
        prop_assert!(from_bytes::<Fields>(&bytes[..at]).is_err());
        let mut longer = bytes.clone();
        longer.push(pad);
        prop_assert!(from_bytes::<Fields>(&longer).is_err());
    }

    #[test]
    fn any_input_is_read_or_refused_without_panicking(bytes in proptest::collection::vec(any::<u8>(), 0..512)) {
        if let Ok(value) = from_bytes::<Fields>(&bytes) {
            // What a reader accepts is exactly what a writer writes.
            prop_assert_eq!(to_bytes(&value).unwrap(), bytes.clone());
        }
        let _ = from_bytes::<Value>(&bytes);
        let _ = from_bytes::<Vec<Shape>>(&bytes);
    }

    #[test]
    fn flipping_a_byte_never_panics(value in fields(), at in any::<prop::sample::Index>(), bit in 0u8..8) {
        let mut bytes = to_bytes(&value).unwrap();
        if !bytes.is_empty() {
            let i = at.index(bytes.len());
            bytes[i] ^= 1 << bit;
            let _ = from_bytes::<Fields>(&bytes);
        }
    }
}

#[test]
fn the_encoding_is_fixed() {
    // Snapshots written by one build are read by every other: these bytes are the format.
    let mut d = BTreeMap::new();
    d.insert("k".to_owned(), (1.5f32, -0.0f64, 'é'));
    let value = Fields {
        a: 7,
        b: Some(-2),
        c: vec![Shape::Unit, Shape::Pair(-1, 300)],
        d,
        scratch: 99,
        e: Arc::new(vec![1, 2]),
        f: json!({"a": [null, true, 1, -1, "x"]}),
        g: [5u16].into(),
    };
    assert_eq!(
        body_bytes(&value).unwrap(),
        [
            7, 1, 3, 2, 0, 2, 1, 172, 2, 1, 1, b'k', 0, 0, 0xc0, 0x3f, 0, 0, 0, 0, 0, 0, 0, 0x80,
            233, 1, 2, 1, 2, 8, 1, 1, b'a', 7, 5, 0, 2, 3, 1, 4, 1, 6, 1, b'x', 1, 5
        ]
    );
    let back: Fields = from_bytes(&to_bytes(&value).unwrap()).unwrap();
    assert_eq!(back.scratch, 0, "a skipped field is not written");
    // A snapshot is its schema, then the body.
    let bytes = to_bytes(&value).unwrap();
    assert_eq!(bytes[..header::<Fields>().len()], header::<Fields>()[..]);
    assert_eq!(
        bytes[header::<Fields>().len()..],
        body_bytes(&value).unwrap()[..]
    );
    // JSON's bytes (an array of integers 0..=255) are one tag and the bytes.
    assert_eq!(body_bytes(&json!([1, 255])).unwrap(), [9, 2, 1, 255]);
    assert_eq!(
        body_bytes(&json!([1, 256])).unwrap(),
        [7, 2, 3, 1, 3, 128, 2]
    );
    assert_eq!(body_bytes(&json!([])).unwrap(), [7, 0]);
    assert!(
        from_body_bytes::<Value>(&[7, 1, 3, 1]).is_err(),
        "bytes written as an array"
    );
    // A variant is its number among all the enum's variants, those a build leaves out
    // included, so builds with different features agree on it.
    assert_eq!(body_bytes(&Featured::Built(1)).unwrap(), [1, 1]);
    // A schema names a type's parts: the names, then each type, then the root.
    assert_eq!(
        header::<Featured>(),
        [
            // Three names: the variants', and the tuple variant's field "0".
            3, 6, b'A', b'l', b'w', b'a', b'y', b's', 5, b'B', b'u', b'i', b'l', b't', 1, b'0',
            // Two types: the enum (variant 0 with no fields, variant 1 with field "0"
            // of type 1), then a byte.
            2, 25, 2, 0, 0, 0, 1, 1, 1, 2, 1, 0, // The root.
            0
        ]
    );
}

#[test]
fn malformed_inputs_name_what_is_wrong() {
    let err = |bytes: &[u8]| {
        from_body_bytes::<Vec<String>>(bytes)
            .unwrap_err()
            .to_string()
    };
    assert!(
        err(&[0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x7f])
            .contains("beyond 64 bits")
    );
    assert!(err(&[9]).contains("length 9"));
    assert!(err(&[1, 1, 0xff]).contains("UTF-8"));
    assert!(err(&[0x80, 0]).contains("shortest"));
    let mut map = Writer::new();
    map.leb(2);
    map.str("b");
    map.byte(1);
    map.str("a");
    map.byte(1);
    let out = from_body_bytes::<BTreeMap<String, bool>>(&map.finish().unwrap()).unwrap_err();
    assert!(out.to_string().contains("out of order"), "{out}");
    // Nesting is bounded, so a hostile input cannot overflow the stack.
    let mut deep = [7u8, 1].repeat(100_000);
    deep.push(0);
    assert!(from_body_bytes::<Value>(&deep)
        .unwrap_err()
        .to_string()
        .contains("deeply"));
}

/// Two builds of one type: what the first wrote, the second reads, and back.
mod v1 {
    use super::*;
    use serde::{Deserialize, Serialize};
    #[derive(Debug, Clone, PartialEq, Encode, Decode, Serialize, Deserialize)]
    pub struct Item {
        pub id: u32,
        pub name: String,
        #[serde(default)]
        pub gone: bool,
        pub inner: Inner,
        pub kind: Kind,
        pub by_key: BTreeMap<Key, u8>,
        pub list: Vec<Kind>,
    }
    #[derive(Debug, Clone, PartialEq, Encode, Decode, Serialize, Deserialize)]
    pub struct Inner {
        pub a: u8,
        pub b: Vec<u16>,
    }
    #[derive(Debug, Clone, PartialEq, Encode, Decode, Serialize, Deserialize)]
    pub enum Kind {
        First,
        Second(u8),
        Third { x: i32 },
    }
    #[derive(
        Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Encode, Decode, Serialize, Deserialize,
    )]
    pub enum Key {
        A,
        B,
    }
}
mod v2 {
    use super::*;
    use serde::{Deserialize, Serialize};
    fn seven() -> u8 {
        7
    }
    /// v1's `Item` with its fields reordered, `gone` removed and four added.
    #[derive(Debug, Clone, PartialEq, Encode, Decode, Serialize, Deserialize)]
    pub struct Item {
        pub kind: Kind,
        pub name: String,
        #[serde(default)]
        pub count: u64,
        pub id: u32,
        pub note: Option<String>,
        pub inner: Inner,
        #[serde(default = "seven")]
        pub seven: u8,
        pub by_key: BTreeMap<Key, u8>,
        pub list: Vec<Kind>,
    }
    #[derive(Debug, Clone, PartialEq, Encode, Decode, Serialize, Deserialize)]
    pub struct Inner {
        pub a: u8,
        pub b: Vec<u16>,
        pub c: Option<bool>,
    }
    /// A variant inserted in the middle, and a field added to another.
    #[derive(Debug, Clone, PartialEq, Encode, Decode, Serialize, Deserialize)]
    pub enum Kind {
        First,
        Inserted,
        Second(u8),
        Third {
            x: i32,
            #[serde(default)]
            y: i32,
        },
    }
    /// The variants in the other order, so a map keyed by them sorts the other way.
    #[derive(
        Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Encode, Decode, Serialize, Deserialize,
    )]
    pub enum Key {
        B,
        A,
    }
}
mod v3 {
    use super::*;
    use serde::{Deserialize, Serialize};
    /// v1's `Item` with a field no snapshot of v1 holds and nothing can fill.
    #[derive(Debug, Clone, PartialEq, Encode, Decode, Serialize, Deserialize)]
    pub struct Item {
        pub id: u32,
        pub must: u32,
    }
    /// v1's `Item` with a field whose type changed.
    #[derive(Debug, Clone, PartialEq, Encode, Decode, Serialize, Deserialize)]
    pub struct Retyped {
        pub id: String,
    }
}

fn first() -> v1::Item {
    v1::Item {
        id: 300,
        name: "lamp".into(),
        gone: true,
        inner: v1::Inner {
            a: 9,
            b: vec![1, 4464],
        },
        kind: v1::Kind::Third { x: -5 },
        by_key: [(v1::Key::A, 1), (v1::Key::B, 2)].into(),
        list: vec![
            v1::Kind::Second(4),
            v1::Kind::First,
            v1::Kind::Third { x: 1 },
        ],
    }
}

#[test]
fn a_snapshot_reads_into_a_build_whose_types_changed_since() {
    let bytes = to_bytes(&first()).unwrap();
    assert!(open::<v2::Item>(&bytes).unwrap().converted());
    let read: v2::Item = from_bytes(&bytes).unwrap();
    assert_eq!(
        read,
        v2::Item {
            kind: v2::Kind::Third { x: -5, y: 0 },
            name: "lamp".into(),
            count: 0,
            id: 300,
            note: None,
            inner: v2::Inner {
                a: 9,
                b: vec![1, 4464],
                c: None,
            },
            seven: 7,
            by_key: [(v2::Key::A, 1), (v2::Key::B, 2)].into(),
            list: vec![
                v2::Kind::Second(4),
                v2::Kind::First,
                v2::Kind::Third { x: 1, y: 0 }
            ],
        }
    );
    // What was read writes as this build writes, and reads back without converting.
    let again = to_bytes(&read).unwrap();
    assert!(!open::<v2::Item>(&again).unwrap().converted());
    assert_eq!(from_bytes::<v2::Item>(&again).unwrap(), read);
}

#[test]
fn a_snapshot_reads_into_a_build_older_than_it() {
    let mut newer: v2::Item = from_bytes(&to_bytes(&first()).unwrap()).unwrap();
    newer.count = 12;
    newer.note = Some("dropped".into());
    newer.inner.c = Some(true);
    newer.kind = v2::Kind::Second(8);
    let read: v1::Item = from_bytes(&to_bytes(&newer).unwrap()).unwrap();
    assert_eq!(
        read,
        v1::Item {
            gone: false,
            kind: v1::Kind::Second(8),
            ..first()
        }
    );
    // A variant the older build does not have is refused by name.
    newer.list.push(v2::Kind::Inserted);
    let err = from_bytes::<v1::Item>(&to_bytes(&newer).unwrap()).unwrap_err();
    assert!(err.message.contains("Kind::Inserted"), "{err}");
    assert!(err.message.contains("Item.list"), "{err}");
}

#[test]
fn a_required_field_a_snapshot_lacks_is_refused_by_name() {
    let bytes = to_bytes(&first()).unwrap();
    let err = from_bytes::<v3::Item>(&bytes).unwrap_err();
    assert!(
        err.message.contains("Item has a field `must`") && err.message.contains("no default"),
        "{err}"
    );
    let err = from_bytes::<v3::Retyped>(&bytes).unwrap_err();
    assert!(
        err.message.contains("Retyped.id")
            && err
                .message
                .contains("an unsigned integer where this build reads text"),
        "{err}"
    );
}

#[test]
fn a_struct_added_to_a_tuple_and_a_byte_widened_convert() {
    // u8 -> u32 and back while it fits; a newtype gaining a field.
    #[derive(Debug, PartialEq, Encode, Decode)]
    struct Narrow(u8, Vec<u8>);
    #[derive(Debug, PartialEq, Encode, Decode)]
    struct Wide(u32, Vec<u8>, Option<i8>);
    let wide: Wide = from_bytes(&to_bytes(&Narrow(200, vec![1, 2])).unwrap()).unwrap();
    assert_eq!(wide, Wide(200, vec![1, 2], None));
    let narrow: Narrow = from_bytes(&to_bytes(&Wide(5, vec![], Some(1))).unwrap()).unwrap();
    assert_eq!(narrow, Narrow(5, vec![]));
    assert!(from_bytes::<Narrow>(&to_bytes(&Wide(256, vec![], None)).unwrap()).is_err());
}

#[test]
fn values_with_no_bytes_cannot_make_a_conversion_unbounded() {
    #[derive(Debug, PartialEq, Encode, Decode)]
    struct Empty {}
    #[derive(Debug, PartialEq, Encode, Decode)]
    struct Filled {
        x: Option<u8>,
    }
    // 2,000 sequences of nearly 4,000 values of no bytes each: 8 million values from
    // 8 KB, each of which conversion writes a byte for.
    let mut w = Writer::document::<Vec<Vec<Empty>>>();
    let k = 2_000u64;
    w.leb(k);
    let n = 2 * k - 10;
    for _ in 0..k {
        w.leb(n);
    }
    let mut bytes = w.buf;
    bytes.resize(bytes.len() + 2 * k as usize, 0);
    let err = from_bytes::<Vec<Vec<Filled>>>(&bytes).unwrap_err();
    assert!(err.message.contains("more values"), "{err}");
}

proptest! {
    #[test]
    fn a_damaged_snapshot_of_another_build_is_refused_without_panicking(
        at in any::<prop::sample::Index>(), bit in 0u8..8, cut in any::<prop::sample::Index>()
    ) {
        let bytes = to_bytes(&first()).unwrap();
        let mut flipped = bytes.clone();
        let i = at.index(flipped.len());
        flipped[i] ^= 1 << bit;
        let _ = from_bytes::<v2::Item>(&flipped);
        let _ = from_bytes::<v1::Item>(&flipped);
        prop_assert!(from_bytes::<v2::Item>(&bytes[..cut.index(bytes.len())]).is_err());
        let mut longer = bytes.clone();
        longer.push(0);
        prop_assert!(from_bytes::<v2::Item>(&longer).is_err());
    }

    #[test]
    fn any_bytes_as_a_snapshot_of_another_build_never_panic(tail in proptest::collection::vec(any::<u8>(), 0..256)) {
        // A real schema, then anything: the conversion's walk meets every shape.
        let mut bytes = header::<v1::Item>();
        bytes.extend_from_slice(&tail);
        let _ = from_bytes::<v2::Item>(&bytes);
        let _ = from_bytes::<v1::Item>(&bytes);
    }
}
