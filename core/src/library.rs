//! Files on disk: `<root>/sounds/*` and `<root>/images/<sound id>.<ext>`.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Component, Path, PathBuf};

use thiserror::Error;

use crate::audio::decode::{decode_file, has_supported_extension};
use crate::config::Sound;
use crate::palette::next_color;

pub const IMAGE_MIMES: [&str; 4] = ["image/png", "image/jpeg", "image/webp", "image/gif"];

#[derive(Debug, Error)]
pub enum LibraryError {
    #[error("{0} is not an mp3, wav, ogg or flac file")]
    UnsupportedExtension(String),
    #[error("{0} could not be decoded")]
    Undecodable(String),
    #[error("{0} is not a png, jpg, webp or gif image")]
    NotAnImage(String),
    #[error(transparent)]
    Io(#[from] io::Error),
}

pub struct Library {
    root: PathBuf,
}

fn display_name(path: &Path) -> String {
    path.file_name().and_then(|n| n.to_str()).unwrap_or("file").to_string()
}

/// Name inside the library folders that import can never create (no audio/image extension).
const INVALID_NAME: &str = "__invalid__";

/// True when `name` is a single plain file name: exactly one `Normal` path component and no
/// separator or drive colon. Ellipses such as `Oh no...wav` are fine; `..`, `.` and `a/b` are not.
fn is_safe_name(name: &str) -> bool {
    if name.contains(['/', '\\', ':']) {
        return false;
    }
    let mut parts = Path::new(name).components();
    matches!((parts.next(), parts.next()), (Some(Component::Normal(_)), None))
}

/// A storable file name for an import source: separators and colons become `_`, and anything
/// still unsafe becomes `sound.<ext>`.
fn safe_file_name(name: &str) -> String {
    let cleaned: String = name.chars().map(|c| if matches!(c, '/' | '\\' | ':') { '_' } else { c }).collect();
    if is_safe_name(&cleaned) {
        return cleaned;
    }
    match Path::new(name).extension().and_then(|e| e.to_str()) {
        Some(ext) if is_safe_name(ext) => format!("sound.{ext}"),
        _ => "sound".to_string(),
    }
}

/// `dir/name`, unless `name` (read from config, possibly hand-edited) could escape `dir`;
/// then a path inside `dir` that cannot exist, so reads miss and deletes do nothing.
fn contained(dir: PathBuf, name: &str) -> PathBuf {
    if !is_safe_name(name) {
        log::warn!("ignoring unsafe library file name {name:?}");
        return dir.join(INVALID_NAME);
    }
    dir.join(name)
}

fn remove_if_exists(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

impl Library {
    pub fn open(root: impl Into<PathBuf>) -> io::Result<Self> {
        let root = root.into();
        fs::create_dir_all(root.join("sounds"))?;
        fs::create_dir_all(root.join("images"))?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn config_path(&self) -> PathBuf {
        self.root.join("config.json")
    }

    pub fn sounds_dir(&self) -> PathBuf {
        self.root.join("sounds")
    }

    pub fn images_dir(&self) -> PathBuf {
        self.root.join("images")
    }

    pub fn sound_path(&self, sound: &Sound) -> PathBuf {
        contained(self.sounds_dir(), &sound.file)
    }

    pub fn image_path(&self, sound: &Sound) -> Option<PathBuf> {
        sound.image.as_ref().map(|i| contained(self.images_dir(), i))
    }

    /// Validates, copies into `sounds/` (renaming on collision) and returns a new `Sound`.
    pub fn import_sound(&self, source: &Path, used_colors: &[&str]) -> Result<Sound, LibraryError> {
        let display = display_name(source);
        if !has_supported_extension(source) {
            return Err(LibraryError::UnsupportedExtension(display));
        }
        decode_file(source).map_err(|_| LibraryError::Undecodable(display.clone()))?;
        let (dest, mut out) = claim_unique(&self.sounds_dir(), &safe_file_name(&display))?;
        let copied = File::open(source).and_then(|mut input| io::copy(&mut input, &mut out));
        drop(out);
        if let Err(e) = copied {
            let _ = fs::remove_file(&dest);
            return Err(e.into());
        }
        let name = source.file_stem().and_then(|s| s.to_str()).unwrap_or("Sound").to_string();
        Ok(Sound {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            file: display_name(&dest),
            image: None,
            volume: 1.0,
            hotkey: None,
            color: Some(next_color(used_colors).to_string()),
            trim: None,
        })
    }

    /// Validates by magic bytes, copies to `images/<id>.<ext>`, removes any previous image.
    pub fn import_image(&self, sound: &mut Sound, source: &Path) -> Result<(), LibraryError> {
        let display = display_name(source);
        let kind = infer::get_from_path(source)?
            .filter(|k| IMAGE_MIMES.contains(&k.mime_type()))
            .ok_or(LibraryError::NotAnImage(display))?;
        let file_name = format!("{}.{}", sound.id, kind.extension());
        let dest = self.images_dir().join(&file_name);
        self.remove_image(sound)?;
        fs::copy(source, &dest)?;
        sound.image = Some(file_name);
        Ok(())
    }

    pub fn remove_image(&self, sound: &mut Sound) -> io::Result<()> {
        if let Some(p) = self.image_path(sound) {
            remove_if_exists(&p)?;
        }
        sound.image = None;
        Ok(())
    }

    pub fn delete_sound(&self, sound: &Sound) -> io::Result<()> {
        remove_if_exists(&self.sound_path(sound))?;
        if let Some(p) = self.image_path(sound) {
            remove_if_exists(&p)?;
        }
        Ok(())
    }
}

/// Atomically creates the first free name of `dir/file.ext`, `dir/file (2).ext`,
/// `dir/file (3).ext`, ... with `create_new`, so concurrent imports never share a file.
fn claim_unique(dir: &Path, file_name: &str) -> io::Result<(PathBuf, File)> {
    let p = Path::new(file_name);
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
    let ext = p.extension().and_then(|e| e.to_str()).map(|e| format!(".{e}")).unwrap_or_default();
    let candidates =
        std::iter::once(file_name.to_string()).chain((2u32..).map(|n| format!("{stem} ({n}){ext}")));
    for name in candidates {
        let candidate = dir.join(name);
        match OpenOptions::new().write(true).create_new(true).open(&candidate) {
            Ok(f) => return Ok((candidate, f)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    unreachable!("some suffix is free")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::write_test_wav;
    use tempfile::tempdir;

    const PNG: &[u8] = &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13, b'I', b'H', b'D', b'R'];
    const GIF: &[u8] = b"GIF89a\x01\x00\x01\x00\x00\x00\x00";

    fn setup() -> (tempfile::TempDir, Library, std::path::PathBuf) {
        let dir = tempdir().unwrap();
        let lib = Library::open(dir.path().join("data")).unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        (dir, lib, src)
    }

    #[test]
    fn open_creates_folders() {
        let (_d, lib, _) = setup();
        assert!(lib.sounds_dir().is_dir());
        assert!(lib.images_dir().is_dir());
        assert_eq!(lib.config_path().file_name().unwrap(), "config.json");
    }

    #[test]
    fn import_rejects_unsupported_extension() {
        let (_d, lib, src) = setup();
        let p = src.join("notes.txt");
        std::fs::write(&p, "hello").unwrap();
        assert!(matches!(lib.import_sound(&p, &[]), Err(LibraryError::UnsupportedExtension(n)) if n == "notes.txt"));
    }

    #[test]
    fn import_rejects_undecodable() {
        let (_d, lib, src) = setup();
        let p = src.join("fake.wav");
        std::fs::write(&p, "not a wav").unwrap();
        assert!(matches!(lib.import_sound(&p, &[]), Err(LibraryError::Undecodable(n)) if n == "fake.wav"));
        assert_eq!(std::fs::read_dir(lib.sounds_dir()).unwrap().count(), 0, "nothing copied");
    }

    #[test]
    fn import_copies_and_renames_on_collision() {
        let (_d, lib, src) = setup();
        let p = src.join("clip.wav");
        write_test_wav(&p, 100);
        let a = lib.import_sound(&p, &[]).unwrap();
        let b = lib.import_sound(&p, &[]).unwrap();
        assert_eq!(a.name, "clip");
        assert_eq!(b.name, "clip");
        assert_eq!(a.file, "clip.wav");
        assert_eq!(b.file, "clip (2).wav");
        assert_ne!(a.id, b.id);
        assert_eq!(a.volume, 1.0);
        assert!(lib.sound_path(&a).exists());
        assert!(lib.sound_path(&b).exists());
    }

    #[test]
    fn import_picks_the_least_used_color() {
        let (_d, lib, src) = setup();
        let p = src.join("clip.wav");
        write_test_wav(&p, 100);
        assert_eq!(lib.import_sound(&p, &[]).unwrap().color.as_deref(), Some("#FF595E"));
        let used = ["#FF595E", "#ff924c"];
        assert_eq!(lib.import_sound(&p, &used).unwrap().color.as_deref(), Some("#FFCA3A"));
    }

    #[test]
    fn import_image_validates_magic_and_replaces_old() {
        let (_d, lib, src) = setup();
        let wav = src.join("clip.wav");
        write_test_wav(&wav, 10);
        let mut s = lib.import_sound(&wav, &[]).unwrap();

        let txt = src.join("x.png");
        std::fs::write(&txt, "not an image even with png extension").unwrap();
        assert!(matches!(lib.import_image(&mut s, &txt), Err(LibraryError::NotAnImage(_))));
        assert_eq!(s.image, None);

        let png = src.join("pic.bin");
        std::fs::write(&png, PNG).unwrap();
        lib.import_image(&mut s, &png).unwrap();
        assert_eq!(s.image.as_deref(), Some(format!("{}.png", s.id).as_str()));
        let png_path = lib.image_path(&s).unwrap();
        assert!(png_path.exists());

        let gif = src.join("anim.gif");
        std::fs::write(&gif, GIF).unwrap();
        lib.import_image(&mut s, &gif).unwrap();
        assert_eq!(s.image.as_deref(), Some(format!("{}.gif", s.id).as_str()));
        assert!(!png_path.exists(), "old image removed");

        lib.remove_image(&mut s).unwrap();
        assert_eq!(s.image, None);
        assert_eq!(std::fs::read_dir(lib.images_dir()).unwrap().count(), 0);
    }

    #[test]
    fn delete_removes_sound_and_image() {
        let (_d, lib, src) = setup();
        let wav = src.join("clip.wav");
        write_test_wav(&wav, 10);
        let mut s = lib.import_sound(&wav, &[]).unwrap();
        let png = src.join("p.png");
        std::fs::write(&png, PNG).unwrap();
        lib.import_image(&mut s, &png).unwrap();
        lib.delete_sound(&s).unwrap();
        assert!(!lib.sound_path(&s).exists());
        assert!(!lib.image_path(&s).unwrap().exists());
        lib.delete_sound(&s).unwrap(); // idempotent
    }

    #[test]
    fn unsafe_stored_names_stay_inside_the_library() {
        let (d, lib, _) = setup();
        let outside = d.path().join("secret.wav");
        std::fs::write(&outside, "x").unwrap();
        for bad in ["../secret.wav", "..\\secret.wav", "sub/x.wav", "C:\\Windows\\win.ini", "C:x.wav", ".."] {
            let s = Sound {
                id: "i".into(),
                name: "n".into(),
                file: bad.into(),
                image: Some(bad.into()),
                volume: 1.0,
                hotkey: None,
                color: None,
                trim: None,
            };
            assert_eq!(lib.sound_path(&s), lib.sounds_dir().join(INVALID_NAME), "{bad}");
            assert_eq!(lib.image_path(&s), Some(lib.images_dir().join(INVALID_NAME)), "{bad}");
            lib.delete_sound(&s).unwrap();
        }
        assert!(outside.exists(), "delete never reaches outside the library");
        let ok = Sound { id: "i".into(), name: "n".into(), file: "clip (2).wav".into(), image: None, volume: 1.0, hotkey: None, color: None, trim: None };
        assert_eq!(lib.sound_path(&ok), lib.sounds_dir().join("clip (2).wav"));
    }

    #[test]
    fn ellipsis_names_import_play_and_delete() {
        let (_d, lib, src) = setup();
        let p = src.join("a...wav");
        write_test_wav(&p, 100);
        let s = lib.import_sound(&p, &[]).unwrap();
        assert_eq!(s.file, "a...wav");
        let stored = lib.sound_path(&s);
        assert_eq!(stored, lib.sounds_dir().join("a...wav"));
        assert!(stored.exists(), "points at the real imported file");
        lib.delete_sound(&s).unwrap();
        assert!(!stored.exists(), "delete removes it, nothing orphaned");
    }

    #[test]
    fn safe_names_and_sanitizing() {
        for ok in ["a.wav", "Oh no...wav", "...wav", "clip (2).mp3"] {
            assert!(is_safe_name(ok), "{ok}");
        }
        for bad in ["", ".", "..", "../x.wav", "sub/x.wav", "sub\\x.wav", "C:x.wav", "C:\\x.wav"] {
            assert!(!is_safe_name(bad), "{bad}");
        }
        assert_eq!(safe_file_name("a...wav"), "a...wav");
        assert_eq!(safe_file_name("C:x.wav"), "C_x.wav");
        assert_eq!(safe_file_name(".."), "sound");
    }

    #[test]
    fn claim_unique_counts_up_and_creates_the_file() {
        let dir = tempdir().unwrap();
        let (p, _f) = claim_unique(dir.path(), "a.mp3").unwrap();
        assert_eq!(p, dir.path().join("a.mp3"));
        assert!(p.exists(), "claim creates the file");
        std::fs::write(dir.path().join("a (2).mp3"), "").unwrap();
        let (p3, _f) = claim_unique(dir.path(), "a.mp3").unwrap();
        assert_eq!(p3, dir.path().join("a (3).mp3"));
    }

    #[test]
    fn import_skips_a_precreated_suffix() {
        let (_d, lib, src) = setup();
        let p = src.join("clip.wav");
        write_test_wav(&p, 100);
        let a = lib.import_sound(&p, &[]).unwrap();
        std::fs::write(lib.sounds_dir().join("clip (2).wav"), "taken").unwrap();
        let b = lib.import_sound(&p, &[]).unwrap();
        assert_eq!(a.file, "clip.wav");
        assert_eq!(b.file, "clip (3).wav");
        assert_eq!(std::fs::read(lib.sounds_dir().join("clip (2).wav")).unwrap(), b"taken", "not overwritten");
        assert_eq!(
            std::fs::read(lib.sound_path(&b)).unwrap(),
            std::fs::read(&p).unwrap(),
            "copied content is intact"
        );
    }
}
