//! Unified diff parser (the hunk bodies GitLab returns, with or without git headers).
use crate::gitlab::models::RawDiff;
use serde::Serialize;

/// One rendered diff row. Short field names keep the IPC payload small.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Line {
    /// '@' hunk header, '+' added, '-' removed, ' ' context, '\\' "no newline" marker.
    pub k: char,
    pub o: Option<u32>,
    pub n: Option<u32>,
    pub t: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileDiff {
    pub old_path: String,
    pub new_path: String,
    pub new_file: bool,
    pub renamed_file: bool,
    pub deleted_file: bool,
    pub too_large: bool,
    pub collapsed: bool,
    pub generated: bool,
    pub binary: bool,
    pub additions: u32,
    pub deletions: u32,
    pub lines: Vec<Line>,
}

/// Parse "@@ -a,b +c,d @@" → (a, c).
fn hunk_start(h: &str) -> Option<(u32, u32)> {
    let mut it = h.strip_prefix("@@ ")?.split(' ');
    let old = it.next()?.strip_prefix('-')?.split(',').next()?.parse().ok()?;
    let new = it.next()?.strip_prefix('+')?.split(',').next()?.parse().ok()?;
    Some((old, new))
}

pub fn parse(diff: &str) -> Vec<Line> {
    let mut out = Vec::new();
    let (mut o, mut n) = (0u32, 0u32);
    let mut in_hunk = false;
    for raw in diff.split('\n') {
        let l = raw.strip_suffix('\r').unwrap_or(raw);
        if l.starts_with("@@ ") {
            if let Some((a, c)) = hunk_start(l) {
                o = a;
                n = c;
                in_hunk = true;
                out.push(Line {
                    k: '@',
                    o: None,
                    n: None,
                    t: l.to_string(),
                });
                continue;
            }
        }
        if !in_hunk {
            continue; // git headers: diff --git, index, ---, +++, Binary files …
        }
        let (k, rest) = match l.chars().next() {
            Some(c @ ('+' | '-' | ' ' | '\\')) => (c, &l[1..]),
            None => continue, // trailing newline at end of the diff string
            Some(_) => {
                // A new git header inside a multi-file diff ends the hunk.
                in_hunk = false;
                continue;
            }
        };
        let line = match k {
            '+' => {
                n += 1;
                Line {
                    k,
                    o: None,
                    n: Some(n - 1),
                    t: rest.into(),
                }
            }
            '-' => {
                o += 1;
                Line {
                    k,
                    o: Some(o - 1),
                    n: None,
                    t: rest.into(),
                }
            }
            ' ' => {
                o += 1;
                n += 1;
                Line {
                    k,
                    o: Some(o - 1),
                    n: Some(n - 1),
                    t: rest.into(),
                }
            }
            _ => Line {
                k,
                o: None,
                n: None,
                t: rest.into(),
            },
        };
        out.push(line);
    }
    out
}

pub fn from_raw(r: RawDiff) -> FileDiff {
    let lines = parse(&r.diff);
    let additions = lines.iter().filter(|l| l.k == '+').count() as u32;
    let deletions = lines.iter().filter(|l| l.k == '-').count() as u32;
    // GitLab sends binaries as an empty diff or a "Binary files … differ" line.
    let binary = r.diff.starts_with("Binary files")
        || r.diff.contains("\nBinary files ")
        || (r.diff.is_empty() && !r.too_large && !r.collapsed && !r.renamed_file);
    FileDiff {
        old_path: r.old_path,
        new_path: r.new_path,
        new_file: r.new_file,
        renamed_file: r.renamed_file,
        deleted_file: r.deleted_file,
        too_large: r.too_large,
        collapsed: r.collapsed,
        generated: r.generated_file,
        binary,
        additions,
        deletions,
        lines,
    }
}

