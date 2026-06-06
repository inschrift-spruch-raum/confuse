use std::collections::HashMap;
use std::ffi::{OsStr, OsString as StdOsString};
use std::time::{Duration, Instant};

use crate::fuser_facade::reply::ReplyEntryPayload;
use crate::fuser_facade::types::{FileAttr, Generation, INodeNo};

#[derive(Clone, Copy, Debug)]
pub(crate) struct PositivePathRecord<'a> {
    pub(crate) path: &'a OsStr,
    pub(crate) parent: INodeNo,
    pub(crate) parent_generation: Generation,
    pub(crate) name: &'a OsStr,
    pub(crate) attr: FileAttr,
    pub(crate) generation: Generation,
    pub(crate) ttl: Duration,
}

#[derive(Clone, Debug)]
pub(crate) struct ResolvedEntry {
    pub(crate) ino: INodeNo,
    pub(crate) generation: Generation,
}

#[derive(Clone, Debug)]
enum PathCacheEntry {
    Positive {
        parent: INodeNo,
        parent_generation: Generation,
        name: StdOsString,
        ino: INodeNo,
        generation: Generation,
        entry_expires: Instant,
        outstanding: Vec<(INodeNo, u64)>,
    },
    Negative {
        parent: INodeNo,
        parent_generation: Generation,
        name: StdOsString,
        expires: Instant,
    },
}

#[derive(Clone, Debug)]
struct InodeState {
    attr: FileAttr,
    generation: Generation,
    lookup_refcount: u64,
    attr_expires: Option<Instant>,
}

#[derive(Debug)]
pub(crate) struct PathResolver {
    paths: HashMap<String, PathCacheEntry>,
    inodes: HashMap<INodeNo, InodeState>,
    pending_forgets: Vec<(INodeNo, u64)>,
    negative_ttl: Option<Duration>,
}

impl Default for PathResolver {
    fn default() -> Self {
        Self {
            paths: HashMap::new(),
            inodes: HashMap::new(),
            pending_forgets: Vec::new(),
            negative_ttl: Some(Self::DEFAULT_NEGATIVE_TTL),
        }
    }
}

impl PathResolver {
    const DEFAULT_NEGATIVE_TTL: Duration = Duration::from_secs(1);

    pub(crate) fn set_negative_ttl(&mut self, ttl: Duration) {
        self.negative_ttl = (!ttl.is_zero()).then_some(ttl);
    }

    pub(crate) fn invalidate_entry_from_notify(&mut self, parent: INodeNo, name: &OsStr) {
        let expected_name = name.to_string_lossy().to_lowercase();
        let _ = self.invalidate_matching(|_candidate, entry| match entry {
            PathCacheEntry::Positive {
                parent: cached_parent,
                name: cached_name,
                ..
            }
            | PathCacheEntry::Negative {
                parent: cached_parent,
                name: cached_name,
                ..
            } => {
                *cached_parent == parent
                    && cached_name.to_string_lossy().to_lowercase() == expected_name
            }
        });
    }

    pub(crate) fn normalized_components(path: &OsStr) -> Vec<String> {
        let raw = path.to_string_lossy();
        let mut components = Vec::new();
        for component in raw.split(['\\', '/']).filter(|part| !part.is_empty()) {
            if component.ends_with(':') || component == "." {
                continue;
            }
            if component == ".." {
                let _ = components.pop();
                continue;
            }
            components.push(component.to_string());
        }
        components
    }

    pub(crate) fn normalize_path(path: &OsStr) -> String {
        let parts: Vec<String> = Self::normalized_components(path)
            .into_iter()
            .map(|part| part.to_lowercase())
            .collect();
        if parts.is_empty() {
            "\\".to_string()
        } else {
            format!("\\{}", parts.join("\\"))
        }
    }

    pub(crate) fn cached(
        &mut self, path: &OsStr, expected_parent: INodeNo, expected_parent_generation: Generation,
    ) -> Option<Result<ResolvedEntry, i32>> {
        let key = Self::normalize_path(path);
        if key == "\\" {
            return None;
        }
        let now = Instant::now();
        match self.paths.get(&key).cloned() {
            Some(PathCacheEntry::Positive {
                parent,
                parent_generation,
                name: _,
                ino,
                generation,
                entry_expires,
                outstanding,
            }) if entry_expires > now => {
                if parent != expected_parent || parent_generation != expected_parent_generation {
                    self.paths.remove(&key);
                    self.release_outstanding(outstanding);
                    return None;
                }
                self.inodes.get(&ino)?;
                Some(Ok(ResolvedEntry { ino, generation }))
            }
            Some(PathCacheEntry::Negative {
                parent,
                parent_generation,
                expires,
                ..
            }) if expires > now
                && parent == expected_parent
                && parent_generation == expected_parent_generation =>
            {
                Some(Err(libc::ENOENT))
            }
            Some(PathCacheEntry::Negative { expires, .. }) if expires > now => {
                self.paths.remove(&key);
                None
            }
            Some(PathCacheEntry::Positive { outstanding, .. }) => {
                self.paths.remove(&key);
                self.release_outstanding(outstanding);
                None
            }
            Some(PathCacheEntry::Negative { .. }) => {
                self.paths.remove(&key);
                None
            }
            None => None,
        }
    }

