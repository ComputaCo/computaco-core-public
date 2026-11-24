//! An append-only sequence that a fork shares with its parent, and that either side
//! appends to in O(1) without copying what the other holds.

use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Records per sealed chunk. A clone copies the chunk spine (a pointer per chunk) and
/// the first append to a shared clone copies the open chunk, fewer than `CHUNK`
/// records; everything sealed stays shared for ever.
///
/// The chunk is small because a record can be large. An event of a page fetch carries
/// the page's body as an array of JSON numbers, 32 bytes for every byte of the page, and
/// with 256-record chunks the first step of a fork of the company world after 4,000
/// steps copied 9.55 MB of open chunk. Holding each record behind an `Arc` of its own
/// removed that copy but added an allocation per record, and a terminal step went from
/// 11.1 to 11.6 µs and dropping the agent-desktop world after 4,000 steps from 59.6 to
/// 64.2 ms. A short chunk adds one allocation per `CHUNK` records and bounds the copy
/// instead.
pub const CHUNK: usize = 16;

/// The records of a [`Chunks`], in order.
pub type Iter<'a, T> = std::iter::Chain<
    std::iter::FlatMap<
        std::slice::Iter<'a, Arc<Vec<T>>>,
        std::slice::Iter<'a, T>,
        fn(&'a Arc<Vec<T>>) -> std::slice::Iter<'a, T>,
    >,
    std::slice::Iter<'a, T>,
>;
/// The log: sealed chunks of exactly [`CHUNK`] records each, shared, and an open tail of
/// fewer. It is a flat sequence in every observable way: it iterates, indexes,
/// compares and serializes as the `Vec<T>` of its records, and deserializes from one.
#[derive(Clone, Debug)]
pub struct Chunks<T> {
    sealed: Vec<Arc<Vec<T>>>,
    sealed_len: usize,
    tail: Arc<Vec<T>>,
}
impl<T> Default for Chunks<T> {
    fn default() -> Self {
        Self {
            sealed: Vec::new(),
            sealed_len: 0,
            tail: Arc::new(Vec::new()),
        }
    }
}
impl<T> Chunks<T> {
    pub fn len(&self) -> usize {
        self.sealed_len + self.tail.len()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn iter(&self) -> Iter<'_, T> {
        fn records<T>(chunk: &Arc<Vec<T>>) -> std::slice::Iter<'_, T> {
            chunk.iter()
        }
        self.sealed
            .iter()
            .flat_map(records as fn(&Arc<Vec<T>>) -> std::slice::Iter<'_, T>)
            .chain(self.tail.iter())
    }
    pub fn get(&self, index: usize) -> Option<&T> {
        if index >= self.sealed_len {
            return self.tail.get(index - self.sealed_len);
        }
        let chunk = index / CHUNK;
        self.sealed.get(chunk)?.get(index - chunk * CHUNK)
    }
    pub fn first(&self) -> Option<&T> {
        self.get(0)
    }
    pub fn last(&self) -> Option<&T> {
        self.tail
            .last()
            .or_else(|| self.sealed.last().and_then(|chunk| chunk.last()))
    }
}
impl<T: Clone> Chunks<T> {
    pub fn push(&mut self, record: T) {
        // Only the open tail is ever copied-on-write, and it is bounded by CHUNK.
        let tail = Arc::make_mut(&mut self.tail);
        tail.push(record);
        if tail.len() >= CHUNK {
            self.sealed_len += tail.len();
            self.sealed.push(std::mem::replace(
                &mut self.tail,
                Arc::new(Vec::with_capacity(CHUNK)),
            ));
        }
    }
    pub fn to_vec(&self) -> Vec<T> {
        let mut out = Vec::with_capacity(self.len());
        out.extend(self.iter().cloned());
        out
    }
}
impl<T: Clone> From<Vec<T>> for Chunks<T> {
    fn from(records: Vec<T>) -> Self {
        let mut log = Self::default();
        for record in records {
            log.push(record);
        }
        log
    }
}
impl<T> std::ops::Index<usize> for Chunks<T> {
    type Output = T;
    fn index(&self, index: usize) -> &T {
        self.get(index).expect("index out of range")
    }
}
impl<'a, T> IntoIterator for &'a Chunks<T> {
    type Item = &'a T;
    type IntoIter = Iter<'a, T>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
/// Two logs are equal when they hold the same records in the same order; how they are
/// chunked is always the same for the same records.
impl<T: PartialEq> PartialEq for Chunks<T> {
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && self.iter().eq(other.iter())
    }
}
impl<T: Eq> Eq for Chunks<T> {}
impl<T: Serialize> Serialize for Chunks<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeSeq;
        let mut seq = serializer.serialize_seq(Some(self.len()))?;
        for record in self.iter() {
            seq.serialize_element(record)?;
        }
        seq.end()
    }
}
impl<'de, T: Deserialize<'de> + Clone> Deserialize<'de> for Chunks<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self::from(Vec::<T>::deserialize(deserializer)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// The chunking is invisible: order, indexing, length and the serialized form are
    /// those of one flat vector, across chunk boundaries and at every boundary case.
    #[test]
    fn a_log_is_its_records_in_order() {
        for count in [0, 1, CHUNK - 1, CHUNK, CHUNK + 1, 3 * CHUNK + 7] {
            let flat: Vec<u64> = (0..count as u64).collect();
            let log = Chunks::from(flat.clone());
            assert_eq!(log.len(), flat.len());
            assert_eq!(log.is_empty(), flat.is_empty());
            assert_eq!(log.to_vec(), flat);
            assert_eq!(log.iter().rev().copied().collect::<Vec<_>>(), {
                let mut r = flat.clone();
                r.reverse();
                r
            });
            assert_eq!(log.last(), flat.last());
            assert_eq!(log.first(), flat.first());
            for i in 0..flat.len() + 2 {
                assert_eq!(log.get(i), flat.get(i), "index {i} of {count}");
            }
            let json = serde_json::to_string(&log).unwrap();
            assert_eq!(json, serde_json::to_string(&flat).unwrap());
            let parsed: Chunks<u64> = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, log);
            assert_eq!(serde_json::to_string(&parsed).unwrap(), json);
        }
    }
    /// A clone shares every chunk; appending to either side copies at most the open
    /// one and leaves the other side's records alone.
    #[test]
    fn appending_to_a_shared_log_leaves_the_other_side_alone() {
        let mut parent = Chunks::from((0..(2 * CHUNK as u64 + 5)).collect::<Vec<_>>());
        let checkpoint = parent.clone();
        let mut branch = parent.clone();
        for i in 0..(CHUNK as u64 + 3) {
            branch.push(1_000 + i);
            parent.push(2_000 + i);
        }
        assert_eq!(
            checkpoint.to_vec(),
            (0..(2 * CHUNK as u64 + 5)).collect::<Vec<_>>()
        );
        assert_ne!(branch, parent);
        assert!(branch.iter().take(checkpoint.len()).eq(checkpoint.iter()));
        assert!(parent.iter().take(checkpoint.len()).eq(checkpoint.iter()));
        assert!(branch
            .sealed
            .iter()
            .zip(&checkpoint.sealed)
            .all(|(a, b)| Arc::ptr_eq(a, b)));
    }
}
/// A portable snapshot writes the log as one flat sequence, as it reads; decoding
/// rebuilds the chunking, which is always the same for the same records.
impl<T: cw_snapshot::Encode> cw_snapshot::Encode for Chunks<T> {
    fn encode(&self, w: &mut cw_snapshot::Writer) {
        w.len_prefix(self.len());
        for record in self.iter() {
            record.encode(w);
        }
    }
}
impl<T: cw_snapshot::Decode + Clone> cw_snapshot::Decode for Chunks<T> {
    fn decode(r: &mut cw_snapshot::Reader<'_>) -> Result<Self, cw_snapshot::Error> {
        Ok(Self::from(Vec::<T>::decode(r)?))
    }
}
