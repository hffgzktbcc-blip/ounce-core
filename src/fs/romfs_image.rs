//! A RomFS image built from a host folder and read lazily.
//!
//! Games mount their data storage by reading a RomFS image through `IStorage` and parsing it
//! themselves: a header, two hash tables and two entry tables (directories, files), then file data.
//! Only the header and tables are kept in memory; file data is read from the host folder on demand,
//! so a multi-gigabyte RomFS costs a few megabytes. Layout follows the public SwitchBrew RomFS page.

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

const HEADER_SIZE: u64 = 0x50;
const NONE: u32 = 0xFFFF_FFFF;
const DATA_ALIGN: u64 = 0x10;

/// Where a file's data sits in the image.
#[derive(Debug, Clone)]
struct Slot {
    start: u64,
    size: u64,
    path: PathBuf,
}

/// A RomFS image: in-memory metadata plus host files for the data region.
#[derive(Debug, Clone)]
pub struct RomfsImage {
    meta: Vec<u8>,
    slots: Vec<Slot>,
    total: u64,
}

struct DirNode {
    name: Vec<u8>,
    parent: usize,
    dirs: Vec<usize>,
    files: Vec<usize>,
}

struct FileNode {
    name: Vec<u8>,
    parent: usize,
    size: u64,
    path: PathBuf,
}

fn align(v: u64, a: u64) -> u64 {
    (v + a - 1) / a * a
}

/// The RomFS name hash: rotate-xor over the name, seeded with the parent directory's offset.
fn hash(parent: u32, name: &[u8]) -> u32 {
    let mut h = parent ^ 123_456_789;
    for &c in name {
        h = h.rotate_right(5) ^ c as u32;
    }
    h
}

/// Hash table size for `n` entries: odd and coprime to the small primes.
fn table_len(n: u32) -> u32 {
    if n < 3 {
        3
    } else if n < 19 {
        n | 1
    } else {
        let mut c = n;
        while [2, 3, 5, 7, 11, 13, 17].iter().any(|p| c % p == 0) {
            c += 1;
        }
        c
    }
}

fn put32(buf: &mut [u8], at: u64, v: u32) {
    buf[at as usize..at as usize + 4].copy_from_slice(&v.to_le_bytes());
}

fn put64(buf: &mut [u8], at: u64, v: u64) {
    buf[at as usize..at as usize + 8].copy_from_slice(&v.to_le_bytes());
}