    pub(crate) fn remember_lookup(
        &mut self, path: &OsStr, parent: INodeNo, parent_generation: Generation, name: &OsStr,
        payload: ReplyEntryPayload,
    ) -> ResolvedEntry {
        self.remember_positive(PositivePathRecord {
            path,
            parent,
            parent_generation,
            name,
            attr: payload.attr,
            generation: payload.generation,
            ttl: payload.ttl,
        })
    }

    pub(crate) fn remember_positive(&mut self, record: PositivePathRecord<'_>) -> ResolvedEntry {
        let entry = ResolvedEntry {
            ino: record.attr.ino,
            generation: record.generation,
        };
        let key = Self::normalize_path(record.path);
        let mut outstanding = self.take_reusable_outstanding(&key, record.attr.ino, record.ttl);
        if !record.ttl.is_zero() {
            increment_outstanding_lookup(&mut outstanding, record.attr.ino);
            self.store_positive_path_entry(&key, &record, outstanding);
            self.remember_positive_inode(record);
        } else {
            self.release_outstanding(outstanding);
            self.pending_forgets.push((record.attr.ino, 1));
            self.retain_live_inodes();
        }
        entry
    }

    fn take_reusable_outstanding(
        &mut self, key: &str, ino: INodeNo, ttl: Duration,
    ) -> Vec<(INodeNo, u64)> {
        match self.paths.remove(key) {
            Some(PathCacheEntry::Positive {
                ino: cached_ino,
                outstanding,
                ..
            }) if cached_ino == ino && !ttl.is_zero() => outstanding,
            Some(PathCacheEntry::Positive { outstanding, .. }) => {
                self.release_outstanding(outstanding);
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn store_positive_path_entry(
        &mut self, key: &str, record: &PositivePathRecord<'_>, outstanding: Vec<(INodeNo, u64)>,
    ) {
        self.paths.insert(
            key.to_string(),
            PathCacheEntry::Positive {
                parent: record.parent,
                parent_generation: record.parent_generation,
                name: record.name.to_os_string(),
                ino: record.attr.ino,
                generation: record.generation,
                entry_expires: Instant::now() + record.ttl,
                outstanding,
            },
        );
    }

    fn remember_positive_inode(&mut self, record: PositivePathRecord<'_>) {
        self.inodes
            .entry(record.attr.ino)
            .and_modify(|state| {
                if state.attr_expires.is_none() {
                    state.attr = record.attr;
                }
                state.generation = record.generation;
                state.lookup_refcount = state.lookup_refcount.saturating_add(1);
            })
            .or_insert(InodeState {
                attr: record.attr,
                generation: record.generation,
                lookup_refcount: 1,
                attr_expires: None,
            });
    }

    pub(crate) fn cached_attr(&mut self, ino: INodeNo) -> Option<FileAttr> {
        let now = Instant::now();
        let state = self.inodes.get_mut(&ino)?;
        if state.attr_expires.is_some_and(|expires| expires > now) {
            Some(state.attr)
        } else {
            state.attr_expires = None;
            self.retain_live_inodes();
            None
        }
    }

    pub(crate) fn invalidate_inode_attr(&mut self, ino: INodeNo) {
        if let Some(state) = self.inodes.get_mut(&ino) {
            state.attr_expires = None;
        }
        self.retain_live_inodes();
    }

    pub(crate) fn remember_attr(&mut self, ino: INodeNo, attr: FileAttr, ttl: Duration) {
        if ttl.is_zero() {
            if let Some(state) = self.inodes.get_mut(&ino) {
                state.attr_expires = None;
            }
            return;
        }
        let expires = Instant::now() + ttl;
        self.inodes
            .entry(ino)
            .and_modify(|state| {
                state.attr = attr;
                state.attr_expires = Some(expires);
            })
            .or_insert(InodeState {
                attr,
                generation: Generation(0),
                lookup_refcount: 0,
                attr_expires: Some(expires),
            });
    }

    pub(crate) fn remember_negative(
        &mut self, path: &OsStr, parent: INodeNo, parent_generation: Generation, name: &OsStr,
    ) {
        let key = Self::normalize_path(path);
        if let Some(PathCacheEntry::Positive { outstanding, .. }) = self.paths.remove(&key) {
            for (ino, _) in &outstanding {
                if let Some(state) = self.inodes.get_mut(ino) {
                    state.attr_expires = None;
                }
            }
            self.release_outstanding(outstanding);
        }
        let Some(negative_ttl) = self.negative_ttl else {
            return;
        };
        self.paths.insert(
            key,
            PathCacheEntry::Negative {
                parent,
                parent_generation,
                name: name.to_os_string(),
                expires: Instant::now() + negative_ttl,
            },
        );
    }

    pub(crate) fn take_pending_forgets(&mut self) -> Vec<(INodeNo, u64)> {
        std::mem::take(&mut self.pending_forgets)
    }

    pub(crate) fn reap_expired(&mut self) -> Vec<(INodeNo, u64)> {
        let now = Instant::now();
        let keys: Vec<String> = self
            .paths
            .iter()
            .filter_map(|(key, entry)| match entry {
                PathCacheEntry::Positive { entry_expires, .. } if *entry_expires <= now => {
                    Some(key.clone())
                }
                PathCacheEntry::Negative { expires, .. } if *expires <= now => Some(key.clone()),
                _ => None,
            })
            .collect();
        for key in keys {
            match self.paths.remove(&key) {
                Some(PathCacheEntry::Positive { outstanding, .. }) => {
                    self.release_outstanding(outstanding);
                }
                Some(PathCacheEntry::Negative { .. }) | None => {}
            }
        }
        self.take_pending_forgets()
    }

    #[cfg(test)]
    pub(crate) fn invalidate_entry(
        &mut self, parent: INodeNo, name: &OsStr,
    ) -> Vec<(INodeNo, u64)> {
        let expected_name = name.to_string_lossy().to_lowercase();
        self.invalidate_matching(|_candidate, entry| match entry {
            PathCacheEntry::Positive {
                parent: cached_parent,
                name: cached_name,
                ..
            }
            | PathCacheEntry::Negative {
                parent: cached_parent,
                name: cached_name,
                ..
            } => {
                *cached_parent == parent
                    && cached_name.to_string_lossy().to_lowercase() == expected_name
            }
        })
    }

    fn release_outstanding(&mut self, outstanding: Vec<(INodeNo, u64)>) {
        for (ino, count) in outstanding {
            if let Some(state) = self.inodes.get_mut(&ino) {
                let nlookup = state.lookup_refcount.min(count);
                state.lookup_refcount = state.lookup_refcount.saturating_sub(nlookup);
                if nlookup > 0 {
                    self.pending_forgets.push((ino, nlookup));
                }
            }
        }
        self.retain_live_inodes();
    }

    fn retain_live_inodes(&mut self) {
        let now = Instant::now();
        self.inodes.retain(|_, state| {
            state.lookup_refcount > 0 || state.attr_expires.is_some_and(|expires| expires > now)
        });
    }

    pub(crate) fn invalidate_subtree(&mut self, path: &OsStr) -> Vec<(INodeNo, u64)> {
        let key = Self::normalize_path(path);
        let prefix = if key.ends_with('\\') {
            key.clone()
        } else {
            format!("{key}\\")
        };
        self.invalidate_matching(|candidate, _entry| {
            candidate == key || candidate.starts_with(&prefix)
        })
    }

    pub(crate) fn generation_for(&self, ino: INodeNo) -> Generation {
        self.inodes
            .get(&ino)
            .map(|state| state.generation)
            .unwrap_or(Generation(0))
    }

    fn invalidate_matching(
        &mut self, matches: impl Fn(&str, &PathCacheEntry) -> bool,
    ) -> Vec<(INodeNo, u64)> {
        let keys: Vec<String> = self
            .paths
            .iter()
            .filter(|(key, entry)| matches(key, entry))
            .map(|(key, _)| key.clone())
            .collect();
        let mut forgets = Vec::new();
        for key in keys {
            if let Some(PathCacheEntry::Positive { outstanding, .. }) = self.paths.remove(&key) {
                self.invalidate_positive_outstanding(outstanding);
            }
        }
        forgets.extend(self.take_pending_forgets());
        self.retain_live_inodes();
        forgets
    }

    fn invalidate_positive_outstanding(&mut self, outstanding: Vec<(INodeNo, u64)>) {
        for (ino, _) in &outstanding {
            if let Some(state) = self.inodes.get_mut(ino) {
                state.attr_expires = None;
            }
        }
        self.release_outstanding(outstanding);
    }
}

fn increment_outstanding_lookup(outstanding: &mut Vec<(INodeNo, u64)>, ino: INodeNo) {
    if let Some((_, nlookup)) = outstanding
        .iter_mut()
        .find(|(cached_ino, _)| *cached_ino == ino)
    {
        *nlookup = nlookup.saturating_add(1);
    } else {
        outstanding.push((ino, 1));
    }
}
