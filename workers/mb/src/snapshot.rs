//! R2 snapshot backend for the MB read API (phase 0 spike).
//!
//! Selected only when the Worker's `MB_BACKEND` var is `r2snapshot`; every
//! deployed environment leaves it unset and keeps the Hyperdrive repository.
//! The spike serves MBID lookups (artist, release, recording) and release
//! search from an immutable snapshot built by `examples/snapshot_build.rs`
//! out of the curated ETL output. Layout and measurement method:
//! `workers/mb/SNAPSHOT.md`.
//!
//! Snapshot objects under one prefix (`snap/<dump>/`):
//! - `<kind>.idx`: [`BUCKETS`] fixed entries of [`IDX_ENTRY_LEN`] bytes,
//!   little-endian `(part u32, len u32, offset u64)`; `len == 0` is an empty
//!   bucket. Cached within one immutable repository after the first read.
//! - `<kind>.dat.<part>`: concatenated raw-DEFLATE blocks, one per bucket.
//!   A block inflates to `key\tvalue\n` lines sorted by key. Parts stay
//!   under the single-request upload limit.
//!
//! Entity kinds bucket by the first four hex digits of the MBID (MBIDs are
//! random UUIDs, so buckets are even). The search kind buckets by FNV-1a of
//! the key: `t:<folded title words>` holds every release with that exact
//! normalized title, `w:<word>` the releases containing the word (capped,
//! shortest titles first). Hits carry display fields, so a search is one
//! parallel round of range reads with no per-hit fetch.

use crate::{Artist, Entity, Kind, MbRepo, Recording, Release, RepoError, RepoStage, SearchHit};
use serde::de::{DeserializeSeed, IgnoredAny, SeqAccess, Visitor};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;
use std::sync::OnceLock;
use unicode_normalization::UnicodeNormalization;

/// Buckets per kind: the first four hex digits of an MBID.
pub const BUCKETS: usize = 65_536;
/// One index entry: `part u32`, `len u32`, `offset u64` (little-endian).
pub const IDX_ENTRY_LEN: usize = 16;
/// Exact byte length of every `<kind>.idx` object.
pub const IDX_LEN: usize = BUCKETS * IDX_ENTRY_LEN;
/// Inflated block cap: a corrupt or hostile block cannot exhaust isolate
/// memory. The builder refuses to write a block above it.
pub const MAX_SNAPSHOT_BLOCK_BYTES: usize = 8 << 20;
/// Conservative compressed cap for the pinned level-6 miniz encoder. Its
/// static/stored fallback prevents sustained expansion beyond two bytes per
/// input byte; allow additional block headers and final padding. This is not
/// the inflated cap: incompressible DEFLATE can exceed the plaintext length.
pub const MAX_COMPRESSED_BLOCK_BYTES: usize = 2 * MAX_SNAPSHOT_BLOCK_BYTES + 1024;
/// Repository-local FIFO cache: at most two fixed-size indexes (2 MiB).
pub const MAX_CACHED_INDEXES: usize = 2;
/// `pg_trgm.similarity_threshold` default: the `%` predicate's floor.
pub const SEARCH_MIN_SCORE: f32 = 0.3;
/// Word keys fetched per search, longest words first (longer words are
/// rarer, so their postings are the most selective).
pub const MAX_QUERY_WORDS: usize = 4;
/// Releases kept per exact-title key: the deepest page the contract can
/// reach (`MAX_SEARCH_OFFSET` + `MAX_PAGE_SIZE`).
pub const MAX_TITLE_POSTINGS: usize = crate::MAX_SEARCH_OFFSET as usize + crate::MAX_PAGE_SIZE;
/// Releases kept per word key, shortest titles first.
pub const MAX_WORD_POSTINGS: usize = 2_000;
/// Upper bound for one `.dat` part object.
pub const MAX_PART_BYTES: u64 = 256 << 20;
/// Search index kind name (object stem).
pub const SEARCH_KIND: &str = "release-search";

const FOLD_TABLE: &str = include_str!("snapshot_fold.tsv");

/// Object stem per lookup kind.
pub fn kind_stem(kind: Kind) -> &'static str {
    kind.as_str()
}

pub fn idx_object(prefix: &str, stem: &str) -> String {
    format!("{prefix}{stem}.idx")
}

pub fn dat_object(prefix: &str, stem: &str, part: u32) -> String {
    format!("{prefix}{stem}.dat.{part:03}")
}

/// Bucket of an MBID: its first four hex digits. `None` for anything that
/// does not start with four hex digits (callers validate MBIDs first).
pub fn mbid_bucket(mbid: &str) -> Option<usize> {
    let head = mbid.as_bytes().get(..4)?;
    let mut bucket = 0usize;
    for &byte in head {
        bucket = bucket * 16 + (byte as char).to_digit(16)? as usize;
    }
    Some(bucket)
}

/// Bucket of a search key: 32-bit FNV-1a over the UTF-8 bytes, low 16 bits.
pub fn key_bucket(key: &str) -> usize {
    let mut hash: u32 = 0x811c_9dc5;
    for &byte in key.as_bytes() {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    (hash & 0xffff) as usize
}

/// Location of one bucket's block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockRef {
    pub part: u32,
    pub len: u32,
    pub offset: u64,
}

pub fn encode_idx_entry(block: BlockRef) -> [u8; IDX_ENTRY_LEN] {
    let mut entry = [0u8; IDX_ENTRY_LEN];
    entry[0..4].copy_from_slice(&block.part.to_le_bytes());
    entry[4..8].copy_from_slice(&block.len.to_le_bytes());
    entry[8..16].copy_from_slice(&block.offset.to_le_bytes());
    entry
}

/// The bucket's block, or `None` for an empty bucket or a short index.
pub fn idx_entry(idx: &[u8], bucket: usize) -> Option<BlockRef> {
    if bucket >= BUCKETS {
        return None;
    }
    let start = bucket.checked_mul(IDX_ENTRY_LEN)?;
    let end = start.checked_add(IDX_ENTRY_LEN)?;
    let entry = idx.get(start..end)?;
    let word = |range: std::ops::Range<usize>| -> [u8; 4] {
        entry[range].try_into().expect("4-byte slice")
    };
    let len = u32::from_le_bytes(word(4..8));
    if len == 0 {
        return None;
    }
    Some(BlockRef {
        part: u32::from_le_bytes(word(0..4)),
        len,
        offset: u64::from_le_bytes(entry[8..16].try_into().expect("8-byte slice")),
    })
}

pub fn deflate_block(plain: &[u8]) -> Vec<u8> {
    miniz_oxide::deflate::compress_to_vec(plain, 6)
}

/// Inflate one block, refusing anything above [`MAX_SNAPSHOT_BLOCK_BYTES`].
pub fn inflate_block(raw: &[u8]) -> Option<Vec<u8>> {
    miniz_oxide::inflate::decompress_to_vec_with_limit(raw, MAX_SNAPSHOT_BLOCK_BYTES).ok()
}

/// The value for `key` in an inflated block (`key\tvalue` lines). Blocks are
/// small (tens of KiB), so a linear scan is cheaper than parsing an index.
pub fn find_value<'a>(block: &'a str, key: &str) -> Option<&'a str> {
    block
        .split('\n')
        .filter_map(|line| line.split_once('\t'))
        .find_map(|(k, v)| (k == key).then_some(v))
}

fn fold_map() -> &'static HashMap<char, &'static str> {
    static MAP: OnceLock<HashMap<char, &'static str>> = OnceLock::new();
    MAP.get_or_init(|| {
        FOLD_TABLE
            .lines()
            .filter(|line| !line.starts_with('#'))
            .filter_map(|line| line.split_once('\t'))
            .filter_map(|(from, to)| {
                let mut chars = from.chars();
                let ch = chars.next()?;
                chars.next().is_none().then_some((ch, to))
            })
            .collect()
    })
}

/// Lowercase, canonically compose (NFC), then fold diacritics using
/// `snapshot_fold.tsv`. Equivalent combining sequences share index keys.
pub fn fold(text: &str) -> String {
    let map = fold_map();
    let mut out = String::with_capacity(text.len());
    for ch in text.to_lowercase().nfc() {
        match map.get(&ch) {
            Some(ascii) => out.push_str(ascii),
            None => out.push(ch),
        }
    }
    out
}

/// Folded words: maximal runs of alphanumeric characters, like `pg_trgm`.
pub fn words(text: &str) -> Vec<String> {
    split_words(&fold(text))
}

