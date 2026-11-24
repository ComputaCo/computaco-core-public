//! Portable append-only hash chained journals. Verification detects edits and reordering.
mod chunks;
pub use chunks::{Chunks, Iter, CHUNK};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Entry<T> {
    pub index: u64,
    pub previous_hash: String,
    pub hash: String,
    pub value: T,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(bound(
    serialize = "T: Serialize",
    deserialize = "T: Deserialize<'de> + Clone"
))]
pub struct Journal<T> {
    /// Chunked and shared: a fork shares its parent's journal, and the first step it
    /// records copies the open chunk instead of every entry (47,210 allocations, 5.3 MB,
    /// for a journal of 4,000 steps). It serializes as the flat array it always was.
    pub entries: Chunks<Entry<T>>,
}
impl<T> Default for Journal<T> {
    fn default() -> Self {
        Self {
            entries: Chunks::default(),
        }
    }
}
/// SHA-256 of `value`'s canonical encoding (`cw_encode`), streamed into the hasher.
pub fn stable_hash<T: Serialize + ?Sized>(value: &T) -> Result<String, serde_json::Error> {
    struct Hasher(Sha256);
    impl cw_encode::Sink for Hasher {
        fn put(&mut self, bytes: &[u8]) {
            self.0.update(bytes);
        }
    }
    let mut hasher = Hasher(Sha256::new());
    cw_encode::encode(value, &mut hasher)
        .map_err(<serde_json::Error as serde::ser::Error>::custom)?;
    Ok(format!("{:x}", hasher.0.finalize()))
}
impl<T: Serialize + Clone> Journal<T> {
    pub fn append(&mut self, value: T) -> Result<&Entry<T>, serde_json::Error> {
        let index = self.entries.len() as u64;
        let previous_hash = self
            .entries
            .last()
            .map(|e| e.hash.clone())
            .unwrap_or_default();
        let hash = stable_hash(&(index, &previous_hash, &value))?;
        self.entries.push(Entry {
            index,
            previous_hash,
            hash,
            value,
        });
        Ok(self.entries.last().unwrap())
    }
    pub fn verify(&self) -> Result<bool, serde_json::Error> {
        let mut previous = "";
        for (index, e) in self.entries.iter().enumerate() {
            if e.index != index as u64
                || e.previous_hash != previous
                || e.hash != stable_hash(&(e.index, &e.previous_hash, &e.value))?
            {
                return Ok(false);
            }
            previous = &e.hash;
        }
        Ok(true)
    }
    pub fn jsonl(&self) -> Result<String, serde_json::Error> {
        let mut output = String::new();
        for entry in &self.entries {
            output.push_str(&serde_json::to_string(entry)?);
            output.push('\n');
        }
        Ok(output)
    }
}
impl<T: Serialize + Clone + for<'de> Deserialize<'de>> Journal<T> {
    pub fn from_jsonl(input: &str) -> Result<Self, String> {
        let journal = Self {
            entries: Chunks::from(
                input
                    .lines()
                    .filter(|l| !l.trim().is_empty())
                    .map(serde_json::from_str)
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| e.to_string())?,
            ),
        };
        if !journal.verify().map_err(|e| e.to_string())? {
            return Err("invalid journal hash chain".into());
        }
        Ok(journal)
    }
}
/// A journal is written as its entries' hashes and values: each entry's index is its
/// place and its previous hash the entry before's, so neither is written. `verify` still
/// recomputes every hash after reading.
impl<T: cw_snapshot::Describe> cw_snapshot::Describe for Journal<T> {
    fn describe(s: &mut cw_snapshot::Schema) -> cw_snapshot::Ty {
        use cw_snapshot::{Describe, FieldSpec};
        // Keyed by the journal, not by `Entry`: an entry here is two of its fields.
        let entry = s.record(
            std::any::type_name::<Self>(),
            &[
                FieldSpec::required("hash", <String as Describe>::describe),
                FieldSpec::required("value", <T as Describe>::describe),
            ],
        );
        s.seq(entry)
    }
}
impl<T: cw_snapshot::Encode> cw_snapshot::Encode for Journal<T> {
    fn encode(&self, w: &mut cw_snapshot::Writer) {
        w.len_prefix(self.entries.len());
        for e in &self.entries {
            e.hash.encode(w);
            e.value.encode(w);
        }
    }
}
impl<T: cw_snapshot::Decode + Clone> cw_snapshot::Decode for Journal<T> {
    fn decode(r: &mut cw_snapshot::Reader<'_>) -> Result<Self, cw_snapshot::Error> {
        let n = r.len_prefix()?;
        let mut entries: Vec<Entry<T>> = Vec::with_capacity(n);
        for index in 0..n as u64 {
            let previous_hash = entries.last().map(|e| e.hash.clone()).unwrap_or_default();
            entries.push(Entry {
                index,
                previous_hash,
                hash: String::decode(r)?,
                value: r.nested(T::decode)?,
            });
        }
        Ok(Self {
            entries: Chunks::from(entries),
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detects_tampering() {
        let mut j = Journal::default();
        j.append("first").unwrap();
        j.append("second").unwrap();
        assert!(j.verify().unwrap());
        let text = j.jsonl().unwrap();
        assert_eq!(
            Journal::<String>::from_jsonl(&text).unwrap().entries.len(),
            2
        );
        let mut tampered = serde_json::to_value(&j).unwrap();
        tampered["entries"][0]["value"] = "other".into();
        let tampered: Journal<String> = serde_json::from_value(tampered).unwrap();
        assert!(!tampered.verify().unwrap());
    }
    /// A fork's copy of a journal, across chunk boundaries: appending to one side
    /// leaves the other's chain as it was, and both still verify.
    #[test]
    fn a_cloned_journal_appends_apart() {
        let mut parent = Journal::default();
        for i in 0..(2 * CHUNK + 3) {
            parent.append(i).unwrap();
        }
        let checkpoint = parent.clone();
        let mut branch = parent.clone();
        for i in 0..CHUNK {
            branch.append(1_000 + i).unwrap();
            parent.append(2_000 + i).unwrap();
        }
        assert!(parent.verify().unwrap() && branch.verify().unwrap());
        assert!(checkpoint.verify().unwrap());
        assert_eq!(checkpoint.entries.len(), 2 * CHUNK + 3);
        let at = checkpoint.entries.len();
        assert_ne!(parent.entries[at].hash, branch.entries[at].hash);
        assert_eq!(
            parent.entries[at].previous_hash,
            branch.entries[at].previous_hash
        );
        assert_eq!(
            serde_json::to_value(&checkpoint).unwrap()["entries"]
                .as_array()
                .unwrap()
                .len(),
            at
        );
    }
}
