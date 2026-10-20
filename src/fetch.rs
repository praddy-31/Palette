//! Wallpapers from a link (`palette bg get <url>`, Settings → Wallpapers →
//! Get from a link). Two kinds of link:
//!
//! - a picture (any `https://` image, a GitHub `blob`/`raw` link): that one
//!   file is downloaded;
//! - a GitHub repo or folder (`github.com/o/r`, `…/tree/<branch>/<path>`):
//!   its pictures are *listed* without downloading the repo, through
//!   GitHub's git trees API (one unauthenticated request), then the ones you
//!   choose are downloaded one by one from `raw.githubusercontent.com`.
//!
//! Network: only here, only on an explicit request, through macOS's own
//! `curl` (https only, redirects too). Downloads are data: each must start
//! with a real picture's magic bytes (png, jpeg, webp, heic, tiff), is
//! capped in size, lands under a sanitised name inside the wallpaper library
//! (`library.rs`) and is never opened by anything but `sips` and the desktop.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};

use crate::library;
use crate::paths::Paths;
use crate::system;

/// One picture at most this big (same as theme installs).
pub const MAX_FILE: u64 = 40 * 1024 * 1024;
/// At most this many pictures per download…
pub const MAX_FILES: usize = 200;
/// …and this much in total, so one click can't fill the disk.
pub const MAX_TOTAL: u64 = 2_000_000_000;
/// GitHub's listing of a big repo is a few MB of JSON.
const MAX_LISTING: u64 = 40 * 1024 * 1024;

/// `curl` may only speak https (tests also read local `file://` fixtures).
const PROTOCOLS: &str = if cfg!(test) { "=https,file" } else { "=https" };

/// What a pasted link points at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Link {
    /// One picture: download it.
    Image { url: String, name: String },
    /// A GitHub repo (`path` empty) or a folder in it. `git_ref` is `HEAD`
    /// (the default branch) unless the link named a branch.
    GitHub {
        owner: String,
        repo: String,
        git_ref: String,
        path: String,
    },
}

impl Link {
    /// Understand a pasted link (see the module notes for the kinds).
    pub fn parse(input: &str) -> Result<Link> {
        let s = input.trim();
        let bare = [
            "github.com/",
            "www.github.com/",
            "raw.githubusercontent.com/",
        ];
        let s = if bare.iter().any(|b| s.starts_with(b)) {
            format!("https://{s}")
        } else {
            s.to_string()
        };
        if s.starts_with("http://") {
            bail!("use an https:// link (plain http isn't downloaded)");
        }
        let Some(rest) = s.strip_prefix("https://") else {
            bail!("`{input}` isn't a link: paste one starting with https://");
        };
        let rest = rest.split(['?', '#']).next().unwrap_or_default();
        let url = format!("https://{rest}");
        let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
        let host = host.to_ascii_lowercase();
        let segs: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
        let last = || {
            segs.last()
                .map(|s| decode(s))
                .unwrap_or_else(|| "image".into())
        };
        match host.trim_start_matches("www.") {
            "github.com" => {
                let (Some(owner), Some(repo)) = (segs.first(), segs.get(1)) else {
                    bail!("{url} isn't a GitHub repo link (github.com/<owner>/<repo>)");
                };
                let repo = repo.trim_end_matches(".git");
                if !plain(owner) || !plain(repo) {
                    bail!("{url} isn't a GitHub repo link (github.com/<owner>/<repo>)");
                }
                let (owner, repo) = (owner.to_string(), repo.to_string());
                let git_ref = match segs.get(3) {
                    Some(r) => decode(r),
                    None => "HEAD".into(),
                };
                if !git_ref
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "-_./".contains(c))
                    || git_ref.contains("..")
                {
                    bail!("`{git_ref}` doesn't look like a branch name");
                }
                let rest: Vec<String> = segs.iter().skip(4).map(|s| decode(s)).collect();
                if rest
                    .iter()
                    .any(|p| p == ".." || p == "." || p.chars().any(char::is_control))
                {
                    bail!("{url}: odd path");
                }
                let path = rest.join("/");
                match segs.get(2).copied() {
                    None => Ok(Link::GitHub {
                        owner,
                        repo,
                        git_ref,
                        path: String::new(),
                    }),
                    Some("tree") if segs.len() >= 4 => Ok(Link::GitHub {
                        owner,
                        repo,
                        git_ref,
                        path,
                    }),
                    Some("blob" | "raw") if segs.len() >= 5 => Ok(Link::Image {
                        url: raw_url(&owner, &repo, &git_ref, &path),
                        name: last(),
                    }),
                    _ => bail!(
                        "{url} isn't a repo, folder or picture link on GitHub \
                         (github.com/<owner>/<repo>, …/tree/<branch>/<folder>, …/blob/…)"
                    ),
                }
            }
            "" => bail!("`{input}` has no host"),
            _ => Ok(Link::Image { url, name: last() }),
        }
    }

    /// The library subfolder downloads go to unless told otherwise: the
    /// repo's name plus the linked folder, so the library mirrors the repo.
    pub fn default_folder(&self) -> String {
        match self {
            Link::GitHub { repo, path, .. } => library::sanitise_path(&format!("{repo}/{path}"))
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_else(|| "github".into()),
            Link::Image { .. } => "downloads".into(),
        }
    }

    /// `owner/repo` or `owner/repo/folder`, or the picture's link.
    pub fn describe(&self) -> String {
        match self {
            Link::GitHub {
                owner, repo, path, ..
            } if path.is_empty() => format!("{owner}/{repo}"),
            Link::GitHub {
                owner, repo, path, ..
            } => format!("{owner}/{repo}/{path}"),
            Link::Image { url, .. } => url.clone(),
        }
    }

    /// The link to paste for one folder of this repo (after a truncated listing).
    pub fn folder_link(&self, folder: &str) -> Option<String> {
        match self {
            Link::GitHub {
                owner,
                repo,
                git_ref,
                ..
            } => Some(format!(
                "https://github.com/{owner}/{repo}/tree/{}/{}",
                encode(git_ref, true),
                encode(folder, true)
            )),
            Link::Image { .. } => None,
        }
    }
}

