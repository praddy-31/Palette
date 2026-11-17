//! One top-level key in a JSON settings file, possibly with comments (Zed's
//! settings.json, Herdr's, micro's): read its value as text, swap it, add it, or
//! delete the line. Everything else in the file (comments, order, spacing)
//! stays byte for byte, so `init --remove` can put the user's own text back.

/// A small reader for JSON with comments: just enough to find a top-level
/// key's value without touching anything else.
struct Scan<'a> {
    b: &'a [u8],
}

impl Scan<'_> {
    /// Skip spaces, `// …` and `/* … */`.
    fn blank(&self, mut i: usize) -> usize {
        loop {
            while i < self.b.len() && self.b[i].is_ascii_whitespace() {
                i += 1;
            }
            match self.b.get(i..i + 2) {
                Some(b"//") => {
                    while i < self.b.len() && self.b[i] != b'\n' {
                        i += 1;
                    }
                }
                Some(b"/*") => {
                    i += 2;
                    while i < self.b.len() && self.b.get(i..i + 2) != Some(b"*/") {
                        i += 1;
                    }
                    i = (i + 2).min(self.b.len());
                }
                _ => return i,
            }
        }
    }

    /// `i` at an opening quote → just past the closing one.
    fn string(&self, mut i: usize) -> Option<usize> {
        i += 1;
        while i < self.b.len() {
            match self.b[i] {
                b'\\' => i += 2,
                b'"' => return Some(i + 1),
                _ => i += 1,
            }
        }
        None
    }

    /// `i` at the start of a value → just past its end.
    fn value(&self, i: usize) -> Option<usize> {
        match *self.b.get(i)? {
            b'"' => self.string(i),
            b'{' | b'[' => {
                let mut depth = 0usize;
                let mut j = i;
                while j < self.b.len() {
                    match self.b[j] {
                        b'"' => {
                            j = self.string(j)?;
                            continue;
                        }
                        b'/' if matches!(self.b.get(j + 1), Some(b'/' | b'*')) => {
                            j = self.blank(j);
                            continue;
                        }
                        b'{' | b'[' => depth += 1,
                        b'}' | b']' => {
                            depth -= 1;
                            if depth == 0 {
                                return Some(j + 1);
                            }
                        }
                        _ => {}
                    }
                    j += 1;
                }
                None
            }
            _ => {
                let mut j = i;
                while j < self.b.len() && !b",}]\n/".contains(&self.b[j]) {
                    j += 1;
                }
                let end = self.b[i..j]
                    .iter()
                    .rposition(|c| !c.is_ascii_whitespace())?;
                Some(i + end + 1)
            }
        }
    }
}

/// The top-level `key` entry: (where its key starts, value start, value end).
fn span(text: &str, key: &str) -> Option<(usize, usize, usize)> {
    let s = Scan { b: text.as_bytes() };
    let quoted = format!("\"{key}\"");
    let mut i = s.blank(0);
    if s.b.get(i) != Some(&b'{') {
        return None;
    }
    i += 1;
    loop {
        i = s.blank(i);
        if s.b.get(i)? != &b'"' {
            return None;
        }
        let key_start = i;
        let key_end = s.string(i)?;
        i = s.blank(key_end);
        if s.b.get(i)? != &b':' {
            return None;
        }
        let start = s.blank(i + 1);
        let end = s.value(start)?;
        if text[key_start..key_end] == quoted {
            return Some((key_start, start, end));
        }
        i = s.blank(end);
        match s.b.get(i)? {
            b',' => i += 1,
            _ => return None,
        }
    }
}

/// The value of top-level `key`, as JSON text (`"Palette"`, `{…}`).
pub fn read(text: &str, key: &str) -> Option<String> {
    let (_, start, end) = span(text, key)?;
    Some(text[start..end].to_string())
}

/// Swap `key`'s value (`Some`, JSON text), add the setting at the top, or
/// (`None`) delete its line. An empty text counts as `{}`.
pub fn write(text: &str, key: &str, value: Option<&str>) -> String {
    match (value, span(text, key)) {
        (Some(v), Some((_, start, end))) => format!("{}{v}{}", &text[..start], &text[end..]),
        (Some(v), None) => match text.find('{') {
            Some(brace) => {
                let rest = &text[brace + 1..];
                let next = &rest[Scan { b: rest.as_bytes() }.blank(0)..];
                let comma = if next.is_empty() || next.starts_with('}') {
                    ""
                } else {
                    ","
                };
                format!("{}\n    \"{key}\": {v}{comma}{rest}", &text[..=brace])
            }
            None => format!("{{\n    \"{key}\": {v}\n}}\n"),
        },
        (None, Some((k, _, end))) => {
            let line_start = text[..k].rfind('\n').map_or(0, |i| i + 1);
            let line_end = text[end..].find('\n').map_or(text.len(), |i| end + i + 1);
            format!("{}{}", &text[..line_start], &text[line_end..])
        }
        (None, None) => text.to_string(),
    }
}

/// `"name"` as JSON text.
pub fn quoted(name: &str) -> String {
    format!("\"{name}\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn other_keys_and_empty_files() {
        let text = "{\n  \"zoom\": 1,\n  \"appearance\": \"dark\"\n}\n";
        assert_eq!(read(text, "appearance").as_deref(), Some("\"dark\""));
        let wired = write(text, "theme", Some("\"palette\""));
        assert_eq!(read(&wired, "theme").as_deref(), Some("\"palette\""));
        assert_eq!(write(&wired, "theme", None), text);
        assert_eq!(
            write("", "colorscheme", Some("\"palette\"")),
            "{\n    \"colorscheme\": \"palette\"\n}\n"
        );
    }
}
