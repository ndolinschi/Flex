const READONLY_BINS: &[&str] = &[
    "ls", "cat", "head", "tail", "wc", "echo", "printf", "pwd", "stat", "file", "du", "df", "date",
    "whoami", "id", "uname", "hostname", "which", "type", "basename", "dirname", "realpath",
    "readlink", "tree", "sort", "uniq", "cut", "tr", "tac", "nl", "cmp", "diff", "column", "jq",
    "yq", "grep", "rg", "ag", "fd", "ps", "true", "false", "test",
];

const READONLY_GIT_SUBCOMMANDS: &[&str] = &[
    "status",
    "log",
    "diff",
    "show",
    "branch",
    "remote",
    "rev-parse",
    "describe",
    "blame",
    "ls-files",
    "ls-tree",
    "shortlog",
    "reflog",
    "cat-file",
    "whatchanged",
    "grep",
    "name-rev",
    "symbolic-ref",
    "for-each-ref",
    "count-objects",
    "var",
    "rev-list",
    "tag",
];

fn has_side_effect_syntax(command: &str) -> bool {
    if command.contains("$(")
        || command.contains('`')
        || command.contains("<(")
        || command.contains(">(")
    {
        return true;
    }
    let mut scrubbed = command.to_owned();
    for safe in [
        "2>&1",
        "2>/dev/null",
        "1>/dev/null",
        ">/dev/null",
        "&>/dev/null",
    ] {
        scrubbed = scrubbed.replace(safe, "");
    }
    scrubbed.contains('>')
}

pub(crate) fn command_is_read_only(command: &str) -> bool {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return false;
    }
    if has_side_effect_syntax(trimmed) {
        return false;
    }
    for segment in trimmed.split(['|', ';', '\n']).flat_map(|s| s.split("&&")) {
        if !segment_is_read_only(segment) {
            return false;
        }
    }
    true
}

fn segment_is_read_only(segment: &str) -> bool {
    let mut tokens = segment
        .split_whitespace()
        .skip_while(|t| is_env_assignment(t));
    let Some(bin) = tokens.next() else {
        return true;
    };
    let bin = strip_path(bin);
    if bin == "git" {
        return match tokens.find(|t| !t.starts_with('-')) {
            Some(sub) => READONLY_GIT_SUBCOMMANDS.contains(&sub),
            None => true,
        };
    }
    if bin == "find" {
        let acts = segment.split_whitespace().any(|t| {
            matches!(
                t,
                "-delete" | "-exec" | "-execdir" | "-ok" | "-okdir" | "-fprint" | "-fls"
            )
        });
        return !acts;
    }
    READONLY_BINS.contains(&bin)
}

fn is_env_assignment(token: &str) -> bool {
    match token.split_once('=') {
        Some((name, _)) => {
            !name.is_empty()
                && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                && name.chars().next().is_some_and(|c| !c.is_ascii_digit())
        }
        None => false,
    }
}

fn strip_path(bin: &str) -> &str {
    bin.rsplit('/').next().unwrap_or(bin)
}

#[cfg(test)]
mod tests {
    use super::command_is_read_only;

    #[test]
    fn allows_common_inspection_commands() {
        for cmd in [
            "ls -la",
            "cat src/main.rs",
            "git log --oneline -20",
            "git diff HEAD~1",
            "git status",
            "rg fn\\s+main",
            "grep -n TODO src/lib.rs",
            "find . -name '*.rs'",
            "cat a | grep b | wc -l",
            "git show HEAD && ls",
            "RUST_LOG=debug ls",
            "/usr/bin/cat file",
            "grep foo bar 2>/dev/null",
        ] {
            assert!(command_is_read_only(cmd), "should be read-only: {cmd}");
        }
    }

    #[test]
    fn denies_mutating_or_unproven_commands() {
        for cmd in [
            "rm -rf build",
            "echo hi > file",
            "cat a >> b",
            "git commit -m x",
            "git push",
            "git checkout main",
            "find . -name '*.tmp' -delete",
            "find . -exec rm {} \\;",
            "sed -i s/a/b/ f",
            "tee out.txt",
            "cargo build",
            "npm install",
            "python script.py",
            "bash -c 'rm x'",
            "ls $(rm x)",
            "echo `rm x`",
            "ls && rm x",
            "cat a; rm b",
            "totally-unknown-tool",
            "",
        ] {
            assert!(!command_is_read_only(cmd), "should NOT be read-only: {cmd}");
        }
    }
}