fn plain(s: &str) -> bool {
    !s.is_empty()
        && s != "."
        && s != ".."
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
}

/// `https://raw.githubusercontent.com/o/r/<ref>/<path>`, path encoded.
pub fn raw_url(owner: &str, repo: &str, git_ref: &str, path: &str) -> String {
    format!(
        "https://raw.githubusercontent.com/{owner}/{repo}/{}/{}",
        encode(git_ref, true),
        encode(path, true)
    )
}

/// The trees API request listing everything under `path` (or the repo).
fn tree_url(owner: &str, repo: &str, git_ref: &str, path: &str) -> String {
    let tree = match path.is_empty() {
        true => encode(git_ref, true),
        false => format!("{}:{}", encode(git_ref, true), encode(path, true)),
    };
    format!("https://api.github.com/repos/{owner}/{repo}/git/trees/{tree}?recursive=1")
}

/// Percent-encode all but unreserved characters (and `/` when `keep_slash`).
fn encode(s: &str, keep_slash: bool) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) || (keep_slash && b == b'/') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// `%20` → space (invalid escapes are kept as they are).
fn decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(hex) = s.get(i + 1..i + 3)
            && let Ok(b) = u8::from_str_radix(hex, 16)
        {
            out.push(b);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

// ---------------------------------------------------------------- listing

/// A picture in a remote repo: its path from the repo root and size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteFile {
    pub path: String,
    pub bytes: u64,
}

#[derive(Debug, Clone)]
pub struct Listing {
    pub link: Link,
    /// Pictures only, sorted by path.
    pub files: Vec<RemoteFile>,
    /// GitHub stopped listing part-way (very big repos): paste a folder link.
    pub truncated: bool,
}

/// A folder of a listing: pictures directly inside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folder {
    pub path: String,
    pub count: usize,
    pub bytes: u64,
}

/// What a link offers. A picture link needs no request; a GitHub link
/// costs one call to GitHub's API (60 an hour are allowed without an account).
pub fn list(link: &Link) -> Result<Listing> {
    match link {
        Link::Image { name, .. } => Ok(Listing {
            link: link.clone(),
            files: vec![RemoteFile {
                path: name.clone(),
                bytes: 0,
            }],
            truncated: false,
        }),
        Link::GitHub {
            owner,
            repo,
            git_ref,
            path,
        } => {
            let url = tree_url(owner, repo, git_ref, path);
            let max = MAX_LISTING.to_string();
            let text = system::run(
                "curl",
                &[
                    "-fsSL",
                    "--proto",
                    PROTOCOLS,
                    "--proto-redir",
                    PROTOCOLS,
                    "--max-redirs",
                    "5",
                    "--connect-timeout",
                    "20",
                    "--max-time",
                    "120",
                    "--max-filesize",
                    &max,
                    "-H",
                    "Accept: application/vnd.github+json",
                    &url,
                ],
            )
            .map_err(|e| github_error(link, &e))?;
            let (files, truncated) = parse_tree(&text, path)?;
            Ok(Listing {
                link: link.clone(),
                files,
                truncated,
            })
        }
    }
}