fn split_words(text: &str) -> Vec<String> {
    text.split(|ch: char| !ch.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Exact-title key for a title or query.
pub fn title_key(text: &str) -> String {
    format!("t:{}", words(text).join(" "))
}

pub fn word_key(word: &str) -> String {
    format!("w:{word}")
}

/// Index keys a query reads: its exact-title key plus up to
/// [`MAX_QUERY_WORDS`] distinct word keys, longest words first.
pub fn search_keys(normalized_q: &str) -> Vec<String> {
    let mut query_words = words(normalized_q);
    if query_words.is_empty() {
        return Vec::new();
    }
    let mut keys = vec![format!("t:{}", query_words.join(" "))];
    query_words.sort_by(|a, b| {
        b.chars()
            .count()
            .cmp(&a.chars().count())
            .then_with(|| a.cmp(b))
    });
    query_words.dedup();
    keys.extend(
        query_words
            .iter()
            .take(MAX_QUERY_WORDS)
            .map(|word| word_key(word)),
    );
    keys
}

/// `pg_trgm` trigrams of the folded text: each word padded with two
/// leading spaces and one trailing space, then every 3-character window,
/// deduplicated.
pub fn trigrams(text: &str) -> Vec<[char; 3]> {
    word_trigrams(words(text))
}

/// `pg_trgm` trigrams of the lowercased text without diacritic folding:
/// the set Postgres itself scores. Breaks ties between titles that fold
/// to the same words, so an exact accented match outranks its ASCII twin.
pub fn exact_trigrams(text: &str) -> Vec<[char; 3]> {
    let normalized: String = text.to_lowercase().nfc().collect();
    word_trigrams(split_words(&normalized))
}

fn word_trigrams(words: Vec<String>) -> Vec<[char; 3]> {
    let mut out = Vec::new();
    for word in words {
        let padded: Vec<char> = "  "
            .chars()
            .chain(word.chars())
            .chain(std::iter::once(' '))
            .collect();
        out.extend(padded.windows(3).map(|w| [w[0], w[1], w[2]]));
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// `pg_trgm` `similarity()`: shared trigrams over the union, on sorted sets.
pub fn similarity_of(a: &[[char; 3]], b: &[[char; 3]]) -> f32 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let (mut i, mut j, mut shared) = (0, 0, 0usize);
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => {
                shared += 1;
                i += 1;
                j += 1;
            }
        }
    }
    shared as f32 / (a.len() + b.len() - shared) as f32
}

pub fn similarity(a: &str, b: &str) -> f32 {
    similarity_of(&trigrams(a), &trigrams(b))
}

pub type LabelKey = (String, String, Vec<bool>, String);

/// Sort key for `label ASC` under the database collation (glibc
/// `en_US.UTF-8`, the Postgres image default): spaces and punctuation
/// ignored, then base letters, then accents, then lowercase before
/// uppercase. The raw label last keeps the order total. Byte order would
/// put every uppercase title before its lowercase twin.
pub fn label_sort_key(label: &str) -> LabelKey {
    let alnum: Vec<char> = label.nfc().filter(|ch| ch.is_alphanumeric()).collect();
    let lower = alnum.iter().collect::<String>().to_lowercase();
    (
        fold(&lower),
        lower,
        alnum.iter().map(|ch| ch.is_uppercase()).collect(),
        label.to_owned(),
    )
}

/// One release-search posting: `[mbid, title, artist_mbid, artist_name]`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Posting(
    pub String,
    pub String,
    pub Option<String>,
    pub Option<String>,
);

fn canonical_mbid(mbid: &str) -> Option<String> {
    crate::is_valid_mbid(mbid).then(|| mbid.to_ascii_lowercase())
}

fn canonical_related(mbid: Option<String>) -> Option<Option<String>> {
    match mbid {
        Some(mbid) => canonical_mbid(&mbid).map(Some),
        None => Some(None),
    }
}

fn canonical_posting(mut posting: Posting) -> Option<Posting> {
    posting.0 = canonical_mbid(&posting.0)?;
    posting.2 = canonical_related(posting.2)?;
    Some(posting)
}

struct PostingListSeed {
    cap: usize,
}

impl<'de> DeserializeSeed<'de> for PostingListSeed {
    type Value = Vec<Posting>;

    fn deserialize<D: serde::Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_seq(self)
    }
}

impl<'de> Visitor<'de> for PostingListSeed {
    type Value = Vec<Posting>;

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a bounded list of release postings")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let mut postings = Vec::new();
        while postings.len() < self.cap {
            let Some(posting) = seq.next_element::<Posting>()? else {
                return Ok(postings);
            };
            postings.push(canonical_posting(posting).ok_or_else(|| {
                <A::Error as serde::de::Error>::custom("invalid posting identifier")
            })?);
        }
        // Never allocate/retain the over-cap posting, even when it repeats
        // an existing ID. Deduplication is a later ranking operation.
        if seq.next_element::<IgnoredAny>()?.is_some() {
            return Err(serde::de::Error::custom("posting limit exceeded"));
        }
        Ok(postings)
    }
}

fn decode_postings(key: &str, value: &str) -> Result<Vec<Posting>, RepoError> {
    let cap = if key.starts_with("t:") {
        MAX_TITLE_POSTINGS
    } else if key.starts_with("w:") {
        MAX_WORD_POSTINGS
    } else {
        return Err(decode_failed());
    };
    let mut deserializer = serde_json::Deserializer::from_str(value);
    let postings = PostingListSeed { cap }
        .deserialize(&mut deserializer)
        .map_err(|_| decode_failed())?;
    deserializer.end().map_err(|_| decode_failed())?;
    Ok(postings)
}

/// Score, filter and page postings the way the Postgres release search
/// orders rows: similarity desc, then title asc ([`label_sort_key`]), then
/// MBID asc, with the `%` floor ([`SEARCH_MIN_SCORE`]). The folded score
/// ranks first; equal folded scores fall back to the unfolded `pg_trgm`
/// score ([`exact_trigrams`]) before the title. Duplicate MBIDs (a release
/// under its title key and its word keys) count once.
pub fn rank_postings(
    normalized_q: &str,
    postings: impl IntoIterator<Item = Posting>,
    limit: u32,
    offset: u32,
) -> Result<Vec<SearchHit>, RepoError> {
    // Reject a corrupt list as a whole, before sorting or pagination can
    // conceal a bad identifier in a later page or a below-threshold row.
    let postings: Vec<Posting> = postings
        .into_iter()
        .map(|posting| canonical_posting(posting).ok_or_else(decode_failed))
        .collect::<Result<_, _>>()?;
    let query = trigrams(normalized_q);
    let exact_query = exact_trigrams(normalized_q);
    let mut seen = HashSet::new();
    let mut scored: Vec<(f32, f32, LabelKey, Posting)> = postings
        .into_iter()
        .filter(|posting| seen.insert(posting.0.clone()))
        .map(|posting| (similarity_of(&query, &trigrams(&posting.1)), posting))
        .filter(|(score, _)| *score >= SEARCH_MIN_SCORE)
        .map(|(score, posting)| {
            let exact = similarity_of(&exact_query, &exact_trigrams(&posting.1));
            (score, exact, label_sort_key(&posting.1), posting)
        })
        .collect();
    scored.sort_by(|(sa, ea, ka, pa), (sb, eb, kb, pb)| {
        sb.total_cmp(sa)
            .then_with(|| eb.total_cmp(ea))
            .then_with(|| ka.cmp(kb))
            .then_with(|| pa.0.cmp(&pb.0))
    });
    Ok(scored
        .into_iter()
        .skip(offset as usize)
        .take(limit as usize)
        .map(
            |(score, _, _, Posting(mbid, title, artist_mbid, artist_name))| SearchHit {
                mbid,
                label: title,
                score,
                artist_mbid,
                artist_name,
            },
        )
        .collect())
}

// --- Stored row shapes ------------------------------------------------------
// The lookup DTOs only serialize (their shapes are frozen by the cache
// compatibility suite), so the snapshot stores these mirrors keyed by MBID.

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArtistRow {
    pub name: String,
    pub sort_name: String,
    #[serde(rename = "type")]
    pub artist_type: Option<String>,
    pub country: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReleaseRow {
    pub artist_mbid: Option<String>,
    pub title: String,
    pub release_type: Option<String>,
    pub year: Option<i16>,
    pub caa_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecordingRow {
    pub release_mbid: Option<String>,
    pub title: String,
    pub track_no: Option<i16>,
    pub duration_ms: Option<i32>,
}

/// Decode one stored lookup value into the served entity.
pub fn decode_entity(kind: Kind, mbid: &str, value: &str) -> Option<Entity> {
    let mbid = canonical_mbid(mbid)?;
    Some(match kind {
        Kind::Artist => {
            let row: ArtistRow = serde_json::from_str(value).ok()?;
            Entity::Artist(Artist {
                mbid,
                name: row.name,
                sort_name: row.sort_name,
                artist_type: row.artist_type,
                country: row.country,
            })
        }
        Kind::Release => {
            let row: ReleaseRow = serde_json::from_str(value).ok()?;
            Entity::Release(Release {
                mbid,
                artist_mbid: canonical_related(row.artist_mbid)?,
                title: row.title,
                release_type: row.release_type,
                year: row.year,
                caa_url: row.caa_url,
            })
        }
        Kind::Recording => {
            let row: RecordingRow = serde_json::from_str(value).ok()?;
            Entity::Recording(Recording {
                mbid,
                release_mbid: canonical_related(row.release_mbid)?,
                title: row.title,
                track_no: row.track_no,
                duration_ms: row.duration_ms,
                medium_no: None,
            })
        }
    })
}

// --- Repository ---------------------------------------------------------------

/// Immutable byte-range object reads: R2 in the Worker, files in native tools.
/// Implementations must reject wrong metadata before buffering, bound retained
/// bodies while reading, and never change objects during a repository's life.
#[allow(async_fn_in_trait)]
pub trait RangeSource {
    /// Exactly `expected_len` bytes; `None` when the object does not exist.
    /// Reject a different object size before consuming its body.
    async fn read_all_exact(
        &self,
        key: &str,
        expected_len: usize,
    ) -> Result<Option<Vec<u8>>, RepoError>;
    /// `len` bytes at `offset`; `None` when the object does not exist.
    /// Bound the response body before materialization, not only afterwards.
    async fn read_range(
        &self,
        key: &str,
        offset: u64,
        len: u32,
    ) -> Result<Option<Vec<u8>>, RepoError>;
}

/// Check metadata before opening a body, then retain no more than the
/// promised length even if metadata and streamed bytes disagree.
async fn read_sized_body<S, E>(
    size: u64,
    expected_len: usize,
    open_body: impl FnOnce() -> Result<S, RepoError>,
) -> Result<Vec<u8>, RepoError>
where
    S: futures_util::Stream<Item = Result<Vec<u8>, E>>,
{
    use futures_util::StreamExt;
    if size != expected_len as u64 {
        return Err(decode_failed());
    }
    let stream = open_body()?;
    futures_util::pin_mut!(stream);
    let mut bytes = Vec::with_capacity(expected_len);
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| query_failed())?;
        if chunk.len() > expected_len - bytes.len() {
            return Err(decode_failed());
        }
        bytes.extend_from_slice(&chunk);
    }
    if bytes.len() != expected_len {
        return Err(decode_failed());
    }
    Ok(bytes)
}

