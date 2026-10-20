//! Small preview images for the picker. A theme's `preview.png`, or else its
//! first wallpaper, shrunk to 480 px with macOS's own `sips` and cached in
//! `~/.config/palette/cache/thumbs/`. Wallpapers can be 6000×4000; the
//! picker shows two dozen of them at once.

use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};

use crate::paths::Paths;
use crate::system;
use crate::theme::Theme;

const SIZE: &str = "480";

/// The image a theme is previewed with, if it has one.
pub fn source(theme: &Theme) -> Result<Option<PathBuf>> {
    let preview = theme.dir.join("preview.png");
    if fs::symlink_metadata(&preview).is_ok_and(|m| m.is_file()) {
        return Ok(Some(preview));
    }
    Ok(theme.backgrounds()?.into_iter().next())
}

/// A cached JPEG thumbnail for `theme`, made on first use. `None` when the
/// theme has no image to show (the picker draws its colours instead).
pub fn thumbnail(paths: &Paths, theme: &Theme) -> Result<Option<PathBuf>> {
    match source(theme)? {
        Some(src) => image(paths, &src).map(Some),
        None => Ok(None),
    }
}

/// A cached 480 px JPEG of any image (each wallpaper in the app's list).
/// The cache key includes size and modification time, so a replaced image
/// gets a new thumbnail.
pub fn image(paths: &Paths, src: &Path) -> Result<PathBuf> {
    let meta = fs::metadata(src)?;
    let mut hasher = DefaultHasher::new();
    src.hash(&mut hasher);
    meta.len().hash(&mut hasher);
    meta.modified().ok().hash(&mut hasher);
    let dir = paths.root().join("cache/thumbs");
    let thumb = dir.join(format!("{:016x}.jpg", hasher.finish()));
    if thumb.is_file() {
        return Ok(thumb);
    }
    fs::create_dir_all(&dir)?;
    let tmp = dir.join(format!(".{:016x}.tmp.jpg", hasher.finish()));
    let (src_arg, tmp_arg) = (src.to_string_lossy(), tmp.to_string_lossy());
    system::run(
        "sips",
        &[
            "-Z", SIZE, "-s", "format", "jpeg", &src_arg, "--out", &tmp_arg,
        ],
    )
    .map_err(|e| anyhow!(e))?;
    fs::rename(&tmp, &thumb)?;
    Ok(thumb)
}
