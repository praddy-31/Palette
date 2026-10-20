//! The wallpaper library: `~/.config/palette/wallpapers/`, a shared folder of
//! pictures that belong to no theme. Downloads from a link (`fetch.rs`) and
//! images you keep land here; from here you can use one now, copy it into a
//! theme (optionally recoloured to it) or make a new theme from it.
//!
//! Subfolders are allowed (one per source repo, say). Items are named by
//! their path inside the library with `/` (`CozyPixels/Nord/lake.png`), and
//! every lookup goes through `list`, which only walks regular files and never
//! follows symlinks, so a name can't reach anything outside the folder.

use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::paths::Paths;
use crate::studio::{self, Drafted, Engine};
use crate::system::{self, Outcome};
use crate::theme::{self, Theme};

/// How deep the walk goes (library root = 0).
const MAX_DEPTH: usize = 8;

/// `~/.config/palette/wallpapers/`.
pub fn dir(paths: &Paths) -> PathBuf {
    paths.root().join("wallpapers")
}

/// One picture in the library.
#[derive(Debug, Clone)]
pub struct Item {
    /// Path inside the library, `/`-separated: what commands take.
    pub name: String,
    /// The folder part of `name` (empty for the top level).
    pub folder: String,
    pub bytes: u64,
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct Library {
    pub dir: PathBuf,
    pub items: Vec<Item>,
    /// Every regular file in the folder, not only the pictures.
    pub bytes: u64,
}

/// Everything in the library, sorted by name. An empty library (no folder
/// yet) is fine.
pub fn list(paths: &Paths) -> Result<Library> {
    let root = dir(paths);
    let mut items = Vec::new();
    let mut bytes = 0;
    walk(&root, &root, 0, &mut |path, rel, len| {
        bytes += len;
        if theme::is_image(path) {
            let folder = rel.rsplit_once('/').map_or("", |(f, _)| f).to_string();
            items.push(Item {
                name: rel.to_string(),
                folder,
                bytes: len,
                path: path.to_path_buf(),
            });
        }
    })?;
    items.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(Library {
        dir: root,
        items,
        bytes,
    })
}

/// Total size of the regular files under `dir` (symlinks not followed);
/// 0 when it doesn't exist. Also used for a theme's wallpaper folder.
pub fn folder_size(dir: &Path) -> u64 {
    let mut bytes = 0;
    let _ = walk(dir, dir, 0, &mut |_, _, len| bytes += len);
    bytes
}

/// Regular files under `dir`, skipping hidden names (partial downloads) and
/// symlinks. `visit(path, name inside root, size)`.
fn walk(
    root: &Path,
    dir: &Path,
    depth: usize,
    visit: &mut dyn FnMut(&Path, &str, u64),
) -> Result<()> {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e).with_context(|| format!("reading {}", dir.display())),
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if name.starts_with('.') {
            continue;
        }
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if kind.is_dir() && depth < MAX_DEPTH {
            walk(root, &path, depth + 1, visit)?;
        } else if kind.is_file() {
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            let len = entry.metadata().map_or(0, |m| m.len());
            visit(&path, &rel, len);
        }
    }
    Ok(())
}

/// A library picture by name (as `list` shows it).
pub fn find(paths: &Paths, name: &str) -> Result<Item> {
    let name = name.trim().trim_start_matches('/');
    list(paths)?
        .items
        .into_iter()
        .find(|i| i.name == name)
        .with_context(|| {
            format!("no picture called `{name}` in your wallpaper library (palette bg library)")
        })
}

/// `palette bg library add`: copy pictures into the library (or a subfolder
/// of it). A name that's taken gets `-2`, `-3`… Returns the new files.
pub fn add(paths: &Paths, files: &[PathBuf], into: Option<&str>) -> Result<Vec<PathBuf>> {
    for file in files {
        let meta = fs::metadata(file).with_context(|| format!("reading {}", file.display()))?;
        if !meta.is_file() || !theme::is_image(file) {
            bail!(
                "{} is not an image (png, jpg, jpeg, heic, webp, tiff)",
                file.display()
            );
        }
    }
    let target = folder(paths, into)?;
    fs::create_dir_all(&target).with_context(|| format!("creating {}", target.display()))?;
    let mut added = Vec::new();
    for file in files {
        let raw = file.file_name().unwrap_or_default().to_string_lossy();
        let name = sanitise(&raw).context("file has no usable name")?;
        let dest = free_name(&target, &name);
        fs::copy(file, &dest).with_context(|| format!("copying {}", file.display()))?;
        added.push(dest);
    }
    Ok(added)
}