/// A friendlier message for GitHub's usual refusals.
fn github_error(link: &Link, curl: &str) -> anyhow::Error {
    if curl.contains("404") {
        anyhow!(
            "GitHub has no {} (a typo, a private repo, or a folder that isn't there)",
            link.describe()
        )
    } else if curl.contains("403") || curl.contains("429") {
        anyhow!(
            "GitHub said no for now: it allows 60 listings an hour without an account. \
             Try again in a while."
        )
    } else {
        anyhow!("listing {}: {curl}", link.describe())
    }
}

/// Pictures in a git trees API answer, paths prefixed with `prefix` (the
/// folder the listing started at), plus whether GitHub truncated it.
pub fn parse_tree(text: &str, prefix: &str) -> Result<(Vec<RemoteFile>, bool)> {
    let value = json::parse(text).context("GitHub's answer isn't valid JSON")?;
    let tree = value
        .get("tree")
        .and_then(json::Value::as_array)
        .context("unexpected answer from GitHub (no `tree`)")?;
    let prefix = prefix.trim_matches('/');
    let mut files = Vec::new();
    for entry in tree {
        let kind = entry.get("type").and_then(json::Value::as_str);
        let mode = entry.get("mode").and_then(json::Value::as_str);
        let Some(path) = entry.get("path").and_then(json::Value::as_str) else {
            continue;
        };
        // Blobs only: not folders, submodules ("commit") or symlinks (120000).
        if kind != Some("blob") || mode == Some("120000") {
            continue;
        }
        let full = match prefix.is_empty() {
            true => path.to_string(),
            false => format!("{prefix}/{path}"),
        };
        if !crate::theme::is_image(Path::new(&full)) {
            continue;
        }
        let bytes = entry.get("size").and_then(json::Value::as_u64).unwrap_or(0);
        files.push(RemoteFile { path: full, bytes });
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let truncated = matches!(value.get("truncated"), Some(json::Value::Bool(true)));
    Ok((files, truncated))
}

/// The folders pictures sit in, with counts and sizes, sorted by path
/// (`""` = the top of the listing).
pub fn folders(files: &[RemoteFile]) -> Vec<Folder> {
    let mut out: Vec<Folder> = Vec::new();
    for f in files {
        let path = f.path.rsplit_once('/').map_or("", |(d, _)| d);
        match out.iter_mut().find(|d| d.path == path) {
            Some(d) => {
                d.count += 1;
                d.bytes += f.bytes;
            }
            None => out.push(Folder {
                path: path.to_string(),
                count: 1,
                bytes: f.bytes,
            }),
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// Choose from a listing: everything under `folder` (and its subfolders),
/// names matching `pick` (`*` and `?`, case-insensitive, against the path or
/// the file name), or `all`. At least one of them is needed.
pub fn select(
    files: &[RemoteFile],
    folder: Option<&str>,
    pick: Option<&str>,
    all: bool,
) -> Result<Vec<RemoteFile>> {
    if folder.is_none() && pick.is_none() && !all {
        bail!("choose what to download: --folder <path>, --pick <pattern> or --all");
    }
    let folder = folder.map(|f| f.trim_matches('/'));
    let chosen: Vec<RemoteFile> = files
        .iter()
        .filter(|f| match folder {
            Some("") | None => true,
            Some(d) => f.path.starts_with(&format!("{d}/")),
        })
        .filter(|f| match pick {
            None => true,
            Some(p) => {
                let name = f.path.rsplit('/').next().unwrap_or_default();
                glob(p, &f.path) || glob(p, name)
            }
        })
        .cloned()
        .collect();
    if chosen.is_empty() {
        bail!("nothing matches; run `palette bg get <link>` alone to see the folders");
    }
    Ok(chosen)
}

/// `*` (any run, `/` included) and `?` (one character), ignoring case.
pub fn glob(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    let t: Vec<char> = text.to_lowercase().chars().collect();
    let (mut pi, mut ti) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some((pi, ti));
            pi += 1;
        } else if let Some((sp, st)) = star {
            pi = sp + 1;
            ti = st + 1;
            star = Some((sp, st + 1));
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|c| *c == '*')
}

/// Refuse a selection over the caps, saying why.
pub fn check_caps(files: &[RemoteFile]) -> Result<()> {
    if files.len() > MAX_FILES {
        bail!(
            "that's {} pictures; Palette downloads at most {MAX_FILES} at a time so one click \
             can't fill your disk. Pick a folder (--folder) or a pattern (--pick).",
            files.len()
        );
    }
    let total: u64 = files.iter().map(|f| f.bytes).sum();
    if total > MAX_TOTAL {
        bail!(
            "that's {} of pictures; Palette downloads at most {} at a time. \
             Pick a folder (--folder) or a pattern (--pick).",
            library::human_size(total),
            library::human_size(MAX_TOTAL)
        );
    }
    Ok(())
}

// ---------------------------------------------------------------- downloading

#[derive(Debug, Default)]
pub struct Downloaded {
    /// The library folder they went to.
    pub dir: PathBuf,
    pub saved: Vec<PathBuf>,
    /// (path, why) for each picture that wasn't saved.
    pub skipped: Vec<(String, String)>,
    pub bytes: u64,
}

/// Download `files` (from `list(link)`) into the library, under `into`
/// (default: the repo's name, or `downloads` for a single picture). Folders
/// inside the link are kept. One bad file is skipped, not fatal.
pub fn download(
    paths: &Paths,
    link: &Link,
    files: &[RemoteFile],
    into: Option<&str>,
) -> Result<Downloaded> {
    check_caps(files)?;
    let default = link.default_folder();
    let dir = library::folder(paths, Some(into.unwrap_or(&default)))?;
    let mut done = Downloaded {
        dir: dir.clone(),
        ..Default::default()
    };
    for file in files {
        if file.bytes > MAX_FILE {
            done.skipped.push((
                file.path.clone(),
                format!(
                    "{} is over the {} limit",
                    library::human_size(file.bytes),
                    library::human_size(MAX_FILE)
                ),
            ));
            continue;
        }
        let github = matches!(link, Link::GitHub { .. });
        if github
            && (file.path.split('/').any(|s| s == ".." || s == ".")
                || !crate::theme::is_image(Path::new(&file.path)))
        {
            done.skipped
                .push((file.path.clone(), "not a picture's path".into()));
            continue;
        }
        if done.bytes > MAX_TOTAL {
            done.skipped
                .push((file.path.clone(), "the 2 GB limit was reached".into()));
            continue;
        }
        let (url, rel) = match link {
            Link::Image { url, .. } => (url.clone(), file.path.clone()),
            Link::GitHub {
                owner,
                repo,
                git_ref,
                path,
            } => {
                let rel = match path.is_empty() {
                    true => file.path.as_str(),
                    false => file
                        .path
                        .strip_prefix(&format!("{path}/"))
                        .unwrap_or(&file.path),
                };
                (raw_url(owner, repo, git_ref, &file.path), rel.to_string())
            }
        };
        let (sub, name) = match rel.rsplit_once('/') {
            Some((d, n)) => (library::sanitise_path(d), n.to_string()),
            None => (None, rel.clone()),
        };
        let target = match sub {
            Some(s) => dir.join(s),
            None => dir.clone(),
        };
        let name = library::sanitise(&name).unwrap_or_else(|| "image".into());
        match save_one(&url, &target, &name, MAX_FILE) {
            Ok(saved) => {
                done.bytes += fs::metadata(&saved).map_or(0, |m| m.len());
                done.saved.push(saved);
            }
            Err(e) => done.skipped.push((file.path.clone(), format!("{e:#}"))),
        }
    }
    Ok(done)
}

/// Download one picture into `dir` as `name` (renamed if taken, extension
/// fixed to what the file really is). Hidden `.part` file until checked.
fn save_one(url: &str, dir: &Path, name: &str, cap: u64) -> Result<PathBuf> {
    fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let part = dir.join(format!(".{name}.part"));
    let _ = fs::remove_file(&part);
    let part_s = part.to_string_lossy().to_string();
    let cap_s = cap.to_string();
    let got = system::run(
        "curl",
        &[
            "-fsSL",
            "--proto",
            PROTOCOLS,
            "--proto-redir",
            PROTOCOLS,
            "--max-redirs",
            "5",
            "--connect-timeout",
            "20",
            "--max-time",
            "300",
            "--max-filesize",
            &cap_s,
            "-o",
            &part_s,
            url,
        ],
    );
    let result = (|| {
        got.map_err(|e| {
            if e.contains("Maximum file size") || e.contains("(63)") {
                anyhow!("over the {} limit", library::human_size(cap))
            } else {
                anyhow!("{e}")
            }
        })?;
        let meta = fs::symlink_metadata(&part).context("nothing was downloaded")?;
        if !meta.is_file() {
            bail!("not a regular file");
        }
        if meta.len() > cap {
            bail!("over the {} limit", library::human_size(cap));
        }
        let mut head = [0u8; 16];
        let n = fs::File::open(&part)?.read(&mut head)?;
        let kind = image_kind(&head[..n]).context(
            "not a picture (png, jpg, webp, heic or tiff); a Git LFS file or a web page?",
        )?;
        let dest = library::free_name(dir, &with_extension(name, kind));
        fs::rename(&part, &dest)?;
        Ok(dest)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&part);
    }
    result
}

/// Picture types macOS shows as a wallpaper, told apart by their first bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Png,
    Jpeg,
    Webp,
    Heic,
    Tiff,
}

/// What a file really is, from its first 12+ bytes.
pub fn image_kind(head: &[u8]) -> Option<Kind> {
    if head.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(Kind::Png)
    } else if head.starts_with(&[0xff, 0xd8, 0xff]) {
        Some(Kind::Jpeg)
    } else if head.len() >= 12 && &head[..4] == b"RIFF" && &head[8..12] == b"WEBP" {
        Some(Kind::Webp)
    } else if head.starts_with(b"II*\0") || head.starts_with(b"MM\0*") {
        Some(Kind::Tiff)
    } else if head.len() >= 12
        && &head[4..8] == b"ftyp"
        && [
            &b"heic"[..],
            b"heix",
            b"hevc",
            b"hevx",
            b"heim",
            b"heis",
            b"mif1",
            b"msf1",
        ]
        .contains(&&head[8..12])
    {
        Some(Kind::Heic)
    } else {
        None
    }
}

/// `name` with an extension matching `kind` (kept when it already does).
fn with_extension(name: &str, kind: Kind) -> String {
    let ok: &[&str] = match kind {
        Kind::Png => &["png"],
        Kind::Jpeg => &["jpg", "jpeg"],
        Kind::Webp => &["webp"],
        Kind::Heic => &["heic"],
        Kind::Tiff => &["tiff"],
    };
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s, e.to_ascii_lowercase()),
        _ => (name, String::new()),
    };
    if ok.contains(&ext.as_str()) {
        return name.to_string();
    }
    // A wrong or missing image extension is replaced; anything else is kept.
    let known = ["png", "jpg", "jpeg", "webp", "heic", "tiff", "tif", "gif"];
    let base = if known.contains(&ext.as_str()) {
        stem
    } else {
        name
    };
    format!("{base}.{}", ok[0])
}