impl RomfsImage {
    /// Builds the image for the folder `root`. Reads directory listings and file sizes only.
    pub fn build(root: &Path) -> std::io::Result<Self> {
        let mut dirs = vec![DirNode { name: vec![], parent: 0, dirs: vec![], files: vec![] }];
        let mut files: Vec<FileNode> = Vec::new();
        let mut stack = vec![(0usize, root.to_path_buf())];
        while let Some((dir, path)) = stack.pop() {
            let mut entries: Vec<_> = std::fs::read_dir(&path)?.collect::<Result<_, _>>()?;
            entries.sort_by_key(|e| e.file_name());
            for e in entries {
                let name = e.file_name().to_string_lossy().into_owned().into_bytes();
                let meta = e.metadata()?;
                if meta.is_dir() {
                    dirs.push(DirNode { name, parent: dir, dirs: vec![], files: vec![] });
                    let idx = dirs.len() - 1;
                    dirs[dir].dirs.push(idx);
                    stack.push((idx, e.path()));
                } else if meta.is_file() {
                    files.push(FileNode { name, parent: dir, size: meta.len(), path: e.path() });
                    let idx = files.len() - 1;
                    dirs[dir].files.push(idx);
                }
            }
        }

        // Entry offsets within each table.
        let mut dir_off = Vec::with_capacity(dirs.len());
        let mut at = 0u64;
        for d in &dirs {
            dir_off.push(at as u32);
            at += 24 + align(d.name.len() as u64, 4);
        }
        let dir_meta_size = at;
        let mut file_off = Vec::with_capacity(files.len());
        at = 0;
        for f in &files {
            file_off.push(at as u32);
            at += 32 + align(f.name.len() as u64, 4);
        }
        let file_meta_size = at;

        let (dir_hash_len, file_hash_len) = (table_len(dirs.len() as u32), table_len(files.len() as u32));
        let dir_hash_off = HEADER_SIZE;
        let dir_meta_off = dir_hash_off + dir_hash_len as u64 * 4;
        let file_hash_off = align(dir_meta_off + dir_meta_size, 4);
        let file_meta_off = file_hash_off + file_hash_len as u64 * 4;
        let data_off = align(file_meta_off + file_meta_size, DATA_ALIGN);

        let mut meta = vec![0u8; data_off as usize];
        for (i, v) in [
            HEADER_SIZE, dir_hash_off, dir_hash_len as u64 * 4, dir_meta_off, dir_meta_size,
            file_hash_off, file_hash_len as u64 * 4, file_meta_off, file_meta_size, data_off,
        ].into_iter().enumerate() {
            put64(&mut meta, i as u64 * 8, v);
        }
        for i in 0..dir_hash_len as u64 {
            put32(&mut meta, dir_hash_off + i * 4, NONE);
        }
        for i in 0..file_hash_len as u64 {
            put32(&mut meta, file_hash_off + i * 4, NONE);
        }

        for (i, d) in dirs.iter().enumerate() {
            let base = dir_meta_off + dir_off[i] as u64;
            let sibling = dirs[d.parent].dirs.iter().position(|&c| c == i).and_then(|p| dirs[d.parent].dirs.get(p + 1));
            let parent_off = dir_off[d.parent];
            put32(&mut meta, base, parent_off);
            put32(&mut meta, base + 4, if i == 0 { NONE } else { sibling.map_or(NONE, |&s| dir_off[s]) });
            put32(&mut meta, base + 8, d.dirs.first().map_or(NONE, |&c| dir_off[c]));
            put32(&mut meta, base + 12, d.files.first().map_or(NONE, |&c| file_off[c]));
            let bucket = dir_hash_off + (hash(parent_off, &d.name) % dir_hash_len) as u64 * 4;
            let next = u32::from_le_bytes(meta[bucket as usize..bucket as usize + 4].try_into().unwrap());
            put32(&mut meta, base + 16, next);
            put32(&mut meta, bucket, dir_off[i]);
            put32(&mut meta, base + 20, d.name.len() as u32);
            meta[base as usize + 24..base as usize + 24 + d.name.len()].copy_from_slice(&d.name);
        }

        let mut slots = Vec::with_capacity(files.len());
        let mut data_at = 0u64;
        for (i, f) in files.iter().enumerate() {
            let base = file_meta_off + file_off[i] as u64;
            let siblings = &dirs[f.parent].files;
            let sibling = siblings.iter().position(|&c| c == i).and_then(|p| siblings.get(p + 1));
            let parent_off = dir_off[f.parent];
            put32(&mut meta, base, parent_off);
            put32(&mut meta, base + 4, sibling.map_or(NONE, |&s| file_off[s]));
            put64(&mut meta, base + 8, data_at);
            put64(&mut meta, base + 16, f.size);
            let bucket = file_hash_off + (hash(parent_off, &f.name) % file_hash_len) as u64 * 4;
            let next = u32::from_le_bytes(meta[bucket as usize..bucket as usize + 4].try_into().unwrap());
            put32(&mut meta, base + 24, next);
            put32(&mut meta, bucket, file_off[i]);
            put32(&mut meta, base + 28, f.name.len() as u32);
            meta[base as usize + 32..base as usize + 32 + f.name.len()].copy_from_slice(&f.name);
            slots.push(Slot { start: data_off + data_at, size: f.size, path: f.path.clone() });
            data_at = align(data_at + f.size, DATA_ALIGN);
        }
        Ok(Self { meta, slots, total: data_off + data_at })
    }

