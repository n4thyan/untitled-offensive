use anyhow::{bail, Context, Result};
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Largest entry `Pk3Fs::read` will inflate. The biggest stock asset is a
/// lightmapped BSP under 32 MiB; anything past this is a corrupt or hostile pak.
pub const MAX_ENTRY_BYTES: u64 = 64 << 20;

/// Case-insensitive virtual filesystem over the *.pk3 archives of one mod
/// directory. Later archives override earlier ones, as the game layers paks.
pub struct Pk3Fs {
    archives: Vec<PathBuf>,
    // lowercased entry path -> (archive index, exact entry name in that zip)
    index: HashMap<String, (usize, String)>,
    // Same with '@' normalized to '_'; BSP material names sometimes spell the
    // '@' surface-type separator as '_'. Consulted when the exact lookup misses.
    alias_index: HashMap<String, (usize, String)>,
    // Kept open after first read; re-parsing a central directory per entry
    // dominates when loading ~3000 anims. Reads take &self, hence the lock.
    open: Mutex<HashMap<usize, zip::ZipArchive<File>>>,
}

impl Pk3Fs {
    /// Open a mod directory, optionally layered over another one.
    ///
    /// `overlay` is searched FIRST and the base `mod_dir` second, so a project
    /// pk3 can add a map (or override any file) without the base install being
    /// modified, copied, or duplicated into the repository. That is what lets
    /// original content sit alongside a retail `main/` under a clean licence
    /// split: nothing proprietary is ever written into the repo, and the
    /// overlay holds only our own archives.
    ///
    /// Ordering matters within the overlay too, and matches retail: later
    /// archives win, so `untitled.pk3` can override a file an earlier overlay
    /// provided. Each directory is sorted and concatenated overlay-first.
    pub fn open_layered(mod_dir: &Path, overlay: Option<&Path>) -> Result<Self> {
        let mut archives: Vec<PathBuf> = Vec::new();
        for dir in overlay.into_iter().chain(std::iter::once(mod_dir)) {
            let entries = match std::fs::read_dir(dir) {
                Ok(e) => e,
                // An overlay that is simply not there is the normal case for a
                // retail-only run; the base directory is the one that must exist.
                Err(e) => {
                    if std::ptr::eq(dir, mod_dir) {
                        return Err(anyhow::Error::from(e)
                            .context(format!("cannot read mod dir {}", mod_dir.display())));
                    }
                    log::debug!("no pk3 overlay dir {}; continuing", dir.display());
                    continue;
                }
            };
            let mut here: Vec<PathBuf> = entries
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pk3")))
                .collect();
            here.sort();
            archives.extend(here);
        }
        Self::from_archives(archives)
    }

    pub fn open(mod_dir: &Path) -> Result<Self> {
        let mut archives: Vec<PathBuf> = std::fs::read_dir(mod_dir)
            .with_context(|| format!("cannot read mod dir {}", mod_dir.display()))?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pk3")))
            .collect();
        archives.sort();
        Self::from_archives(archives)
    }