fn checked_block_end(offset: u64, len: u32) -> Result<u64, RepoError> {
    if len == 0 || len as usize > MAX_COMPRESSED_BLOCK_BYTES {
        return Err(decode_failed());
    }
    offset
        .checked_add(u64::from(len))
        .filter(|end| *end <= MAX_PART_BYTES)
        .ok_or_else(decode_failed)
}

fn query_failed() -> RepoError {
    RepoError::new(RepoStage::Query)
}

fn configured_prefix<E>(prefix: Result<String, E>) -> Result<String, RepoError> {
    let prefix = prefix.map_err(|_| query_failed())?;
    if prefix.trim().is_empty() {
        return Err(query_failed());
    }
    Ok(prefix)
}

fn decode_failed() -> RepoError {
    RepoError::new(RepoStage::Decode)
}

/// [`MbRepo`] over one immutable source and nonempty snapshot prefix.
/// Index reuse is bounded and repository-local; no global store identity or
/// stale cross-store entry survives dropping this repository.
pub struct SnapshotRepo<S> {
    source: S,
    prefix: String,
    indexes: RefCell<VecDeque<(String, Rc<Vec<u8>>)>>,
    reads: std::cell::Cell<u32>,
}

impl<S: RangeSource> SnapshotRepo<S> {
    pub fn new(source: S, prefix: impl Into<String>) -> Result<Self, RepoError> {
        let prefix = configured_prefix(Ok::<_, ()>(prefix.into()))?;
        Ok(SnapshotRepo {
            source,
            prefix,
            indexes: RefCell::new(VecDeque::new()),
            reads: std::cell::Cell::new(0),
        })
    }

    /// Object reads issued so far (index loads included).
    pub fn reads(&self) -> u32 {
        self.reads.get()
    }

    async fn index(&self, stem: &str) -> Result<Rc<Vec<u8>>, RepoError> {
        let key = idx_object(&self.prefix, stem);
        if let Some((_, hit)) = self.indexes.borrow().iter().find(|(name, _)| name == &key) {
            return Ok(hit.clone());
        }
        self.reads.set(self.reads.get() + 1);
        let bytes = self
            .source
            .read_all_exact(&key, IDX_LEN)
            .await?
            .ok_or_else(query_failed)?;
        if bytes.len() != IDX_LEN {
            return Err(decode_failed());
        }
        let bytes = Rc::new(bytes);
        let mut indexes = self.indexes.borrow_mut();
        // A concurrent read may already have populated this entry.
        if let Some((_, hit)) = indexes.iter().find(|(name, _)| name == &key) {
            return Ok(hit.clone());
        }
        if indexes.len() == MAX_CACHED_INDEXES {
            indexes.pop_front();
        }
        indexes.push_back((key, bytes.clone()));
        Ok(bytes)
    }

    /// The inflated block of one bucket; `None` for an empty bucket.
    async fn block(&self, stem: &str, bucket: usize) -> Result<Option<String>, RepoError> {
        let idx = self.index(stem).await?;
        let Some(block) = idx_entry(&idx, bucket) else {
            return Ok(None);
        };
        checked_block_end(block.offset, block.len)?;
        self.reads.set(self.reads.get() + 1);
        let raw = self
            .source
            .read_range(
                &dat_object(&self.prefix, stem, block.part),
                block.offset,
                block.len,
            )
            .await?
            .ok_or_else(query_failed)?;
        if raw.len() != block.len as usize {
            return Err(decode_failed());
        }
        let plain = inflate_block(&raw).ok_or_else(decode_failed)?;
        String::from_utf8(plain)
            .map(Some)
            .map_err(|_| decode_failed())
    }

    /// Release-search postings for every key of the query, read in one
    /// parallel round (one read per distinct bucket).
    pub async fn search_postings(&self, normalized_q: &str) -> Result<Vec<Posting>, RepoError> {
        let keys = search_keys(normalized_q);
        let mut buckets: Vec<usize> = keys.iter().map(|key| key_bucket(key)).collect();
        buckets.sort_unstable();
        buckets.dedup();
        // Load the index first so the parallel block reads share it.
        self.index(SEARCH_KIND).await?;
        let blocks = futures_util::future::join_all(
            buckets
                .iter()
                .map(|&bucket| self.block(SEARCH_KIND, bucket)),
        )
        .await;
        let mut by_bucket = HashMap::new();
        for (bucket, block) in buckets.into_iter().zip(blocks) {
            if let Some(block) = block? {
                by_bucket.insert(bucket, block);
            }
        }
        let mut postings = Vec::new();
        for key in &keys {
            let Some(block) = by_bucket.get(&key_bucket(key)) else {
                continue;
            };
            if let Some(value) = find_value(block, key) {
                postings.extend(decode_postings(key, value)?);
            }
        }
        Ok(postings)
    }
}

impl<S: RangeSource> MbRepo for SnapshotRepo<S> {
    async fn lookup(&self, kind: Kind, mbid: &str) -> Result<Option<Entity>, RepoError> {
        let mbid = canonical_mbid(mbid).ok_or_else(query_failed)?;
        let bucket = mbid_bucket(&mbid).ok_or_else(query_failed)?;
        let Some(block) = self.block(kind_stem(kind), bucket).await? else {
            return Ok(None);
        };
        match find_value(&block, &mbid) {
            None => Ok(None),
            Some(value) => decode_entity(kind, &mbid, value)
                .map(Some)
                .ok_or_else(decode_failed),
        }
    }

    async fn search(
        &self,
        kind: Kind,
        normalized_q: &str,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<SearchHit>, RepoError> {
        // The spike indexes release search only (the ingest matcher's
        // query); artist/recording search stay on the Postgres backend.
        if kind != Kind::Release {
            return Err(query_failed());
        }
        let postings = self.search_postings(normalized_q).await?;
        rank_postings(normalized_q, postings, limit, offset)
    }

    async fn release_recordings(
        &self,
        _release_mbid: &str,
    ) -> Result<Option<Vec<Recording>>, RepoError> {
        // Out of the spike's scope (lookups plus release search).
        Err(query_failed())
    }

    async fn release_group(&self, _mbid: &str) -> Result<Option<crate::ReleaseGroup>, RepoError> {
        Err(query_failed())
    }

    async fn group_releases(&self, _mbid: &str) -> Result<Option<Vec<Release>>, RepoError> {
        Err(query_failed())
    }

    async fn artist_urls(&self, _mbid: &str) -> Result<Option<Vec<crate::ArtistUrl>>, RepoError> {
        Err(query_failed())
    }

    async fn recording_videos(
        &self,
        _mbid: &str,
    ) -> Result<Option<Vec<crate::RecordingVideo>>, RepoError> {
        Err(query_failed())
    }

    async fn recording_isrcs(&self, _mbid: &str) -> Result<Option<Vec<String>>, RepoError> {
        Err(query_failed())
    }
}

// --- Worker entry (spike) ---------------------------------------------------

pub use worker_entry::spike_fetch;

/// Worker glue. Compiles natively like `http_layer` (the worker types only
/// bind at runtime in the isolate); the logic above is what tests cover.
mod worker_entry {
    use super::*;
    use crate::{handle_lookup, handle_search_paged, ApiResponse, SuccessCache};
    use worker::{Bucket, Env, Headers, Request, Response, ResponseBody, Result as WorkerResult};

    /// R2 binding holding the snapshot objects.
    pub const SNAPSHOT_BINDING: &str = "MB_SNAPSHOT";
    /// Var naming the snapshot prefix (`snap/<dump>/`).
    pub const SNAPSHOT_PREFIX_VAR: &str = "MB_SNAPSHOT_PREFIX";
    /// Secret every spike request must present in [`BENCH_HEADER`]. The
    /// spike Worker exists only for one measurement job; without the secret
    /// it serves nothing but `/healthz`.
    pub const BENCH_TOKEN_SECRET: &str = "MB_SNAPSHOT_BENCH_TOKEN";
    pub const BENCH_HEADER: &str = "x-mb-snapshot-bench";