// ---------------------------------------------------------------- JSON

/// Just enough JSON for GitHub's tree listing: no serde in the core
/// for one response format.
pub mod json {
    #[derive(Debug, Clone, PartialEq)]
    pub enum Value {
        Null,
        Bool(bool),
        Num(f64),
        Str(String),
        Arr(Vec<Value>),
        Obj(Vec<(String, Value)>),
    }

    impl Value {
        pub fn get(&self, key: &str) -> Option<&Value> {
            match self {
                Value::Obj(pairs) => pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v),
                _ => None,
            }
        }
        pub fn as_str(&self) -> Option<&str> {
            match self {
                Value::Str(s) => Some(s),
                _ => None,
            }
        }
        pub fn as_u64(&self) -> Option<u64> {
            match self {
                Value::Num(n) if *n >= 0.0 => Some(*n as u64),
                _ => None,
            }
        }
        pub fn as_array(&self) -> Option<&Vec<Value>> {
            match self {
                Value::Arr(a) => Some(a),
                _ => None,
            }
        }
    }

    pub fn parse(text: &str) -> Option<Value> {
        let mut p = Parser {
            b: text.as_bytes(),
            i: 0,
        };
        let v = p.value(0)?;
        p.ws();
        (p.i == p.b.len()).then_some(v)
    }

    struct Parser<'a> {
        b: &'a [u8],
        i: usize,
    }

    impl Parser<'_> {
        fn ws(&mut self) {
            while self.i < self.b.len() && self.b[self.i].is_ascii_whitespace() {
                self.i += 1;
            }
        }

        fn eat(&mut self, c: u8) -> bool {
            self.ws();
            let ok = self.b.get(self.i) == Some(&c);
            if ok {
                self.i += 1;
            }
            ok
        }

        fn value(&mut self, depth: usize) -> Option<Value> {
            if depth > 64 {
                return None;
            }
            self.ws();
            match *self.b.get(self.i)? {
                b'{' => {
                    self.i += 1;
                    let mut pairs = Vec::new();
                    if self.eat(b'}') {
                        return Some(Value::Obj(pairs));
                    }
                    loop {
                        self.ws();
                        let key = self.string()?;
                        if !self.eat(b':') {
                            return None;
                        }
                        pairs.push((key, self.value(depth + 1)?));
                        if self.eat(b'}') {
                            return Some(Value::Obj(pairs));
                        }
                        if !self.eat(b',') {
                            return None;
                        }
                    }
                }
                b'[' => {
                    self.i += 1;
                    let mut items = Vec::new();
                    if self.eat(b']') {
                        return Some(Value::Arr(items));
                    }
                    loop {
                        items.push(self.value(depth + 1)?);
                        if self.eat(b']') {
                            return Some(Value::Arr(items));
                        }
                        if !self.eat(b',') {
                            return None;
                        }
                    }
                }
                b'"' => self.string().map(Value::Str),
                b't' => self.word("true", Value::Bool(true)),
                b'f' => self.word("false", Value::Bool(false)),
                b'n' => self.word("null", Value::Null),
                _ => {
                    let start = self.i;
                    while self.i < self.b.len() && b"+-.eE0123456789".contains(&self.b[self.i]) {
                        self.i += 1;
                    }
                    std::str::from_utf8(&self.b[start..self.i])
                        .ok()?
                        .parse()
                        .ok()
                        .map(Value::Num)
                }
            }
        }

        fn word(&mut self, w: &str, v: Value) -> Option<Value> {
            if self.b[self.i..].starts_with(w.as_bytes()) {
                self.i += w.len();
                Some(v)
            } else {
                None
            }
        }

        fn hex4(&mut self) -> Option<u32> {
            let s = std::str::from_utf8(self.b.get(self.i..self.i + 4)?).ok()?;
            self.i += 4;
            u32::from_str_radix(s, 16).ok()
        }

        fn string(&mut self) -> Option<String> {
            if self.b.get(self.i) != Some(&b'"') {
                return None;
            }
            self.i += 1;
            let mut out: Vec<u8> = Vec::new();
            loop {
                let c = *self.b.get(self.i)?;
                self.i += 1;
                match c {
                    b'"' => return String::from_utf8(out).ok(),
                    b'\\' => {
                        let e = *self.b.get(self.i)?;
                        self.i += 1;
                        let ch = match e {
                            b'n' => '\n',
                            b't' => '\t',
                            b'r' => '\r',
                            b'b' => '\u{8}',
                            b'f' => '\u{c}',
                            b'u' => {
                                let hi = self.hex4()?;
                                let code = if (0xd800..0xdc00).contains(&hi)
                                    && self.b.get(self.i..self.i + 2) == Some(b"\\u")
                                {
                                    self.i += 2;
                                    let lo = self.hex4()?;
                                    0x10000
                                        + ((hi - 0xd800) << 10)
                                        + (lo.wrapping_sub(0xdc00) & 0x3ff)
                                } else {
                                    hi
                                };
                                char::from_u32(code).unwrap_or('\u{fffd}')
                            }
                            other => other as char,
                        };
                        let mut buf = [0u8; 4];
                        out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                    }
                    _ => out.push(c),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gh(owner: &str, repo: &str, git_ref: &str, path: &str) -> Link {
        Link::GitHub {
            owner: owner.into(),
            repo: repo.into(),
            git_ref: git_ref.into(),
            path: path.into(),
        }
    }

    #[test]
    fn links() {
        assert_eq!(
            Link::parse("https://github.com/SleepyCatHey/CozyPixels").unwrap(),
            gh("SleepyCatHey", "CozyPixels", "HEAD", "")
        );
        assert_eq!(
            Link::parse("github.com/a/b.git/").unwrap(),
            gh("a", "b", "HEAD", "")
        );
        assert_eq!(
            Link::parse(
                "https://github.com/SleepyCatHey/CozyPixels/tree/main/Catppuccin/Abstract%20%26%20Artistic?x=1"
            )
            .unwrap(),
            gh("SleepyCatHey", "CozyPixels", "main", "Catppuccin/Abstract & Artistic")
        );
        assert_eq!(
            Link::parse("https://github.com/a/b/blob/main/walls/My%20Lake.png").unwrap(),
            Link::Image {
                url: "https://raw.githubusercontent.com/a/b/main/walls/My%20Lake.png".into(),
                name: "My Lake.png".into()
            }
        );
        assert!(matches!(
            Link::parse("https://raw.githubusercontent.com/a/b/main/x.jpg").unwrap(),
            Link::Image { name, .. } if name == "x.jpg"
        ));
        assert_eq!(
            Link::parse("https://example.com/pics/sunset.webp#top").unwrap(),
            Link::Image {
                url: "https://example.com/pics/sunset.webp".into(),
                name: "sunset.webp".into()
            }
        );
        assert!(Link::parse("http://example.com/a.png").is_err());
        assert!(Link::parse("ftp://example.com/a.png").is_err());
        assert!(Link::parse("https://github.com/a").is_err());
        assert!(Link::parse("https://github.com/a/b;rm/x").is_err());
        assert!(Link::parse("https://github.com/a/b/issues/3").is_err());
        assert!(Link::parse("https://github.com/a/b/tree/main/%2E%2E/x").is_err());
    }

    #[test]
    fn urls_are_encoded() {
        assert_eq!(
            tree_url("o", "r", "HEAD", "Nord/Cozy & Warm"),
            "https://api.github.com/repos/o/r/git/trees/HEAD:Nord/Cozy%20%26%20Warm?recursive=1"
        );
        assert_eq!(
            tree_url("o", "r", "HEAD", ""),
            "https://api.github.com/repos/o/r/git/trees/HEAD?recursive=1"
        );
        assert_eq!(
            raw_url("o", "r", "HEAD", "a b/c#1.png"),
            "https://raw.githubusercontent.com/o/r/HEAD/a%20b/c%231.png"
        );
        let link = gh("o", "r", "HEAD", "");
        assert_eq!(
            link.folder_link("Nord").unwrap(),
            "https://github.com/o/r/tree/HEAD/Nord"
        );
    }

    /// Trimmed from a real `git/trees/HEAD?recursive=1` answer.
    const TREE: &str = r#"{
      "sha": "9a72", "url": "https://api.github.com/x",
      "tree": [
        {"path": "README.md", "mode": "100644", "type": "blob", "sha": "1", "size": 1819, "url": "u"},
        {"path": "Nord", "mode": "040000", "type": "tree", "sha": "2", "url": "u"},
        {"path": "Nord/lake.png", "mode": "100644", "type": "blob", "sha": "3", "size": 2048000, "url": "u"},
        {"path": "Nord/Cozy & Warm/fire.JPG", "mode": "100644", "type": "blob", "sha": "4", "size": 512, "url": "u"},
        {"path": "Nord/link.png", "mode": "120000", "type": "blob", "sha": "5", "size": 9, "url": "u"},
        {"path": "vendor", "mode": "160000", "type": "commit", "sha": "6", "url": "u"},
        {"path": "Gruvbox/hills.webp", "mode": "100644", "type": "blob", "sha": "7", "size": 100, "url": "u"},
        {"path": "Gruvbox/café.jpeg", "mode": "100644", "type": "blob", "sha": "8", "size": 1e2, "url": "u"}
      ],
      "truncated": false
    }"#;

    #[test]
    fn reads_the_tree_listing() {
        let (files, truncated) = parse_tree(TREE, "").unwrap();
        assert!(!truncated);
        let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                "Gruvbox/café.jpeg",
                "Gruvbox/hills.webp",
                "Nord/Cozy & Warm/fire.JPG",
                "Nord/lake.png"
            ]
        );
        assert_eq!(files[3].bytes, 2_048_000);
        assert_eq!(
            folders(&files),
            [
                Folder {
                    path: "Gruvbox".into(),
                    count: 2,
                    bytes: 200
                },
                Folder {
                    path: "Nord".into(),
                    count: 1,
                    bytes: 2_048_000
                },
                Folder {
                    path: "Nord/Cozy & Warm".into(),
                    count: 1,
                    bytes: 512
                },
            ]
        );
        // A folder listing (`HEAD:Nord`) has paths relative to the folder.
        let (inside, _) = parse_tree(
            r#"{"tree":[{"path":"lake.png","type":"blob","mode":"100644","size":1}],"truncated":true}"#,
            "Nord",
        )
        .unwrap();
        assert_eq!(inside[0].path, "Nord/lake.png");
        assert!(parse_tree(r#"{"tree":[],"truncated":true}"#, "").unwrap().1);
        assert!(parse_tree("{\"message\": \"Not Found\"}", "").is_err());
        assert!(parse_tree("{\"tree\": [", "").is_err());
    }

    #[test]
    fn json_edge_cases() {
        let v = json::parse(r#" {"a": [1, -2.5e1, true, null, "x\"\\\/😀"], "b": {}} "#).unwrap();
        let a = v.get("a").unwrap().as_array().unwrap();
        assert_eq!(a[1], json::Value::Num(-25.0));
        assert_eq!(a[4].as_str(), Some("x\"\\/😀"));
        assert!(json::parse("[1,]").is_none());
        assert!(
            json::parse(&"[".repeat(500)).is_none(),
            "deep nesting refused"
        );
    }

    #[test]
    fn choosing() {
        let (files, _) = parse_tree(TREE, "").unwrap();
        assert!(select(&files, None, None, false).is_err());
        assert_eq!(select(&files, None, None, true).unwrap().len(), 4);
        assert_eq!(select(&files, Some("Nord/"), None, false).unwrap().len(), 2);
        assert_eq!(select(&files, Some("Nor"), None, false).ok(), None);
        assert_eq!(
            select(&files, None, Some("*.jp*g"), false).unwrap().len(),
            2
        );
        assert_eq!(
            select(&files, Some("Nord"), Some("l*"), false).unwrap()[0].path,
            "Nord/lake.png"
        );
        assert!(glob("gruvbox/*", "Gruvbox/hills.webp"));
        assert!(glob("h?lls.*", "hills.webp"));
        assert!(!glob("*.png", "hills.webp"));
    }

    #[test]
    fn caps() {
        let many: Vec<RemoteFile> = (0..=MAX_FILES)
            .map(|i| RemoteFile {
                path: format!("{i}.png"),
                bytes: 1,
            })
            .collect();
        assert!(check_caps(&many[..MAX_FILES]).is_ok());
        assert!(
            check_caps(&many)
                .unwrap_err()
                .to_string()
                .contains("at most 200")
        );
        let big = vec![
            RemoteFile {
                path: "a.png".into(),
                bytes: MAX_TOTAL,
            },
            RemoteFile {
                path: "b.png".into(),
                bytes: 1,
            },
        ];
        assert!(check_caps(&big).unwrap_err().to_string().contains("2.0 GB"));
    }

    #[test]
    fn magic_bytes() {
        assert_eq!(image_kind(b"\x89PNG\r\n\x1a\n...."), Some(Kind::Png));
        assert_eq!(image_kind(&[0xff, 0xd8, 0xff, 0xe0]), Some(Kind::Jpeg));
        assert_eq!(image_kind(b"RIFF\0\0\0\0WEBPVP8 "), Some(Kind::Webp));
        assert_eq!(image_kind(b"\0\0\0\x18ftypheic\0\0"), Some(Kind::Heic));
        assert_eq!(image_kind(b"MM\0*\0\0"), Some(Kind::Tiff));
        assert_eq!(image_kind(b"<!doctype html>"), None);
        assert_eq!(image_kind(b"version https://git-lfs"), None);
        assert_eq!(image_kind(b"\0\0\0\x18ftypavif\0\0"), None);
        assert_eq!(with_extension("a.png", Kind::Png), "a.png");
        assert_eq!(with_extension("a.JPG", Kind::Jpeg), "a.JPG");
        assert_eq!(with_extension("a.png", Kind::Jpeg), "a.jpg");
        assert_eq!(with_extension("photo", Kind::Webp), "photo.webp");
        assert_eq!(with_extension("v1.2", Kind::Png), "v1.2.png");
    }

    /// Downloads go through curl; a `file://` fixture stands in for GitHub.
    #[test]
    fn saves_only_real_pictures_under_the_cap() {
        let src = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        let png = src.path().join("p.png");
        let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
        bytes.extend([0u8; 100]);
        fs::write(&png, &bytes).unwrap();
        let html = src.path().join("page.png");
        fs::write(&html, "<html>not a picture</html>").unwrap();
        let url = |p: &Path| format!("file://{}", p.display());

        let saved = save_one(&url(&png), out.path(), "lake.jpg", 1000).unwrap();
        assert_eq!(
            saved,
            out.path().join("lake.png"),
            "extension follows the bytes"
        );
        let again = save_one(&url(&png), out.path(), "lake.png", 1000).unwrap();
        assert_eq!(again, out.path().join("lake-2.png"));

        let e = save_one(&url(&html), out.path(), "page.png", 1000).unwrap_err();
        assert!(format!("{e:#}").contains("not a picture"), "{e:#}");
        let e = save_one(&url(&png), out.path(), "big.png", 50).unwrap_err();
        assert!(format!("{e:#}").contains("limit"), "{e:#}");
        let left: Vec<_> = fs::read_dir(out.path()).unwrap().flatten().collect();
        assert_eq!(left.len(), 2, "no .part files left behind");
    }

    #[test]
    #[ignore = "uses the network"]
    fn lists_a_real_repo() {
        let link = Link::parse("https://github.com/tokyo-night/wallpapers").unwrap();
        let listing = list(&link).unwrap();
        assert!(!listing.files.is_empty());
    }
}