    /// Mount every ZIP-compatible archive with `extension` from `dir`.
    /// Later mounts override earlier entries, matching the engine's pak order.
    pub fn mount_dir(&mut self, dir: &Path, extension: &str) -> Result<()> {
        let mut archives: Vec<PathBuf> = std::fs::read_dir(dir)
            .with_context(|| format!("cannot read archive dir {}", dir.display()))?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case(extension))
            })
            .collect();
        archives.sort();
        if archives.is_empty() {
            bail!("no .{extension} archives found");
        }
        self.mount_archives(archives);
        Ok(())
    }

    /// Index an already-collected archive list. Shared by constructors and
    /// later mounts; the archive order IS the override order.
    fn from_archives(archives: Vec<PathBuf>) -> Result<Self> {
        if archives.is_empty() {
            bail!("no .pk3 archives found");
        }
        let mut fs = Self::empty();
        fs.mount_archives(archives);
        if fs.archives.is_empty() {
            bail!("no readable .pk3 archives");
        }
        Ok(fs)
    }

    fn mount_archives(&mut self, archives: Vec<PathBuf>) {
        for path in archives {
            if self.archives.contains(&path) {
                continue;
            }
            let zip = std::fs::File::open(&path)
                .map_err(anyhow::Error::from)
                .and_then(|f| Ok(zip::ZipArchive::new(f)?));
            let zip = match zip {
                Ok(z) => z,
                Err(e) => {
                    log::warn!("skipping unreadable archive {}: {e}", path.display());
                    continue;
                }
            };
            let i = self.archives.len();
            for name in zip.file_names() {
                let key = name.to_lowercase();
                if key.contains('@') {
                    self.alias_index
                        .insert(key.replace('@', "_"), (i, name.to_string()));
                }
                self.index.insert(key, (i, name.to_string()));
            }
            self.archives.push(path);
        }
    }

    /// A filesystem with no archives; every lookup misses. For tests that
    /// need the type but not the game.
    pub fn empty() -> Self {
        Self {
            archives: Vec::new(),
            index: HashMap::new(),
            alias_index: HashMap::new(),
            open: Mutex::new(HashMap::new()),
        }
    }

    /// Same lookup as `read`, without touching the archive.
    pub fn contains(&self, path: &str) -> bool {
        let key = path.to_lowercase();
        self.index.contains_key(&key) || self.alias_index.contains_key(&key)
    }

    /// The archive `read(path)` would hit.
    pub fn source_archive(&self, path: &str) -> Option<&Path> {
        let key = path.to_lowercase();
        let (ai, _) = self
            .index
            .get(&key)
            .or_else(|| self.alias_index.get(&key))?;
        Some(&self.archives[*ai])
    }

    /// `read` with `MAX_ENTRY_BYTES` as the size limit.
    pub fn read(&self, path: &str) -> Option<Vec<u8>> {
        self.read_limited(path, MAX_ENTRY_BYTES)
    }

    /// `None` when the entry is missing, unreadable, or inflates past
    /// `limit` bytes. The zip central directory's size field only sizes the
    /// buffer; the stream itself is bounded, since the field can lie.
    pub fn read_limited(&self, path: &str, limit: u64) -> Option<Vec<u8>> {
        let key = path.to_lowercase();
        let (ai, entry) = self
            .index
            .get(&key)
            .or_else(|| self.alias_index.get(&key))?;
        // A poisoned lock still holds a usable cache; `by_name` re-seeks from
        // the central directory.
        let mut open = self.open.lock().unwrap_or_else(|e| e.into_inner());
        let zip = match open.entry(*ai) {
            Entry::Occupied(e) => e.into_mut(),
            Entry::Vacant(e) => {
                let file = File::open(&self.archives[*ai]).ok()?;
                e.insert(zip::ZipArchive::new(file).ok()?)
            }
        };
        let f = zip.by_name(entry).ok()?;
        if f.size() > limit {
            log::warn!("{path}: declared size {} exceeds {limit} bytes", f.size());
            return None;
        }
        let mut buf = Vec::with_capacity(f.size() as usize);
        // One byte past the limit is read on purpose: it tells "exactly
        // limit" from "more than limit".
        f.take(limit + 1).read_to_end(&mut buf).ok()?;
        if buf.len() as u64 > limit {
            log::warn!("{path}: entry inflates past {limit} bytes");
            return None;
        }
        Some(buf)
    }

    /// Lowercased entry paths ending in `suffix`, e.g. ".shader".
    pub fn names_with_suffix(&self, suffix: &str) -> Vec<String> {
        let mut names: Vec<String> = self
            .index
            .keys()
            .filter(|k| k.ends_with(suffix))
            .cloned()
            .collect();
        names.sort();
        names
    }

    /// Lowercased entry paths starting with `prefix`, e.g. "xanim/".
    pub fn list_prefix(&self, prefix: &str) -> Vec<String> {
        let mut names: Vec<String> = self
            .index
            .keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect();
        names.sort();
        names
    }

    pub fn find_maps(&self) -> Vec<String> {
        let mut maps: Vec<String> = self
            .index
            .keys()
            .filter(|k| k.ends_with(".bsp"))
            .filter(|k| {
                let dir = &k[..k.rfind('/').unwrap_or(0)];
                dir == "maps" || dir == "maps/mp"
            })
            .map(|k| k[k.rfind('/').unwrap() + 1..k.len() - 4].to_string())
            .collect();
        maps.sort();
        maps.dedup();
        maps
    }

    pub fn resolve_map(&self, name: &str) -> Option<String> {
        let name = name.to_lowercase();
        for candidate in [format!("maps/{name}.bsp"), format!("maps/mp/{name}.bsp")] {
            if let Some((_, entry)) = self.index.get(&candidate) {
                return Some(entry.clone());
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn make_pk3(dir: &std::path::Path, file: &str, entries: &[(&str, &str)]) {
        let f = std::fs::File::create(dir.join(file)).unwrap();
        let mut z = zip::ZipWriter::new(f);
        let opts = zip::write::SimpleFileOptions::default();
        for (name, content) in entries {
            z.start_file(*name, opts).unwrap();
            z.write_all(content.as_bytes()).unwrap();
        }
        z.finish().unwrap();
    }

    #[test]
    fn case_insensitive_lookup() {
        let dir = tempfile::tempdir().unwrap();
        make_pk3(dir.path(), "pak0.pk3", &[("maps/MP/mp_test.bsp", "data")]);
        let fs = Pk3Fs::open(dir.path()).unwrap();
        assert_eq!(fs.read("maps/mp/MP_TEST.bsp").unwrap(), b"data");
        assert!(fs.read("maps/mp/missing.bsp").is_none());
    }

    #[test]
    fn underscore_matches_at_sign_in_entry_names() {
        let dir = tempfile::tempdir().unwrap();
        make_pk3(
            dir.path(),
            "pak0.pk3",
            &[("textures/x/snow@1024fill.dds", "tex")],
        );
        let fs = Pk3Fs::open(dir.path()).unwrap();
        assert_eq!(fs.read("textures/x/snow_1024fill.dds").unwrap(), b"tex");
        assert_eq!(fs.read("textures/x/snow@1024fill.dds").unwrap(), b"tex");
        assert!(fs.contains("textures/x/SNOW@1024fill.dds"));
        assert!(fs.contains("textures/x/snow_1024fill.dds"));
        assert!(!fs.contains("textures/x/missing.dds"));
    }

    #[test]
    fn exact_entry_beats_alias() {
        let dir = tempfile::tempdir().unwrap();
        make_pk3(
            dir.path(),
            "pak0.pk3",
            &[("a/b@c.txt", "aliased"), ("a/b_c.txt", "exact")],
        );
        let fs = Pk3Fs::open(dir.path()).unwrap();
        assert_eq!(fs.read("a/b_c.txt").unwrap(), b"exact");
    }

    #[test]
    fn later_pk3_overrides_earlier() {
        let dir = tempfile::tempdir().unwrap();
        make_pk3(dir.path(), "pak0.pk3", &[("a.txt", "old")]);
        make_pk3(dir.path(), "pak1.pk3", &[("A.TXT", "new")]);
        let fs = Pk3Fs::open(dir.path()).unwrap();
        assert_eq!(fs.read("a.txt").unwrap(), b"new");
    }

    #[test]
    fn finds_and_resolves_maps() {
        let dir = tempfile::tempdir().unwrap();
        make_pk3(
            dir.path(),
            "pak0.pk3",
            &[
                ("maps/MP/mp_test.bsp", "x"),
                ("maps/training.bsp", "x"),
                ("maps/MP/readme.txt", "x"),
            ],
        );
        let fs = Pk3Fs::open(dir.path()).unwrap();
        assert_eq!(
            fs.find_maps(),
            vec!["mp_test".to_string(), "training".to_string()]
        );
        assert_eq!(fs.resolve_map("MP_TEST").unwrap(), "maps/MP/mp_test.bsp");
        assert_eq!(fs.resolve_map("training").unwrap(), "maps/training.bsp");
        assert!(fs.resolve_map("nope").is_none());
    }

    /// Overwrites the uncompressed-size field of the single central
    /// directory entry so `size()` lies about the payload.
    fn patch_central_size(path: &std::path::Path, size: u32) {
        let mut d = std::fs::read(path).unwrap();
        let cd = d
            .windows(4)
            .position(|w| w == b"PK\x01\x02")
            .expect("central directory");
        d[cd + 24..cd + 28].copy_from_slice(&size.to_le_bytes());
        std::fs::write(path, d).unwrap();
    }

    #[test]
    fn read_refuses_entries_over_the_size_limit() {
        let dir = tempfile::tempdir().unwrap();
        let body = "x".repeat(100);
        make_pk3(dir.path(), "pak0.pk3", &[("big.txt", &body)]);
        let fs = Pk3Fs::open(dir.path()).unwrap();
        assert_eq!(fs.read_limited("big.txt", 100).unwrap().len(), 100);
        // declared size beyond the limit: refused before allocating
        assert!(fs.read_limited("big.txt", 99).is_none());
        assert!(fs.read("big.txt").is_some());
    }

    #[test]
    fn read_stops_when_the_stream_outgrows_its_declared_size() {
        let dir = tempfile::tempdir().unwrap();
        let body = "x".repeat(100);
        make_pk3(dir.path(), "pak0.pk3", &[("lie.txt", &body)]);
        // central directory claims 10 bytes; the stream holds 100
        patch_central_size(&dir.path().join("pak0.pk3"), 10);
        let fs = Pk3Fs::open(dir.path()).unwrap();
        assert!(fs.read_limited("lie.txt", 50).is_none());
        assert_eq!(fs.read_limited("lie.txt", 100).unwrap().len(), 100);
    }

    #[test]
    fn huge_declared_size_does_not_preallocate() {
        let dir = tempfile::tempdir().unwrap();
        make_pk3(dir.path(), "pak0.pk3", &[("lie.txt", "tiny")]);
        patch_central_size(&dir.path().join("pak0.pk3"), u32::MAX - 1);
        let fs = Pk3Fs::open(dir.path()).unwrap();
        assert!(fs.read("lie.txt").is_none());
    }

    #[test]
    fn corrupt_archive_is_skipped_not_fatal() {
        let dir = tempfile::tempdir().unwrap();
        make_pk3(dir.path(), "pak0.pk3", &[("a.txt", "ok")]);
        std::fs::write(dir.path().join("pak1.pk3"), b"not a zip at all").unwrap();
        make_pk3(dir.path(), "pak2.pk3", &[("b.txt", "also ok")]);
        let fs = Pk3Fs::open(dir.path()).unwrap();
        assert_eq!(fs.read("a.txt").unwrap(), b"ok");
        assert_eq!(fs.read("b.txt").unwrap(), b"also ok");
        assert_eq!(
            fs.source_archive("b.txt").unwrap(),
            dir.path().join("pak2.pk3")
        );
    }

    #[test]
    fn only_corrupt_archives_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("pak0.pk3"), b"garbage").unwrap();
        assert!(Pk3Fs::open(dir.path()).is_err());
        assert!(Pk3Fs::open(&dir.path().join("missing")).is_err());
    }

    #[test]
    fn mounts_iwd_archives_after_the_cod_filesystem_is_open() {
        let cod = tempfile::tempdir().unwrap();
        let mw2 = tempfile::tempdir().unwrap();
        make_pk3(cod.path(), "pak0.pk3", &[("maps/mp/mp_test.bsp", "map")]);
        make_pk3(
            mw2.path(),
            "iw_00.iwd",
            &[("images/rust_col.iwi", "pixels")],
        );

        let mut fs = Pk3Fs::open(cod.path()).unwrap();
        fs.mount_dir(mw2.path(), "iwd").unwrap();

        assert_eq!(fs.read("maps/mp/mp_test.bsp").unwrap(), b"map");
        assert_eq!(fs.read("images/rust_col.iwi").unwrap(), b"pixels");
    }

    #[test]
    fn real_game_dir_smoke() {
        let dir = crate::testing::game_dir().join("main");
        if !dir.is_dir() {
            return;
        }
        let fs = Pk3Fs::open(&dir).unwrap();
        assert!(fs.resolve_map("mp_pavlov").is_some());
        assert!(fs.find_maps().contains(&"mp_pavlov".to_string()));
    }
}