    pub struct R2Source {
        bucket: Bucket,
    }

    impl RangeSource for R2Source {
        async fn read_all_exact(
            &self,
            key: &str,
            expected_len: usize,
        ) -> Result<Option<Vec<u8>>, RepoError> {
            if expected_len != IDX_LEN {
                return Err(decode_failed());
            }
            let Some(object) = self
                .bucket
                .get(key)
                .execute()
                .await
                .map_err(|_| query_failed())?
            else {
                return Ok(None);
            };
            read_sized_body(object.size(), expected_len, || {
                object
                    .body()
                    .ok_or_else(query_failed)?
                    .stream()
                    .map_err(|_| query_failed())
            })
            .await
            .map(Some)
        }

        async fn read_range(
            &self,
            key: &str,
            offset: u64,
            len: u32,
        ) -> Result<Option<Vec<u8>>, RepoError> {
            let end = checked_block_end(offset, len)?;
            let range = worker::Range::OffsetWithLength {
                offset,
                length: u64::from(len),
            };
            let Some(object) = self
                .bucket
                .get(key)
                .range(range)
                .execute()
                .await
                .map_err(|_| query_failed())?
            else {
                return Ok(None);
            };
            if object.size() > MAX_PART_BYTES || end > object.size() {
                return Err(decode_failed());
            }
            match object.range().map_err(|_| decode_failed())? {
                worker::Range::OffsetWithLength {
                    offset: actual_offset,
                    length,
                } if actual_offset == offset && length == u64::from(len) => {}
                _ => return Err(decode_failed()),
            }
            read_sized_body(u64::from(len), len as usize, || {
                object
                    .body()
                    .ok_or_else(query_failed)?
                    .stream()
                    .map_err(|_| query_failed())
            })
            .await
            .map(Some)
        }
    }

    /// The spike measures uncached reads: no KV, no edge cache.
    struct NoCache;

    impl SuccessCache for NoCache {
        async fn get(&self, _key: &str) -> Option<String> {
            None
        }
        async fn put(&self, _key: &str, _body: &str, _ttl_s: u32) {}
    }

    fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
        a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
    }

    fn render(api: ApiResponse, started: f64, reads: u32) -> WorkerResult<Response> {
        let headers = Headers::new();
        headers.set("content-type", "application/json")?;
        headers.set("cache-control", "no-store")?;
        let elapsed = js_sys_now() - started;
        headers.set(
            "server-timing",
            &format!("snapshot;dur={elapsed:.1};desc=\"reads={reads}\""),
        )?;
        Ok(
            Response::from_body(ResponseBody::Body(api.body.into_bytes()))?
                .with_status(api.status)
                .with_headers(headers),
        )
    }

    fn js_sys_now() -> f64 {
        worker::Date::now().as_millis() as f64
    }

    fn plain(status: u16, body: &str) -> WorkerResult<Response> {
        let headers = Headers::new();
        headers.set("content-type", "application/json")?;
        headers.set("cache-control", "no-store")?;
        Ok(
            Response::from_body(ResponseBody::Body(body.as_bytes().to_vec()))?
                .with_status(status)
                .with_headers(headers),
        )
    }

    pub(super) fn spike_route_error(method: worker::Method, path: &[&str]) -> Option<ApiResponse> {
        if method != worker::Method::Get {
            return Some(crate::error_response(crate::ApiError::not_found(
                "not found",
            )));
        }
        match path {
            ["v1", "mb", "release", mbid, "recordings"]
            | ["v1", "mb", "release-group", mbid, "releases"]
            | ["v1", "mb", "artist", mbid, "urls"]
            | ["v1", "mb", "recording", mbid, "videos" | "isrcs"] => {
                let error = if crate::is_valid_mbid(mbid) {
                    crate::ApiError::internal_error()
                } else {
                    crate::ApiError::invalid_request("invalid mbid (must be UUID)")
                };
                Some(crate::error_response(error))
            }
            _ => None,
        }
    }

    pub(super) fn spike_path(path: &str) -> &str {
        path.strip_prefix("/__mb-snap")
            .filter(|rest| rest.is_empty() || rest.starts_with('/'))
            .unwrap_or(path)
    }

    #[derive(Debug, PartialEq, Eq)]
    pub(super) enum SpikeRoute<'a> {
        Search,
        Lookup(Kind, &'a str),
    }

