use std::borrow::Cow;
use std::hash::BuildHasher;
use std::iter::FusedIterator;

use hashbrown::{DefaultHashBuilder, HashTable};

pub(crate) const MAX_PATH_LEN: usize = 1024;

pub fn normalize_path(path: &str) -> Cow<'_, str> {
    if is_clean(path.as_bytes(), true) {
        return Cow::Borrowed(path);
    }
    let mut out = String::with_capacity(path.len());
    push_chars_normalized(&mut out, 0, path);
    Cow::Owned(out)
}

#[inline]
fn is_clean(bytes: &[u8], mut prev_slash: bool) -> bool {
    for &b in bytes {
        let slash = b == b'/';
        if b.is_ascii_uppercase() || b == b'\\' || (slash && prev_slash) {
            return false;
        }
        prev_slash = slash;
    }
    true
}

fn push_chars_normalized(out: &mut String, start: usize, s: &str) {
    for c in s.chars() {
        let c = if c == '\\' { '/' } else { c.to_ascii_lowercase() };
        if c == '/' && (out.len() == start || out.ends_with('/')) {
            continue;
        }
        out.push(c);
    }
}

pub(crate) fn extension_of(path: &str) -> &str {
    let name_start = path.rfind('/').map_or(0, |i| i + 1);
    let name = path.get(name_start..).unwrap_or("");
    match name.rfind('.') {
        Some(dot) => name.get(dot + 1..).unwrap_or(""),
        None => "",
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PathFilter {
    prefix: String,
    ext: Option<String>,
}

impl PathFilter {
    pub(crate) fn new(prefix: &str, ext: Option<&str>) -> Self {
        let prefix = normalize_path(prefix).trim_end_matches('/').to_owned();
        let ext = ext.map(|e| e.trim_start_matches('.').to_ascii_lowercase());
        Self { prefix, ext }
    }

    #[inline]
    pub(crate) fn matches_prefix(&self, path: &str) -> bool {
        self.prefix.is_empty()
            || (path.starts_with(self.prefix.as_str()) && path.as_bytes().get(self.prefix.len()) == Some(&b'/'))
    }

    #[inline]
    pub(crate) fn matches_ext(&self, path: &str) -> bool {
        match &self.ext {
            None => true,
            Some(ext) => extension_of(path) == ext.as_str(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Added {
    New(usize),
    Duplicate(usize),
    Rejected,
}

pub(crate) struct PathIndex {
    arena: String,
    ends: Vec<u32>,
    table: HashTable<u32>,
    hasher: DefaultHashBuilder,
}

#[inline]
fn path_at<'a>(arena: &'a str, ends: &[u32], i: usize) -> &'a str {
    let start = match i.checked_sub(1) {
        Some(prev) => ends.get(prev).copied().unwrap_or(0),
        None => 0,
    };
    let end = ends.get(i).copied().unwrap_or(0);
    arena.get(start as usize..end as usize).unwrap_or("")
}

impl PathIndex {
    #[inline]
    pub(crate) fn len(&self) -> usize {
        self.ends.len()
    }

    #[inline]
    pub(crate) fn path(&self, i: usize) -> &str {
        path_at(&self.arena, &self.ends, i)
    }

    #[inline]
    pub(crate) fn find(&self, normalized: &str) -> Option<usize> {
        let hash = self.hasher.hash_one(normalized);
        self.table.find(hash, |&i| self.path(i as usize) == normalized).map(|&i| i as usize)
    }

    pub(crate) fn iter(&self) -> Paths<'_> {
        Paths { arena: &self.arena, ends: self.ends.iter(), start: 0 }
    }
}

pub(crate) struct PathIndexBuilder {
    arena: String,
    ends: Vec<u32>,
    table: HashTable<u32>,
    hasher: DefaultHashBuilder,
    start: usize,
}

impl PathIndexBuilder {
    pub(crate) fn with_capacity(paths: usize, bytes: usize) -> Self {
        Self {
            arena: String::with_capacity(bytes),
            ends: Vec::with_capacity(paths),
            table: HashTable::with_capacity(paths),
            hasher: DefaultHashBuilder::default(),
            start: 0,
        }
    }

    #[inline]
    pub(crate) fn push(&mut self, component: &str) {
        let at_boundary = self.arena.len() == self.start || self.arena.ends_with('/');
        if is_clean(component.as_bytes(), at_boundary) {
            self.arena.push_str(component);
        } else {
            push_chars_normalized(&mut self.arena, self.start, component);
        }
    }

    pub(crate) fn finish(&mut self) -> Added {
        let Self { arena, ends, table, hasher, start } = self;
        let path = arena.get(*start..).unwrap_or("");
        let end = u32::try_from(arena.len()).ok();
        let index = u32::try_from(ends.len()).ok();
        let (Some(end), Some(index)) = (end, index) else {
            arena.truncate(*start);
            return Added::Rejected;
        };
        if path.is_empty() || path.len() > MAX_PATH_LEN {
            arena.truncate(*start);
            return Added::Rejected;
        }
        let hash = hasher.hash_one(path);
        if let Some(&existing) = table.find(hash, |&i| path_at(arena, ends, i as usize) == path) {
            arena.truncate(*start);
            return Added::Duplicate(existing as usize);
        }
        ends.push(end);
        table.insert_unique(hash, index, |&i| hasher.hash_one(path_at(arena, ends, i as usize)));
        *start = arena.len();
        Added::New(index as usize)
    }

    pub(crate) fn build(mut self) -> PathIndex {
        self.arena.truncate(self.start);
        self.arena.shrink_to_fit();
        self.ends.shrink_to_fit();
        self.table.shrink_to_fit(|&i| self.hasher.hash_one(path_at(&self.arena, &self.ends, i as usize)));
        PathIndex { arena: self.arena, ends: self.ends, table: self.table, hasher: self.hasher }
    }
}

#[derive(Debug, Clone)]
pub struct Paths<'a> {
    arena: &'a str,
    ends: std::slice::Iter<'a, u32>,
    start: usize,
}

impl<'a> Iterator for Paths<'a> {
    type Item = &'a str;

    #[inline]
    fn next(&mut self) -> Option<&'a str> {
        let end = *self.ends.next()? as usize;
        let path = self.arena.get(self.start..end).unwrap_or("");
        self.start = end;
        Some(path)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.ends.size_hint()
    }
}

impl ExactSizeIterator for Paths<'_> {}
impl FusedIterator for Paths<'_> {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_rules() {
        assert_eq!(normalize_path("a/b.vmt"), "a/b.vmt");
        assert_eq!(normalize_path("A\\B.VMT"), "a/b.vmt");
        assert_eq!(normalize_path("/a//b///c"), "a/b/c");
        assert_eq!(normalize_path("\\\\Server\\x"), "server/x");
        assert_eq!(normalize_path(""), "");
        assert_eq!(normalize_path("Ünïcode/Ä.TXT"), "Ünïcode/Ä.txt");
        assert!(matches!(normalize_path("a/b/c.txt"), Cow::Borrowed(_)));
        assert!(matches!(normalize_path("a/B"), Cow::Owned(_)));
    }

    #[test]
    fn extension() {
        assert_eq!(extension_of("materials/a.vmt"), "vmt");
        assert_eq!(extension_of("a.b/c"), "");
        assert_eq!(extension_of("a/b.tar.gz"), "gz");
        assert_eq!(extension_of("noext"), "");
        assert_eq!(extension_of("dir/.hidden"), "hidden");
        assert_eq!(extension_of("trailing."), "");
    }

    #[test]
    fn filter() {
        let f = PathFilter::new("Materials/", Some(".VMT"));
        assert!(f.matches_prefix("materials/a/b.vmt"));
        assert!(!f.matches_prefix("materialsx/a.vmt"));
        assert!(!f.matches_prefix("materials"));
        assert!(f.matches_ext("materials/a/b.vmt"));
        assert!(!f.matches_ext("materials/a/b.vtf"));
        let all = PathFilter::new("", None);
        assert!(all.matches_prefix("x"));
        assert!(all.matches_ext("x.y"));
        let none = PathFilter::new("/", Some(""));
        assert!(none.matches_prefix("anything"));
        assert!(none.matches_ext("dir/file"));
        assert!(!none.matches_ext("dir/file.txt"));
    }

    fn add(b: &mut PathIndexBuilder, path: &str) -> Added {
        b.push(path);
        b.finish()
    }

    #[test]
    fn builder_dedup_and_lookup() {
        let mut b = PathIndexBuilder::with_capacity(0, 0);
        assert_eq!(add(&mut b, "Materials/A.vmt"), Added::New(0));
        assert_eq!(add(&mut b, "models/b.mdl"), Added::New(1));
        assert_eq!(add(&mut b, "materials\\a.VMT"), Added::Duplicate(0));
        assert_eq!(add(&mut b, ""), Added::Rejected);
        assert_eq!(add(&mut b, "//"), Added::Rejected);
        let long = "x".repeat(MAX_PATH_LEN + 1);
        assert_eq!(add(&mut b, &long), Added::Rejected);
        assert_eq!(add(&mut b, "/root.txt"), Added::New(2));
        b.push("dangling");
        let idx = b.build();
        assert_eq!(idx.len(), 3);
        assert_eq!(idx.path(0), "materials/a.vmt");
        assert_eq!(idx.path(1), "models/b.mdl");
        assert_eq!(idx.path(2), "root.txt");
        assert_eq!(idx.path(3), "");
        assert_eq!(idx.find("models/b.mdl"), Some(1));
        assert_eq!(idx.find("models/b.mdx"), None);
        assert_eq!(idx.find("dangling"), None);
        let all: Vec<&str> = idx.iter().collect();
        assert_eq!(all, ["materials/a.vmt", "models/b.mdl", "root.txt"]);
        assert_eq!(idx.iter().len(), 3);
    }

    #[test]
    fn many_paths_survive_rehash() {
        let mut b = PathIndexBuilder::with_capacity(0, 0);
        for i in 0..20_000 {
            assert_eq!(add(&mut b, &format!("dir{}/file{i}.vtf", i % 37)), Added::New(i));
        }
        let idx = b.build();
        for i in (0..20_000).step_by(7) {
            let p = format!("dir{}/file{i}.vtf", i % 37);
            assert_eq!(idx.find(&p), Some(i));
            assert_eq!(idx.path(i), p);
        }
    }
}