/// The library folder for `into` (a subfolder path, each part sanitised).
pub fn folder(paths: &Paths, into: Option<&str>) -> Result<PathBuf> {
    let mut target = dir(paths);
    if let Some(into) = into.map(str::trim).filter(|s| !s.is_empty()) {
        target.push(sanitise_path(into).with_context(|| format!("`{into}` can't be a folder"))?);
    }
    Ok(target)
}

/// `palette bg library remove`: delete a picture from the library, and
/// folders left empty by it (never the library folder itself).
pub fn remove(paths: &Paths, name: &str) -> Result<PathBuf> {
    let item = find(paths, name)?;
    fs::remove_file(&item.path).with_context(|| format!("removing {}", item.path.display()))?;
    let root = dir(paths);
    let mut parent = item.path.parent();
    while let Some(p) = parent {
        if p == root || !p.starts_with(&root) || fs::remove_dir(p).is_err() {
            break; // not empty (or not ours): stop
        }
        parent = p.parent();
    }
    Ok(item.path)
}

/// Use a library picture as the wallpaper now (until the next theme switch).
pub fn use_now(paths: &Paths, name: &str) -> Result<Outcome> {
    let item = find(paths, name)?;
    crate::effects::set_wallpaper(paths, &item.path)
}

/// `palette bg library theme`: copy a library picture into a theme's
/// wallpapers, or with `recolour` a copy painted in the theme's colours.
/// The library copy stays. Returns the theme's new file.
pub fn add_to_theme(paths: &Paths, name: &str, theme: &Theme, recolour: bool) -> Result<PathBuf> {
    let item = find(paths, name)?;
    copy_to_theme(&item.path, theme, recolour)
}

/// Copy (or recolour) one picture into a theme's backgrounds, never
/// replacing one that's there.
pub fn copy_to_theme(image: &Path, theme: &Theme, recolour: bool) -> Result<PathBuf> {
    if recolour {
        return studio::recolour(theme, image, Engine::Palette, studio::Strength::Balanced);
    }
    let dir = theme.backgrounds_dir();
    fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let file = image.file_name().context("picture has no name")?;
    let dest = free_name(&dir, &file.to_string_lossy());
    fs::copy(image, &dest).with_context(|| format!("copying {}", image.display()))?;
    Ok(dest)
}

/// `palette bg library new-theme`: a new theme worked out from a library
/// picture, with it as the theme's wallpaper (`palette new --from-wallpaper`).
pub fn new_theme(
    paths: &Paths,
    name: &str,
    theme_name: &str,
    light: Option<bool>,
    dry_run: bool,
) -> Result<Drafted> {
    let item = find(paths, name)?;
    studio::new_from_wallpaper(paths, theme_name, &item.path, light, dry_run)
}

/// Show a folder in Finder (`open`), creating it first so there's
/// something to show.
pub fn reveal_folder(dir: &Path) -> Result<()> {
    fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    system::reveal(dir).map_err(|e| anyhow::anyhow!(e))
}

// ---------------------------------------------------------------- names

/// A safe file or folder name: no separators, no leading dot, no control
/// characters, at most 120 bytes (keeping the extension). `None` if
/// nothing usable is left.
pub fn sanitise(name: &str) -> Option<String> {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '_'
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned.trim().trim_start_matches(['.', ' ']).to_string();
    if cleaned.is_empty() || cleaned.chars().all(|c| c == '_' || c == '.') {
        return None;
    }
    Some(shorten(&cleaned, 120))
}

/// Cut to `max` bytes on a character boundary, keeping a short extension.
fn shorten(name: &str, max: usize) -> String {
    if name.len() <= max {
        return name.to_string();
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() && e.len() <= 5 => (s, format!(".{e}")),
        _ => (name, String::new()),
    };
    let mut cut = max.saturating_sub(ext.len());
    while !stem.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{}{ext}", &stem[..cut])
}

/// A relative folder path with each part sanitised; `..`, absolute paths
/// and empty results are refused.
pub fn sanitise_path(path: &str) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for part in path.split(['/', '\\']).filter(|p| !p.is_empty()) {
        if part == "." || part == ".." {
            return None;
        }
        out.push(sanitise(part)?);
    }
    // Belt and braces: only plain names may remain.
    let plain = out.components().all(|c| matches!(c, Component::Normal(_)));
    (plain && out.components().count() > 0).then_some(out)
}

/// `dir/name`, or `dir/stem-2.ext`, `-3`… if that's taken.
pub fn free_name(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if fs::symlink_metadata(&first).is_err() {
        return first;
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s.to_string(), format!(".{e}")),
        _ => (name.to_string(), String::new()),
    };
    (2..)
        .map(|n| dir.join(format!("{stem}-{n}{ext}")))
        .find(|p| fs::symlink_metadata(p).is_err())
        .expect("some name is free")
}