    pub(super) fn spike_route<'a>(path: &[&'a str]) -> Option<SpikeRoute<'a>> {
        match path {
            ["v1", "mb", "search"] => Some(SpikeRoute::Search),
            ["v1", "mb", kind, mbid] => {
                Kind::parse(Some((*kind).to_owned())).map(|kind| SpikeRoute::Lookup(kind, mbid))
            }
            _ => None,
        }
    }

    /// Spike router: lookups and release search over the snapshot, with the
    /// same core handlers (validation, envelopes, error bodies) as the
    /// Postgres routes. Paths may carry a leading `/__mb-snap` segment.
    pub async fn spike_fetch(req: Request, env: Env) -> WorkerResult<Response> {
        use crate::http_layer::{
            build_sha_from_env, emit, inbound_trace_id, outcome_level, with_trace,
        };
        let Ok(trace_id) = inbound_trace_id(&req) else {
            return Response::error("Service unavailable", 503);
        };
        let build_sha = build_sha_from_env(&env);
        let response = spike_response(req, env).await?;
        emit(
            outcome_level(response.status_code()),
            &trace_id,
            "mb.snapshot.completed",
            serde_json::json!({"status_code": response.status_code(), "build_sha": build_sha}),
        );
        Ok(with_trace(response, &trace_id))
    }

    async fn spike_response(req: Request, env: Env) -> WorkerResult<Response> {
        let url = req.url()?;
        let path = spike_path(url.path());
        if path == "/healthz" && req.method() == worker::Method::Get {
            return plain(200, r#"{"ok":true}"#);
        }
        let expected = env
            .secret(BENCH_TOKEN_SECRET)
            .map(|secret| secret.to_string())
            .unwrap_or_default();
        let presented = req.headers().get(BENCH_HEADER)?.unwrap_or_default();
        if expected.is_empty() || !constant_time_eq(expected.as_bytes(), presented.as_bytes()) {
            return plain(
                404,
                r#"{"code":"not_found","message":"not found","retryable":false}"#,
            );
        }
        let started = js_sys_now();
        let segments: Vec<&str> = path.trim_matches('/').split('/').collect();
        if let Some(api) = spike_route_error(req.method(), &segments) {
            return render(api, started, 0);
        }
        // Unknown routes never acquire bindings or depend on snapshot config.
        let Some(route) = spike_route(&segments) else {
            return render(
                crate::error_response(crate::ApiError::not_found("not found")),
                started,
                0,
            );
        };
        let prefix = match configured_prefix(env.var(SNAPSHOT_PREFIX_VAR).map(|v| v.to_string())) {
            Ok(prefix) => prefix,
            Err(_) => {
                return render(
                    crate::error_response(crate::ApiError::internal_error()),
                    started,
                    0,
                )
            }
        };
        let bucket = match env.bucket(SNAPSHOT_BINDING) {
            Ok(bucket) => bucket,
            Err(_) => {
                return render(
                    crate::error_response(crate::ApiError::internal_error()),
                    started,
                    0,
                )
            }
        };
        let repo = match SnapshotRepo::new(R2Source { bucket }, prefix) {
            Ok(repo) => repo,
            Err(_) => {
                return render(
                    crate::error_response(crate::ApiError::internal_error()),
                    started,
                    0,
                )
            }
        };
        let api = match route {
            SpikeRoute::Search => {
                let query: HashMap<String, String> = url.query_pairs().into_owned().collect();
                let limit = query.get("limit").and_then(|v| v.parse::<u32>().ok());
                let offset = query.get("offset").and_then(|v| v.parse::<u32>().ok());
                let raw_q = query.get("q").map(String::as_str).unwrap_or("");
                handle_search_paged(
                    &repo,
                    &NoCache,
                    query.get("type").cloned(),
                    raw_q,
                    limit,
                    offset,
                )
                .await
            }
            SpikeRoute::Lookup(kind, mbid) => handle_lookup(&repo, &NoCache, kind, mbid).await,
        };
        render(api, started, repo.reads())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::task::{Context, Poll, Waker};

    #[test]
    fn spike_route_errors_preserve_method_and_listing_contract() {
        use worker::Method;
        use worker_entry::spike_route_error;
        let lookup = ["v1", "mb", "artist", "id"];
        assert!(spike_route_error(Method::Get, &lookup).is_none());
        for method in [Method::Post, Method::Put, Method::Delete, Method::Head] {
            assert_eq!(spike_route_error(method, &lookup).unwrap().status, 404);
        }
        for (kind, listing) in [
            ("release", "recordings"),
            ("release-group", "releases"),
            ("artist", "urls"),
            ("recording", "videos"),
            ("recording", "isrcs"),
        ] {
            for mbid in [
                "123e4567-e89b-12d3-a456-426614174000",
                "123E4567-E89B-12D3-A456-426614174000",
            ] {
                let api =
                    spike_route_error(Method::Get, &["v1", "mb", kind, mbid, listing]).unwrap();
                assert_eq!(api.status, 500);
                assert!(api.body.contains("internal_error"));
            }
            for mbid in ["not-a-uuid", "", "123e4567-e89b-12d3-a456-42661417400"] {
                let api =
                    spike_route_error(Method::Get, &["v1", "mb", kind, mbid, listing]).unwrap();
                assert_eq!(api.status, 400);
                assert_eq!(
                    serde_json::from_str::<serde_json::Value>(&api.body).unwrap(),
                    serde_json::json!({
                        "code": "invalid_request",
                        "message": "invalid mbid (must be UUID)",
                        "retryable": false,
                    })
                );
                assert_eq!(
                    spike_route_error(Method::Post, &["v1", "mb", kind, mbid, listing])
                        .unwrap()
                        .status,
                    404
                );
            }
        }
        assert!(spike_route_error(Method::Get, &["v1", "mb", "artist", "id", "unknown"]).is_none());
    }

    #[test]
    fn spike_paths_require_a_prefix_segment_and_classify_before_bindings() {
        use worker_entry::{spike_path, spike_route, SpikeRoute};
        for prefix in ["", "/__mb-snap"] {
            let search = format!("{prefix}/v1/mb/search");
            let segments: Vec<_> = spike_path(&search).trim_matches('/').split('/').collect();
            assert_eq!(spike_route(&segments), Some(SpikeRoute::Search));
            let lookup = format!("{prefix}/v1/mb/artist/id");
            let segments: Vec<_> = spike_path(&lookup).trim_matches('/').split('/').collect();
            assert_eq!(
                spike_route(&segments),
                Some(SpikeRoute::Lookup(Kind::Artist, "id"))
            );
            assert_eq!(spike_path(&format!("{prefix}/healthz")), "/healthz");
        }
        for path in [
            "/__mb-snapv1/mb/search",
            "/__mb-snap-other/v1/mb/artist/id",
            "/v1/mb/unknown/id",
            "/unknown",
        ] {
            let segments: Vec<_> = spike_path(path).trim_matches('/').split('/').collect();
            assert_eq!(spike_route(&segments), None, "{path}");
        }
        let source = include_str!("snapshot.rs");
        let router = source
            .split_once("pub async fn spike_fetch(")
            .unwrap()
            .1
            .split_once("#[cfg(test)]")
            .unwrap()
            .0;
        let classified = router.find("let Some(route) = spike_route(").unwrap();
        assert!(classified < router.find("configured_prefix(env.var(").unwrap());
        assert!(classified < router.find("env.bucket(").unwrap());
        assert!(router.contains("Err(_) => {\n                return render("));
        assert!(!router.contains("env.bucket(SNAPSHOT_BINDING)?"));
    }

    #[test]
    fn unsupported_snapshot_methods_fail_without_store_reads() {
        let repo = SnapshotRepo::new(MemSource::default(), "snap/unsupported/").unwrap();
        let id = "123e4567-e89b-12d3-a456-426614174000";
        assert_eq!(
            block_on(repo.release_recordings(id)).unwrap_err().stage(),
            RepoStage::Query
        );
        assert_eq!(
            block_on(repo.release_group(id)).unwrap_err().stage(),
            RepoStage::Query
        );
        assert_eq!(
            block_on(repo.group_releases(id)).unwrap_err().stage(),
            RepoStage::Query
        );
        assert_eq!(
            block_on(repo.artist_urls(id)).unwrap_err().stage(),
            RepoStage::Query
        );
        assert_eq!(
            block_on(repo.recording_videos(id)).unwrap_err().stage(),
            RepoStage::Query
        );
        assert_eq!(
            block_on(repo.recording_isrcs(id)).unwrap_err().stage(),
            RepoStage::Query
        );
        assert_eq!(repo.source.body_reads.get(), 0);
        assert_eq!(repo.source.range_reads.get(), 0);
    }

    fn block_on<F: Future>(future: F) -> F::Output {
        let mut cx = Context::from_waker(Waker::noop());
        let mut future = Box::pin(future);
        for _ in 0..1000 {
            if let Poll::Ready(value) = future.as_mut().poll(&mut cx) {
                return value;
            }
        }
        panic!("test future did not complete");
    }

    /// In-memory objects, built with the same writer helpers the builder uses.
    #[derive(Default)]
    struct MemSource {
        objects: HashMap<String, Vec<u8>>,
        body_reads: std::cell::Cell<usize>,
        range_reads: std::cell::Cell<usize>,
    }

    impl RangeSource for MemSource {
        async fn read_all_exact(
            &self,
            key: &str,
            expected_len: usize,
        ) -> Result<Option<Vec<u8>>, RepoError> {
            let Some(bytes) = self.objects.get(key) else {
                return Ok(None);
            };
            if bytes.len() != expected_len {
                return Err(decode_failed());
            }
            self.body_reads.set(self.body_reads.get() + 1);
            Ok(Some(bytes.clone()))
        }
        async fn read_range(
            &self,
            key: &str,
            offset: u64,
            len: u32,
        ) -> Result<Option<Vec<u8>>, RepoError> {
            self.range_reads.set(self.range_reads.get() + 1);
            checked_block_end(offset, len)?;
            let Some(bytes) = self.objects.get(key) else {
                return Ok(None);
            };
            let start = usize::try_from(offset).map_err(|_| decode_failed())?;
            let end = start.checked_add(len as usize).ok_or_else(decode_failed)?;
            bytes
                .get(start..end)
                .map(|bytes| Some(bytes.to_vec()))
                .ok_or_else(decode_failed)
        }
    }

    /// Write one kind: lines grouped by bucket, sorted by key, one part.
    fn write_kind(
        source: &mut MemSource,
        prefix: &str,
        stem: &str,
        lines: &[(String, String)],
        bucket_of: impl Fn(&str) -> usize,
    ) {
        let mut by_bucket: std::collections::BTreeMap<usize, Vec<&(String, String)>> =
            Default::default();
        for line in lines {
            by_bucket.entry(bucket_of(&line.0)).or_default().push(line);
        }
        let mut idx = vec![0u8; IDX_LEN];
        let mut dat = Vec::new();
        for (bucket, mut entries) in by_bucket {
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            let plain: String = entries.iter().map(|(k, v)| format!("{k}\t{v}\n")).collect();
            let raw = deflate_block(plain.as_bytes());
            let block = BlockRef {
                part: 0,
                len: raw.len() as u32,
                offset: dat.len() as u64,
            };
            idx[bucket * IDX_ENTRY_LEN..(bucket + 1) * IDX_ENTRY_LEN]
                .copy_from_slice(&encode_idx_entry(block));
            dat.extend(raw);
        }
        source.objects.insert(idx_object(prefix, stem), idx);
        source.objects.insert(dat_object(prefix, stem, 0), dat);
    }

    const ARTIST: &str = "0383dadf-2a4e-4d10-a46a-e9e041da8eb3";
    const RELEASE_A: &str = "1e6a0e9b-0000-4000-8000-000000000001";
    const RELEASE_B: &str = "1e6a0e9b-0000-4000-8000-000000000002";
    const RELEASE_C: &str = "ffff0e9b-0000-4000-8000-000000000003";

    fn fixture(prefix: &str) -> SnapshotRepo<MemSource> {
        let mut source = MemSource::default();
        let artist = ArtistRow {
            name: "Björk".into(),
            sort_name: "Björk".into(),
            artist_type: Some("Person".into()),
            country: Some("IS".into()),
        };
        write_kind(
            &mut source,
            prefix,
            "artist",
            &[(ARTIST.into(), serde_json::to_string(&artist).unwrap())],
            |k| mbid_bucket(k).unwrap(),
        );
        let release = |title: &str| ReleaseRow {
            artist_mbid: Some(ARTIST.into()),
            title: title.into(),
            release_type: Some("Album".into()),
            year: Some(1997),
            caa_url: None,
        };
        write_kind(
            &mut source,
            prefix,
            "release",
            &[
                (
                    RELEASE_A.into(),
                    serde_json::to_string(&release("Homogenic")).unwrap(),
                ),
                (
                    RELEASE_C.into(),
                    serde_json::to_string(&release("Début")).unwrap(),
                ),
            ],
            |k| mbid_bucket(k).unwrap(),
        );
        let posting = |mbid: &str, title: &str| {
            Posting(
                mbid.into(),
                title.into(),
                Some(ARTIST.into()),
                Some("Björk".into()),
            )
        };
        let mut search: HashMap<String, Vec<Posting>> = HashMap::new();
        for p in [
            posting(RELEASE_A, "Homogenic"),
            posting(RELEASE_B, "Homogenic (Live)"),
            posting(RELEASE_C, "Début"),
        ] {
            search.entry(title_key(&p.1)).or_default().push(p.clone());
            for word in words(&p.1) {
                search.entry(word_key(&word)).or_default().push(p.clone());
            }
        }
        let lines: Vec<(String, String)> = search
            .into_iter()
            .map(|(k, v)| (k, serde_json::to_string(&v).unwrap()))
            .collect();
        write_kind(&mut source, prefix, SEARCH_KIND, &lines, key_bucket);
        SnapshotRepo::new(source, prefix).unwrap()
    }

    #[test]
    fn buckets_are_stable_across_languages() {
        // The Python-free builder and the Worker share this code; these
        // vectors pin the format so a later port cannot drift silently.
        assert_eq!(mbid_bucket("0383dadf-2a4e"), Some(0x0383));
        assert_eq!(mbid_bucket("FFFF"), Some(0xffff));
        assert_eq!(mbid_bucket("zz00"), None);
        assert_eq!(mbid_bucket("ab"), None);
        assert_eq!(key_bucket(""), 0x9dc5);
        assert_eq!(key_bucket("t:homogenic"), key_bucket("t:homogenic"));
        assert!(key_bucket("w:a") < BUCKETS);
        // Independently calculated FNV-1a values, including UTF-8 bytes.
        assert_eq!(key_bucket("t:homogenic"), 0x42dc);
        assert_eq!(key_bucket("w:a"), 0xfe5f);
        assert_eq!(key_bucket("t:東京"), 0x835d);
    }

    #[test]
    fn idx_entries_round_trip_and_empty_is_none() {
        let block = BlockRef {
            part: 3,
            len: 77,
            offset: (5u64 << 32) + 9,
        };
        let mut idx = vec![0u8; IDX_LEN];
        idx[16..32].copy_from_slice(&encode_idx_entry(block));
        assert_eq!(idx_entry(&idx, 1), Some(block));
        assert_eq!(idx_entry(&idx, 0), None);
        assert_eq!(idx_entry(&idx, BUCKETS), None);
    }

    #[test]
    fn inflate_refuses_oversized_blocks() {
        let big = vec![b'a'; MAX_SNAPSHOT_BLOCK_BYTES + 1];
        assert!(inflate_block(&deflate_block(&big)).is_none());
        assert_eq!(inflate_block(&deflate_block(b"k\tv\n")).unwrap(), b"k\tv\n");
    }

    #[test]
    fn fold_and_words_match_the_index_rules() {
        assert_eq!(fold("Björk ÆON Straße"), "bjork aeon strasse");
        assert_eq!(
            words("Sigur Rós – Ágætis byrjun!"),
            vec!["sigur", "ros", "agaetis", "byrjun"]
        );
        // Marks outside Latin fold too: scene posts strip them, and a
        // candidate needs a shared word key.
        assert_eq!(fold("Λαϊκά ζεϊμπέκικα"), "λαικα ζειμπεκικα");
        assert_eq!(fold("また逢う日まで"), "また逢う日まて");
        assert_eq!(fold("Ёлка"), "елка");
        assert_eq!(title_key("  The   Wall "), "t:the wall");
        assert_eq!(
            search_keys("the dark side of the moon"),
            vec![
                "t:the dark side of the moon",
                "w:dark",
                "w:moon",
                "w:side",
                "w:the"
            ]
        );
        assert!(search_keys("!!").is_empty());
    }

    #[test]
    fn similarity_matches_pg_trgm_reference_values() {
        // Reference values from PostgreSQL pg_trgm `similarity()`.
        assert_eq!(similarity("word", "word"), 1.0);
        assert!((similarity("word", "two words") - 0.363_636_37).abs() < 1e-6);
        assert_eq!(similarity("abc", "xyz"), 0.0);
        assert_eq!(similarity("", "abc"), 0.0);
        // Case and punctuation do not change trigram sets.
        assert_eq!(similarity("The Wall!", "the wall"), 1.0);
    }

    // Preserve the original a/b/x/y ordering vectors using valid, distinct
    // UUIDs now that the public ranking boundary rejects corrupt IDs.
    fn test_mbid(label: &str) -> String {
        format!("00000000-0000-0000-0000-{:012x}", label.as_bytes()[0])
    }

    #[test]
    fn canonically_equivalent_titles_share_keys_buckets_and_scores() {
        assert_eq!(title_key("ΟΣ"), "t:ος");
        assert_eq!(title_key("ΟΣ"), title_key(&crate::normalize_query("ΟΣ")));
        assert_eq!(search_keys("ΟΣ"), search_keys("ος"));
        assert_eq!(exact_trigrams("ΟΣ"), exact_trigrams("ος"));
        let posting = |mbid: &str, title: &str| Posting(mbid.into(), title.into(), None, None);
        for (composed, decomposed) in [
            ("Café Début", "Cafe\u{301} De\u{301}but"),
            ("Ångström", "A\u{30a}ngstro\u{308}m"),
            ("Йога", "И\u{306}ога"),
        ] {
            assert_eq!(words(composed), words(decomposed));
            assert_eq!(title_key(composed), title_key(decomposed));
            assert_eq!(search_keys(composed), search_keys(decomposed));
            assert_eq!(
                key_bucket(&title_key(composed)),
                key_bucket(&title_key(decomposed))
            );
            assert_eq!(trigrams(composed), trigrams(decomposed));
            assert_eq!(exact_trigrams(composed), exact_trigrams(decomposed));
            let a = rank_postings(composed, [posting(RELEASE_A, composed)], 25, 0).unwrap();
            let b = rank_postings(decomposed, [posting(RELEASE_A, composed)], 25, 0).unwrap();
            assert_eq!(a.len(), 1);
            assert_eq!(b.len(), 1);
            assert_eq!(a[0].mbid, b[0].mbid);
            assert_eq!(a[0].score, b[0].score);
        }
        assert_ne!(title_key("cafe noir"), title_key("cafe nord"));
        assert_ne!(exact_trigrams("cafe"), exact_trigrams("café"));
    }

    #[test]
    fn ranking_follows_the_postgres_release_order() {
        let p = |mbid: &str, title: &str| Posting(test_mbid(mbid), title.into(), None, None);
        let hits = rank_postings(
            "homogenic",
            vec![
                p("b", "Homogenic"),
                p("a", "Homogenic"),
                p("a", "Homogenic"),
                p("c", "Homogenic (Live)"),
                p("d", "Unrelated"),
            ],
            25,
            0,
        )
        .unwrap();
        let order: Vec<&str> = hits.iter().map(|h| h.mbid.as_str()).collect();
        assert_eq!(order, vec![test_mbid("a"), test_mbid("b"), test_mbid("c")]);
        assert_eq!(hits[0].score, 1.0);
        assert!(hits[2].score < 1.0 && hits[2].score >= SEARCH_MIN_SCORE);
        let page = rank_postings(
            "homogenic",
            vec![p("a", "Homogenic"), p("b", "Homogenic")],
            1,
            1,
        )
        .unwrap();
        assert_eq!(page.len(), 1);
        assert_eq!(page[0].mbid, test_mbid("b"));
    }

    #[test]
    fn ties_follow_the_database_collation_and_the_exact_score() {
        let p = |mbid: &str, title: &str| Posting(test_mbid(mbid), title.into(), None, None);
        // Collation order, not byte order: punctuation ignored first, then
        // lowercase before uppercase.
        let hits = rank_postings(
            "late show",
            vec![
                p("a", "LATE SHOW"),
                p("b", "Late Show"),
                p("c", "late show"),
                p("d", "Late, Late Show"),
            ],
            25,
            0,
        )
        .unwrap();
        let order: Vec<&str> = hits.iter().map(|h| h.label.as_str()).collect();
        assert_eq!(order[1..], ["late show", "Late Show", "LATE SHOW"]);
        assert!(hits.iter().all(|h| h.score == 1.0));
        // Titles that fold alike: the unaccented query prefers the ASCII
        // title, the accented query the accented one.
        let twins = || vec![p("x", "Detente"), p("y", "Détente")];
        assert_eq!(
            rank_postings("detente", twins(), 25, 0).unwrap()[0].label,
            "Detente"
        );
        let hits = rank_postings("détente", twins(), 25, 0).unwrap();
        assert_eq!(hits[0].label, "Détente");
        assert_eq!(hits[1].score, 1.0, "folded score is unchanged");
    }

    #[test]
    fn lookup_reads_one_block_after_the_index() {
        let repo = fixture("snap/t1/");
        let found = block_on(repo.lookup(Kind::Artist, &ARTIST.to_ascii_uppercase())).unwrap();
        match found {
            Some(Entity::Artist(a)) => {
                assert_eq!(a.mbid, ARTIST);
                assert_eq!(a.name, "Björk");
                assert_eq!(a.country.as_deref(), Some("IS"));
            }
            other => panic!("unexpected {other:?}"),
        }
        assert_eq!(repo.reads(), 2, "index load plus one block read");
        // Same bucket, absent MBID: a clean miss, not an error.
        let missing = "0383dadf-0000-4000-8000-000000000000";
        assert!(block_on(repo.lookup(Kind::Artist, missing))
            .unwrap()
            .is_none());
        // Empty bucket: a miss without a block read.
        let before = repo.reads();
        assert!(block_on(repo.lookup(Kind::Artist, RELEASE_C))
            .unwrap()
            .is_none());
        assert_eq!(repo.reads(), before);
    }

    #[test]
    fn release_lookup_keeps_the_dto_shape() {
        let repo = fixture("snap/t2/");
        let Some(Entity::Release(r)) = block_on(repo.lookup(Kind::Release, RELEASE_A)).unwrap()
        else {
            panic!("release expected");
        };
        assert_eq!(r.title, "Homogenic");
        assert_eq!(r.year, Some(1997));
        assert_eq!(r.artist_mbid.as_deref(), Some(ARTIST));
    }

    #[test]
    fn release_search_folds_diacritics_and_ranks_exact_titles_first() {
        let repo = fixture("snap/t3/");
        let hits = block_on(repo.search(Kind::Release, "debut", 25, 0)).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].mbid, RELEASE_C);
        assert_eq!(hits[0].artist_name.as_deref(), Some("Björk"));
        let hits = block_on(repo.search(Kind::Release, "homogenic", 25, 0)).unwrap();
        let order: Vec<&str> = hits.iter().map(|h| h.mbid.as_str()).collect();
        assert_eq!(order, vec![RELEASE_A, RELEASE_B]);
    }

    #[test]
    fn spike_scope_errors_are_generic_query_failures() {
        let repo = fixture("snap/t4/");
        let err = block_on(repo.search(Kind::Artist, "bjork", 25, 0)).unwrap_err();
        assert_eq!(err.stage(), RepoStage::Query);
        let err = block_on(repo.release_recordings(RELEASE_A)).unwrap_err();
        assert_eq!(err.stage(), RepoStage::Query);
    }

    #[test]
    fn index_ranges_reject_overflow_and_preserve_boundary_buckets() {
        let block = BlockRef {
            part: 0,
            len: 1,
            offset: 0,
        };
        let mut idx = vec![0; IDX_LEN];
        idx[..IDX_ENTRY_LEN].copy_from_slice(&encode_idx_entry(block));
        idx[IDX_LEN - IDX_ENTRY_LEN..].copy_from_slice(&encode_idx_entry(block));
        assert_eq!(idx_entry(&idx, 0), Some(block));
        assert_eq!(idx_entry(&idx, BUCKETS - 1), Some(block));
        for bucket in [BUCKETS, usize::MAX, usize::MAX / IDX_ENTRY_LEN] {
            assert_eq!(idx_entry(&idx, bucket), None);
        }
        assert_eq!(idx_entry(&[], 0), None);
        assert_eq!(idx_entry(&idx[..IDX_ENTRY_LEN - 1], 0), None);
    }

    #[test]
    fn posting_identifiers_are_validated_before_ranking_and_pagination() {
        let p = |mbid: &str, related: Option<String>| {
            Posting(mbid.into(), "Homogenic".into(), related, None)
        };
        let hits = rank_postings(
            "homogenic",
            [
                p(
                    &RELEASE_A.to_ascii_uppercase(),
                    Some(ARTIST.to_ascii_uppercase()),
                ),
                p(RELEASE_A, None),
                p(RELEASE_B, None),
            ],
            1,
            1,
        )
        .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(
            hits[0].mbid, RELEASE_B,
            "case variants consume only one slot"
        );
        let hits = rank_postings(
            "homogenic",
            [p(
                &RELEASE_A.to_ascii_uppercase(),
                Some(ARTIST.to_ascii_uppercase()),
            )],
            25,
            0,
        )
        .unwrap();
        assert_eq!(hits[0].mbid, RELEASE_A);
        assert_eq!(hits[0].artist_mbid.as_deref(), Some(ARTIST));
        for invalid in [
            "not-a-uuid",
            "1e6a0e9b-0000-4000-8000-00000000000",
            "ge6a0e9b-0000-4000-8000-000000000001",
        ] {
            let err = rank_postings("homogenic", [p(invalid, None)], 0, 10).unwrap_err();
            assert_eq!(err.stage(), RepoStage::Decode);
            let err = rank_postings("homogenic", [p(RELEASE_A, Some(invalid.into()))], 25, 0)
                .unwrap_err();
            assert_eq!(err.stage(), RepoStage::Decode);
        }
    }

    #[test]
    fn posting_decoding_enforces_each_key_cap_without_dedup_shortcuts() {
        let posting = Posting(RELEASE_A.into(), "Homogenic".into(), None, None);
        for (key, cap) in [
            ("t:homogenic", MAX_TITLE_POSTINGS),
            ("w:homogenic", MAX_WORD_POSTINGS),
        ] {
            let exact = serde_json::to_string(&vec![posting.clone(); cap]).unwrap();
            assert_eq!(decode_postings(key, &exact).unwrap().len(), cap);
            let extra = serde_json::to_string(&vec![posting.clone(); cap + 1]).unwrap();
            assert_eq!(
                decode_postings(key, &extra).unwrap_err().stage(),
                RepoStage::Decode
            );
            for malformed in ["[null]", "[[1,2,3,4]]", "[] false"] {
                assert_eq!(
                    decode_postings(key, malformed).unwrap_err().stage(),
                    RepoStage::Decode
                );
            }
            for corrupt in [
                Posting("not-a-uuid".into(), "Homogenic".into(), None, None),
                Posting(
                    RELEASE_A.into(),
                    "Homogenic".into(),
                    Some("invalid".into()),
                    None,
                ),
            ] {
                let value = serde_json::to_string(&vec![corrupt]).unwrap();
                assert_eq!(
                    decode_postings(key, &value).unwrap_err().stage(),
                    RepoStage::Decode
                );
            }
        }
        assert!(decode_postings("unknown", "[]").is_err());
        assert!(search_keys("one two three four five six").len() <= 1 + MAX_QUERY_WORDS);
        let repo = fixture("snap/duplicates/");
        let postings = block_on(repo.search_postings("homogenic live")).unwrap();
        assert!(postings.len() <= MAX_TITLE_POSTINGS + MAX_QUERY_WORDS * MAX_WORD_POSTINGS);
        let hits = rank_postings("homogenic live", postings, 25, 0).unwrap();
        assert_eq!(hits.iter().filter(|hit| hit.mbid == RELEASE_B).count(), 1);
    }

    #[test]
    fn related_lookup_identifiers_are_validated_and_canonicalized() {
        for related in [None, Some(ARTIST.to_ascii_uppercase())] {
            let row = ReleaseRow {
                artist_mbid: related.clone(),
                title: "Homogenic".into(),
                release_type: None,
                year: None,
                caa_url: None,
            };
            let Some(Entity::Release(release)) = decode_entity(
                Kind::Release,
                &RELEASE_A.to_ascii_uppercase(),
                &serde_json::to_string(&row).unwrap(),
            ) else {
                panic!("valid release");
            };
            assert_eq!(release.mbid, RELEASE_A);
            assert_eq!(release.artist_mbid, related.map(|_| ARTIST.to_owned()));
        }
        for related in [None, Some(RELEASE_A.to_ascii_uppercase())] {
            let row = RecordingRow {
                release_mbid: related.clone(),
                title: "Jóga".into(),
                track_no: None,
                duration_ms: None,
            };
            let Some(Entity::Recording(recording)) = decode_entity(
                Kind::Recording,
                RELEASE_B,
                &serde_json::to_string(&row).unwrap(),
            ) else {
                panic!("valid recording");
            };
            assert_eq!(
                recording.release_mbid,
                related.map(|_| RELEASE_A.to_owned())
            );
        }
        assert!(decode_entity(
            Kind::Release,
            RELEASE_A,
            r#"{"title":"ok","artist_mbid":"invalid"}"#
        )
        .is_none());
        assert!(decode_entity(
            Kind::Recording,
            RELEASE_B,
            r#"{"title":"ok","release_mbid":"invalid"}"#
        )
        .is_none());
        assert!(decode_entity(Kind::Release, "not-a-uuid", r#"{"title":"ok"}"#).is_none());
        assert!(decode_entity(Kind::Release, RELEASE_A, r#"{"title":"ok"}"#).is_some());
        assert!(decode_entity(Kind::Recording, RELEASE_B, r#"{"title":"ok"}"#).is_some());
        let repo = fixture("snap/invalid-lookup/");
        assert!(block_on(repo.lookup(Kind::Artist, "0383-not-a-uuid")).is_err());
        assert_eq!(repo.reads(), 0);
    }

    #[test]
    fn index_cache_is_source_local_in_both_orders_and_evicts() {
        for populated_first in [false, true] {
            let populated = fixture("snap/shared/");
            let mut source = MemSource::default();
            source
                .objects
                .insert(idx_object("snap/shared/", "artist"), vec![0; IDX_LEN]);
            let empty = SnapshotRepo::new(source, "snap/shared/").unwrap();
            for (repo, expected) in if populated_first {
                [(&populated, true), (&empty, false)]
            } else {
                [(&empty, false), (&populated, true)]
            } {
                assert_eq!(
                    block_on(repo.lookup(Kind::Artist, ARTIST))
                        .unwrap()
                        .is_some(),
                    expected
                );
                let before = repo.source.body_reads.get();
                assert_eq!(
                    block_on(repo.lookup(Kind::Artist, ARTIST))
                        .unwrap()
                        .is_some(),
                    expected
                );
                assert_eq!(
                    repo.source.body_reads.get(),
                    before,
                    "same repository reuses its index"
                );
            }
        }
        let repo = fixture("snap/eviction/");
        block_on(repo.index("artist")).unwrap();
        block_on(repo.index("release")).unwrap();
        assert_eq!(repo.source.body_reads.get(), 2);
        block_on(repo.index(SEARCH_KIND)).unwrap();
        assert_eq!(repo.indexes.borrow().len(), MAX_CACHED_INDEXES);
        let bytes: usize = repo
            .indexes
            .borrow()
            .iter()
            .map(|(_, bytes)| bytes.len())
            .sum();
        assert_eq!(bytes, MAX_CACHED_INDEXES * IDX_LEN);
        block_on(repo.index("artist")).unwrap();
        assert_eq!(repo.source.body_reads.get(), 4, "evicted index reloads");
        assert_eq!(repo.indexes.borrow().len(), MAX_CACHED_INDEXES);
        let other = fixture("snap/another/");
        assert!(block_on(other.lookup(Kind::Artist, ARTIST))
            .unwrap()
            .is_some());
        assert_eq!(other.source.body_reads.get(), 1);
    }

    #[test]
    fn missing_or_empty_prefix_cannot_fall_back_to_root_objects() {
        let mut source = MemSource::default();
        source.objects.insert("artist.idx".into(), vec![0; IDX_LEN]);
        for prefix in [
            Err("missing"),
            Err("unreadable"),
            Ok(String::new()),
            Ok("   ".into()),
        ] {
            let selected = configured_prefix(prefix);
            assert!(selected.is_err());
            assert_eq!(source.body_reads.get(), 0);
            assert_eq!(source.range_reads.get(), 0);
        }
        assert!(SnapshotRepo::new(source, "").is_err());
        assert_eq!(
            configured_prefix(Ok::<_, ()>("snap/valid/".into())).unwrap(),
            "snap/valid/"
        );
        // Health and authentication still precede configuration and storage;
        // a source-wiring guard is not a Worker runtime execution claim.
        let src = include_str!("snapshot.rs");
        let router = src
            .split("pub async fn spike_fetch(")
            .nth(1)
            .unwrap()
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        let health = router.find("path == \"/healthz\"").unwrap();
        let auth = router.find("expected.is_empty()").unwrap();
        let prefix = router.find("configured_prefix(").unwrap();
        let bucket = router.find("env.bucket(SNAPSHOT_BINDING)").unwrap();
        assert!(health < auth && auth < prefix && prefix < bucket);
    }

    #[test]
    fn invalid_block_spans_fail_before_range_io() {
        for (offset, len) in [
            (0, MAX_COMPRESSED_BLOCK_BYTES as u32 + 1),
            (u64::MAX, 1),
            (MAX_PART_BYTES, 1),
            (MAX_PART_BYTES - 1, 2),
        ] {
            let mut source = MemSource::default();
            let mut idx = vec![0; IDX_LEN];
            idx[..IDX_ENTRY_LEN].copy_from_slice(&encode_idx_entry(BlockRef {
                part: 0,
                offset,
                len,
            }));
            source
                .objects
                .insert(idx_object("snap/span/", "artist"), idx);
            let repo = SnapshotRepo::new(source, "snap/span/").unwrap();
            assert_eq!(
                block_on(repo.block("artist", 0)).unwrap_err().stage(),
                RepoStage::Decode
            );
            assert_eq!(repo.source.range_reads.get(), 0);
            assert_eq!(repo.reads(), 1, "only the index was read");
        }
        assert_eq!(
            checked_block_end(MAX_PART_BYTES - 1, 1).unwrap(),
            MAX_PART_BYTES
        );
        assert_eq!(
            checked_block_end(0, MAX_COMPRESSED_BLOCK_BYTES as u32).unwrap(),
            MAX_COMPRESSED_BLOCK_BYTES as u64
        );
        assert!(checked_block_end(0, 0).is_err());
        let source = MemSource::default();
        assert!(block_on(source.read_range("missing", 0, 1))
            .unwrap()
            .is_none());
    }

    #[test]
    fn sized_reads_check_metadata_before_body_and_bound_streamed_bytes() {
        use futures_util::stream;
        let opens = std::cell::Cell::new(0);
        for size in [0, IDX_LEN as u64 - 1, IDX_LEN as u64 + 1, u64::MAX] {
            let err = block_on(read_sized_body(size, IDX_LEN, || {
                opens.set(opens.get() + 1);
                Ok(stream::iter([Ok::<_, ()>(Vec::new())]))
            }))
            .unwrap_err();
            assert_eq!(err.stage(), RepoStage::Decode);
            assert_eq!(opens.get(), 0);
        }
        for chunks in [
            vec![vec![0; 3]],
            vec![vec![0; 5]],
            vec![vec![0; 2], vec![0; 3]],
        ] {
            assert_eq!(
                block_on(read_sized_body(4, 4, || Ok(stream::iter(
                    chunks.into_iter().map(Ok::<_, ()>)
                ))))
                .unwrap_err()
                .stage(),
                RepoStage::Decode
            );
        }
        let bytes = block_on(read_sized_body(4, 4, || {
            Ok(stream::iter([Ok::<_, ()>(vec![1, 2]), Ok(vec![3, 4])]))
        }))
        .unwrap();
        assert_eq!(bytes, vec![1, 2, 3, 4]);
        assert_eq!(
            block_on(read_sized_body(1, 1, || Ok(stream::iter([Err::<
                Vec<u8>,
                _,
            >(
                ()
            )]))))
            .unwrap_err()
            .stage(),
            RepoStage::Query
        );
        let bytes = block_on(read_sized_body(IDX_LEN as u64, IDX_LEN, || {
            Ok(stream::iter([Ok::<_, ()>(vec![0; IDX_LEN])]))
        }))
        .unwrap();
        assert_eq!(bytes.len(), IDX_LEN);
    }

    #[test]
    fn block_reads_reject_short_bodies_missing_data_and_invalid_encodings() {
        struct ReturnedBody {
            idx: Vec<u8>,
            raw: Option<Vec<u8>>,
        }
        impl RangeSource for ReturnedBody {
            async fn read_all_exact(
                &self,
                _: &str,
                _: usize,
            ) -> Result<Option<Vec<u8>>, RepoError> {
                Ok(Some(self.idx.clone()))
            }
            async fn read_range(
                &self,
                _: &str,
                _: u64,
                _: u32,
            ) -> Result<Option<Vec<u8>>, RepoError> {
                Ok(self.raw.clone())
            }
        }
        let valid = deflate_block(b"key\tvalue\n");
        for (raw, len, stage) in [
            (None, valid.len() as u32, RepoStage::Query),
            (
                Some(valid.clone()),
                valid.len() as u32 + 1,
                RepoStage::Decode,
            ),
            (Some(vec![0xff]), 1, RepoStage::Decode),
            (
                Some(deflate_block(&[0xff])),
                deflate_block(&[0xff]).len() as u32,
                RepoStage::Decode,
            ),
        ] {
            let mut idx = vec![0; IDX_LEN];
            idx[..IDX_ENTRY_LEN].copy_from_slice(&encode_idx_entry(BlockRef {
                part: 0,
                len,
                offset: 0,
            }));
            let repo = SnapshotRepo::new(ReturnedBody { idx, raw }, "snap/body/").unwrap();
            assert_eq!(
                block_on(repo.block("artist", 0)).unwrap_err().stage(),
                stage
            );
        }
        let mut idx = vec![0; IDX_LEN];
        idx[..IDX_ENTRY_LEN].copy_from_slice(&encode_idx_entry(BlockRef {
            part: 0,
            len: valid.len() as u32,
            offset: 0,
        }));
        let repo = SnapshotRepo::new(
            ReturnedBody {
                idx,
                raw: Some(valid),
            },
            "snap/body/",
        )
        .unwrap();
        assert_eq!(
            block_on(repo.block("artist", 0)).unwrap().as_deref(),
            Some("key\tvalue\n")
        );
        let repo = SnapshotRepo::new(
            ReturnedBody {
                idx: vec![0; 10],
                raw: None,
            },
            "snap/contract-violation/",
        )
        .unwrap();
        assert_eq!(
            block_on(repo.block("artist", 0)).unwrap_err().stage(),
            RepoStage::Decode
        );
    }

    #[test]
    fn compressed_cap_accepts_maximum_incompressible_plaintext() {
        // Deterministic synthetic bytes, not an acquired dump or a hostile allocation.
        let mut state = 0x1234_5678u32;
        let plain: Vec<u8> = (0..MAX_SNAPSHOT_BLOCK_BYTES)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                state as u8
            })
            .collect();
        let raw = deflate_block(&plain);
        assert!(
            raw.len() > MAX_SNAPSHOT_BLOCK_BYTES,
            "compressed and inflated caps are distinct"
        );
        assert!(raw.len() <= MAX_COMPRESSED_BLOCK_BYTES);
        assert!(checked_block_end(0, raw.len() as u32).is_ok());
        assert_eq!(inflate_block(&raw).unwrap(), plain);
    }

    #[test]
    fn missing_or_short_index_fails_closed() {
        let repo = SnapshotRepo::new(MemSource::default(), "snap/none/").unwrap();
        let err = block_on(repo.lookup(Kind::Artist, ARTIST)).unwrap_err();
        assert_eq!(err.stage(), RepoStage::Query);
        let mut source = MemSource::default();
        source
            .objects
            .insert(idx_object("snap/short/", "artist"), vec![0u8; 10]);
        let repo = SnapshotRepo::new(source, "snap/short/").unwrap();
        let err = block_on(repo.lookup(Kind::Artist, ARTIST)).unwrap_err();
        assert_eq!(err.stage(), RepoStage::Decode);
    }
}