impl FileDiff {
    /// True if `line` is a new-side line (added or context) in this file.
    pub fn has_new_line(&self, line: u32) -> bool {
        self.lines.iter().any(|l| l.n == Some(line))
    }

    /// Reconstruct a unified diff text for the agent prompt.
    pub fn to_text(&self) -> String {
        let mut s = format!("--- a/{}\n+++ b/{}\n", self.old_path, self.new_path);
        for l in &self.lines {
            if l.k == '@' {
                s.push_str(&l.t);
            } else {
                s.push(l.k);
                s.push_str(&l.t);
            }
            s.push('\n');
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_lines_across_hunks() {
        let d = "@@ -1,3 +1,4 @@ fn x\n a\n-b\n+B\n+C\n c\n@@ -10,2 +11,2 @@\n x\n-y\n+z\n";
        let l = parse(d);
        assert_eq!(l[0].k, '@');
        assert_eq!((l[1].o, l[1].n), (Some(1), Some(1)));
        assert_eq!((l[2].k, l[2].o, l[2].n), ('-', Some(2), None));
        assert_eq!((l[3].k, l[3].n), ('+', Some(2)));
        assert_eq!((l[4].k, l[4].n), ('+', Some(3)));
        assert_eq!((l[5].o, l[5].n), (Some(3), Some(4)));
        assert_eq!((l[7].o, l[7].n), (Some(10), Some(11)));
        assert_eq!((l[9].k, l[9].n, l[9].t.as_str()), ('+', Some(12), "z"));
        assert_eq!(l.len(), 10);
    }

    #[test]
    fn skips_git_headers_and_handles_no_newline_marker() {
        let d = "diff --git a/f b/f\nindex 1..2 100644\n--- a/f\n+++ b/f\n@@ -1 +1 @@\n-old\n\\ No newline at end of file\n+new\n";
        let l = parse(d);
        assert_eq!(l.iter().map(|x| x.k).collect::<String>(), "@-\\+");
        assert_eq!(l[3].n, Some(1));
    }

    #[test]
    fn single_line_hunk_header_and_new_file() {
        let l = parse("@@ -0,0 +1,2 @@\n+a\n+b");
        assert_eq!(l[2].n, Some(2));
        assert_eq!(parse("").len(), 0);
        assert_eq!(parse("@@ garbage @@\n+a").len(), 0);
    }

    #[test]
    fn crlf_and_binary() {
        let l = parse("@@ -1 +1 @@\r\n-a\r\n+b\r\n");
        assert_eq!(l[2].t, "b");
        let f = from_raw(RawDiff {
            diff: "Binary files a/x.png and b/x.png differ\n".into(),
            ..Default::default()
        });
        assert!(f.binary);
        assert!(f.lines.is_empty());
    }

    #[test]
    fn counts_and_anchor_lookup() {
        let f = from_raw(RawDiff {
            new_path: "f".into(),
            diff: "@@ -1,2 +1,2 @@\n a\n-b\n+c\n".into(),
            ..Default::default()
        });
        assert_eq!((f.additions, f.deletions), (1, 1));
        assert!(f.has_new_line(2));
        assert!(!f.has_new_line(3));
        assert!(f.to_text().contains("+c\n"));
    }
}

#[cfg(test)]
mod bench {
    /// cargo test --release --lib diff::bench -- --ignored --nocapture
    #[test]
    #[ignore]
    fn parse_large_diff() {
        let hunk = "@@ -1,40 +1,42 @@\n".to_string()
            + &" context line with some code();\n-removed line\n+added line one\n+added line two\n".repeat(10);
        let big = hunk.repeat(5000); // ~200k lines
        let t = std::time::Instant::now();
        let mut n = 0;
        for _ in 0..5 {
            n += super::parse(&big).len();
        }
        println!(
            "parse: {:.1} ms per 200k-line diff ({n} lines)",
            t.elapsed().as_secs_f64() * 1000.0 / 5.0
        );
    }
}
