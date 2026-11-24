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
        to_bytes(&value).unwrap(),
        [
            7, 1, 3, 2, 0, 2, 1, 172, 2, 1, 1, b'k', 0, 0, 0xc0, 0x3f, 0, 0, 0, 0, 0, 0, 0, 0x80,
            233, 1, 2, 1, 2, 8, 1, 1, b'a', 7, 5, 0, 2, 3, 1, 4, 1, 6, 1, b'x', 1, 5
        ]
    );
    let back: Fields = from_bytes(&to_bytes(&value).unwrap()).unwrap();
    assert_eq!(back.scratch, 0, "a skipped field is not written");
    // JSON's bytes (an array of integers 0..=255) are one tag and the bytes.
    assert_eq!(to_bytes(&json!([1, 255])).unwrap(), [9, 2, 1, 255]);
    assert_eq!(to_bytes(&json!([1, 256])).unwrap(), [7, 2, 3, 1, 3, 128, 2]);
    assert_eq!(to_bytes(&json!([])).unwrap(), [7, 0]);
    assert!(
        from_bytes::<Value>(&[7, 1, 3, 1]).is_err(),
        "bytes written as an array"
    );
    // A variant only some builds have is written by name.
    assert_eq!(
        to_bytes(&Featured::Built(1)).unwrap(),
        [5, b'B', b'u', b'i', b'l', b't', 1]
    );
}

#[test]
fn malformed_inputs_name_what_is_wrong() {
    let err = |bytes: &[u8]| from_bytes::<Vec<String>>(bytes).unwrap_err().to_string();
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
    let out = from_bytes::<BTreeMap<String, bool>>(&map.finish().unwrap()).unwrap_err();
    assert!(out.to_string().contains("out of order"), "{out}");
    // Nesting is bounded, so a hostile input cannot overflow the stack.
    let mut deep = [7u8, 1].repeat(100_000);
    deep.push(0);
    assert!(from_bytes::<Value>(&deep)
        .unwrap_err()
        .to_string()
        .contains("deeply"));
}
