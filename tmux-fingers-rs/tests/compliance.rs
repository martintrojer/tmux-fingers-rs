use pcre2::bytes::Regex;
use tmux_fingers_rs::fingers::config::builtin_patterns;

fn matches_for(pattern_name: &str, input: &str) -> Vec<String> {
    let pattern = builtin_patterns()[pattern_name];
    let regex = Regex::new(pattern).expect("valid pattern");
    regex
        .captures_iter(input.as_bytes())
        .map(|captures| {
            let captures = captures.expect("captures");
            let capture = captures
                .name("match")
                .or_else(|| captures.get(0))
                .expect("match");
            String::from_utf8(input.as_bytes()[capture.start()..capture.end()].to_vec()).unwrap()
        })
        .collect()
}

#[test]
fn matches_ip_addresses() {
    let input = "
      foo
        192.168.0.1
        127.0.0.1
        foofofo
    ";
    assert_eq!(matches_for("ip", input), vec!["192.168.0.1", "127.0.0.1"]);
}

#[test]
fn matches_uuids() {
    let input = "
      foo
      d6f4b4ac-4b78-4d79-96a1-eb9ab72f2c59
      7a8e24d1-5a81-4f5a-bc6a-9d7f9818a8c4
      e5c3dcf0-9b01-45c2-8327-6d9d4bb8a0c8
      2fa5c6e9-33f9-46b7-ba89-3f17b12e59e5
      b882bfc5-6b24-43a7-ae1e-8f9ea14eeff2
      bar
    ";
    assert_eq!(
        matches_for("uuid", input),
        vec![
            "d6f4b4ac-4b78-4d79-96a1-eb9ab72f2c59",
            "7a8e24d1-5a81-4f5a-bc6a-9d7f9818a8c4",
            "e5c3dcf0-9b01-45c2-8327-6d9d4bb8a0c8",
            "2fa5c6e9-33f9-46b7-ba89-3f17b12e59e5",
            "b882bfc5-6b24-43a7-ae1e-8f9ea14eeff2",
        ]
    );
}

#[test]
fn matches_shas() {
    let input = "
      foo
      fc4fea27210bc0d85b74f40866e12890e3788134
      fc4fea2
      bar
    ";
    assert_eq!(
        matches_for("sha", input),
        vec!["fc4fea27210bc0d85b74f40866e12890e3788134", "fc4fea2"]
    );
}

#[test]
fn matches_digits() {
    let input = "
      foo
      12345
      67891011
      bar
    ";
    assert_eq!(matches_for("digit", input), vec!["12345", "67891011"]);
}

#[test]
fn matches_urls() {
    let input = "
      foo
      https://geocities.com
      bar
    ";
    assert_eq!(matches_for("url", input), vec!["https://geocities.com"]);
}

#[test]
fn matches_paths() {
    let input = "
      absolute paths /foo/bar/lol
      relative paths ./foo/bar/lol
      home paths ~/foo/bar/lol
      bar
    ";
    assert_eq!(
        matches_for("path", input),
        vec!["/foo/bar/lol", "./foo/bar/lol", "~/foo/bar/lol"]
    );
}

#[test]
fn matches_hex_numbers() {
    let input = "
      hello 0xcafe
      0xcaca
      0xdeadbeef hehehe 0xCACA
    ";
    assert_eq!(
        matches_for("hex", input),
        vec!["0xcafe", "0xcaca", "0xdeadbeef", "0xCACA"]
    );
}

#[test]
fn matches_git_status_lines() {
    let input = r#"
Your branch is up to date with 'origin/crystal-rewrite'.

Changes to be committed:
  (use "git restore --staged <file>..." to unstage)
        deleted:    CHANGELOG.md
        new file:   wat

Changes not staged for commit:
  (use "git add <file>..." to update what will be committed)
  (use "git restore <file>..." to discard changes in working directory)
        modified:   Makefile
        modified:   spec/lib/patterns_spec.cr
        modified:   src/fingers/config.cr
    "#;
    assert_eq!(
        matches_for("git-status", input),
        vec![
            "CHANGELOG.md",
            "wat",
            "Makefile",
            "spec/lib/patterns_spec.cr",
            "src/fingers/config.cr",
        ]
    );
}

#[test]
fn matches_git_status_branch() {
    let input = "
Your branch is up to date with 'origin/crystal-rewrite'.
    ";
    assert_eq!(
        matches_for("git-status-branch", input),
        vec!["origin/crystal-rewrite"]
    );
}

#[test]
fn matches_diff_paths() {
    let input = "
  diff --git a/spec/lib/patterns_spec.cr b/spec/lib/patterns_spec.cr
  index 5281097..6c9c18e 100644
  --- a/spec/lib/patterns_spec.cr
  +++ b/spec/lib/patterns_spec.cr
  ";
    assert_eq!(
        matches_for("diff", input),
        vec!["spec/lib/patterns_spec.cr", "spec/lib/patterns_spec.cr"]
    );
}