    /// Image size in bytes.
    pub fn size(&self) -> u64 {
        self.total
    }

    pub fn file_count(&self) -> usize {
        self.slots.len()
    }

    /// Fills `out` from `offset`; bytes past the end of the image, and alignment gaps, read as zero.
    /// Returns the number of bytes that lie inside the image.
    pub fn read(&self, offset: u64, out: &mut [u8]) -> usize {
        out.fill(0);
        let end = offset.saturating_add(out.len() as u64).min(self.total);
        if offset >= end {
            return 0;
        }
        let mut pos = offset;
        while pos < end {
            let dst = (pos - offset) as usize;
            if pos < self.meta.len() as u64 {
                let n = (self.meta.len() as u64).min(end) - pos;
                out[dst..dst + n as usize].copy_from_slice(&self.meta[pos as usize..(pos + n) as usize]);
                pos += n;
                continue;
            }
            let idx = self.slots.partition_point(|s| s.start + s.size <= pos);
            match self.slots.get(idx) {
                Some(s) if s.start <= pos => {
                    let n = (s.start + s.size).min(end) - pos;
                    if let Ok(mut f) = std::fs::File::open(&s.path) {
                        if f.seek(SeekFrom::Start(pos - s.start)).is_ok() {
                            let _ = f.read_exact(&mut out[dst..dst + n as usize]);
                        }
                    }
                    pos += n;
                }
                // Gap before the next file's data: zeros.
                Some(s) => pos = s.start.min(end),
                None => break,
            }
        }
        (end - offset) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u32_at(b: &[u8], at: u64) -> u32 {
        u32::from_le_bytes(b[at as usize..at as usize + 4].try_into().unwrap())
    }
    fn u64_at(b: &[u8], at: u64) -> u64 {
        u64::from_le_bytes(b[at as usize..at as usize + 8].try_into().unwrap())
    }

    /// Looks `name` up in the root directory the way a RomFS client does: hash, bucket, chain.
    fn find_root_file(img: &[u8], name: &[u8]) -> Option<(u64, u64)> {
        let (fh_off, fh_size, fm_off) = (u64_at(img, 0x28), u64_at(img, 0x30), u64_at(img, 0x38));
        let mut e = u32_at(img, fh_off + (hash(0, name) % (fh_size as u32 / 4)) as u64 * 4);
        while e != NONE {
            let base = fm_off + e as u64;
            let len = u32_at(img, base + 28) as usize;
            if &img[base as usize + 32..base as usize + 32 + len] == name {
                return Some((u64_at(img, base + 8), u64_at(img, base + 16)));
            }
            e = u32_at(img, base + 24);
        }
        None
    }

    #[test]
    fn image_has_walkable_tables_and_lazy_file_data() {
        let dir = std::env::temp_dir().join(format!("ounce-romfs-image-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("a.bin"), b"hello").unwrap();
        std::fs::write(dir.join("sub/b.bin"), b"0123456789abcdefXYZ").unwrap();

        let image = RomfsImage::build(&dir).unwrap();
        assert_eq!(image.file_count(), 2);
        let mut all = vec![0u8; image.size() as usize];
        assert_eq!(image.read(0, &mut all), all.len());

        assert_eq!(u64_at(&all, 0), HEADER_SIZE);
        let data_off = u64_at(&all, 0x48);
        let (off, size) = find_root_file(&all, b"a.bin").expect("a.bin is in the root hash chain");
        assert_eq!(size, 5);
        assert_eq!(&all[(data_off + off) as usize..(data_off + off) as usize + 5], b"hello");
        assert!(find_root_file(&all, b"missing").is_none());

        // A read that starts in one file and runs into the next sees both, with zeros in the gap.
        let mut part = vec![0u8; 40];
        image.read(data_off + 3, &mut part);
        assert_eq!(&part[..2], b"lo");
        assert_eq!(image.read(image.size() + 8, &mut part), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