/// `1.2 GB`, `340 MB`, `12 KB`.
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["bytes", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    match unit {
        0 => format!("{bytes} bytes"),
        _ if value < 10.0 => format!("{value:.1} {}", UNITS[unit]),
        _ => format!("{value:.0} {}", UNITS[unit]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_sanitised() {
        assert_eq!(sanitise("lake.png").as_deref(), Some("lake.png"));
        assert_eq!(sanitise("../x.png").as_deref(), Some("_x.png"));
        assert_eq!(sanitise(".hidden.png").as_deref(), Some("hidden.png"));
        assert_eq!(sanitise("a/b\\c:d.png").as_deref(), Some("a_b_c_d.png"));
        assert_eq!(sanitise("..").as_deref(), None);
        assert_eq!(sanitise("  ").as_deref(), None);
        assert_eq!(
            sanitise("Abstract & Artistic").as_deref(),
            Some("Abstract & Artistic")
        );
        let long = format!("{}.jpeg", "é".repeat(100));
        let short = sanitise(&long).unwrap();
        assert!(short.len() <= 120 && short.ends_with(".jpeg"), "{short}");
    }

    #[test]
    fn folder_paths_stay_inside() {
        assert_eq!(sanitise_path("a/b"), Some(PathBuf::from("a/b")));
        assert_eq!(sanitise_path("/a//b/"), Some(PathBuf::from("a/b")));
        assert_eq!(sanitise_path("a/../b"), None);
        assert_eq!(sanitise_path(""), None);
    }

    #[test]
    fn free_names_count_up() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(free_name(d.path(), "a.png"), d.path().join("a.png"));
        fs::write(d.path().join("a.png"), "x").unwrap();
        fs::write(d.path().join("a-2.png"), "x").unwrap();
        assert_eq!(free_name(d.path(), "a.png"), d.path().join("a-3.png"));
    }

    #[test]
    fn sizes() {
        assert_eq!(human_size(512), "512 bytes");
        assert_eq!(human_size(1_500_000), "1.5 MB");
        assert_eq!(human_size(340_000_000), "340 MB");
        assert_eq!(human_size(2_000_000_000), "2.0 GB");
    }

    #[test]
    fn add_list_remove_and_to_theme() {
        let home = tempfile::tempdir().unwrap();
        let paths = Paths::fake_home(home.path());
        let src = home.path().join("lake.png");
        fs::write(&src, "png").unwrap();
        let notes = home.path().join("notes.txt");
        fs::write(&notes, "hi").unwrap();
        assert!(add(&paths, std::slice::from_ref(&notes), None).is_err());

        add(&paths, std::slice::from_ref(&src), None).unwrap();
        add(&paths, std::slice::from_ref(&src), Some("Nord/Lakes")).unwrap();
        add(&paths, std::slice::from_ref(&src), Some("Nord/Lakes")).unwrap();
        assert!(add(&paths, std::slice::from_ref(&src), Some("../out")).is_err());
        // A symlink and a hidden partial download are not library items.
        #[cfg(unix)]
        std::os::unix::fs::symlink(&notes, dir(&paths).join("link.png")).unwrap();
        fs::write(dir(&paths).join(".x.png.part"), "p").unwrap();

        let lib = list(&paths).unwrap();
        let names: Vec<&str> = lib.items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(
            names,
            ["Nord/Lakes/lake-2.png", "Nord/Lakes/lake.png", "lake.png"]
        );
        assert_eq!(lib.items[0].folder, "Nord/Lakes");
        assert_eq!(lib.bytes, 9);
        assert!(find(&paths, "link.png").is_err());
        assert!(find(&paths, "../lake.png").is_err());

        let nord = theme::load(&paths, "nord").unwrap();
        let copied = add_to_theme(&paths, "Nord/Lakes/lake.png", &nord, false).unwrap();
        assert_eq!(copied, nord.backgrounds_dir().join("lake.png"));
        let again = add_to_theme(&paths, "lake.png", &nord, false).unwrap();
        assert_eq!(again, nord.backgrounds_dir().join("lake-2.png"));
        assert_eq!(folder_size(&nord.backgrounds_dir()), 6);

        remove(&paths, "Nord/Lakes/lake.png").unwrap();
        remove(&paths, "Nord/Lakes/lake-2.png").unwrap();
        assert!(!dir(&paths).join("Nord").exists(), "empty folders go too");
        assert!(dir(&paths).is_dir(), "the library folder stays");
        assert!(nord.backgrounds_dir().join("lake.png").is_file());
    }
}
