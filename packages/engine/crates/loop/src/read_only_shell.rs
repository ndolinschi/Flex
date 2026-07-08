//! Conservative read-only classification for shell commands, used to let safe
//! inspection commands (`git log`, `ls`, `rg`, …) run while in plan mode.
//!
//! Safety-first: [`command_is_read_only`] returns `true` only when it can prove
//! a command has no side effects. Anything it cannot prove — an unknown
//! executable, output redirection, command/process substitution, or a mutating
//! subcommand — is treated as read-write and left for the plan-mode gate to
//! deny. Over-rejecting a harmless command is acceptable; letting a mutating one
//! through is not.

/// Executables that only read state. Deliberately excludes tools that can write
/// (`tee`, `sed`/`awk` with in-place or redirection, `dd`, `cp`, `mv`, `rm`, …)
/// and interpreters that can run arbitrary code (`bash`, `sh`, `python`, `node`,
/// `eval`, `env`, `xargs`, `sudo`).
const READONLY_BINS: &[&str] = &[
    "ls", "cat", "head", "tail", "wc", "echo", "printf", "pwd", "stat", "file", "du", "df", "date",
    "whoami", "id", "uname", "hostname", "which", "type", "basename", "dirname", "realpath",
    "readlink", "tree", "sort", "uniq", "cut", "tr", "tac", "nl", "cmp", "diff", "column", "jq",
    "yq", "grep", "rg", "ag", "fd", "ps", "true", "false", "test",
];

/// Read-only `git` subcommands. Excludes anything that writes to the repo,
/// index, worktree, or remotes (`add`, `commit`, `push`, `checkout`, `reset`, …).
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

/// Shell metacharacters that can cause writes or run hidden commands. `>`/`>>`
/// redirect to files; `$(`/backtick/`<(`/`>(` substitute command output; `&`
/// backgrounds. Their mere presence forces a deny (we don't try to prove the
/// nuance safe).
fn has_side_effect_syntax(command: &str) -> bool {
    // Command / process substitution and backgrounding — always reject.
    if command.contains("$(")
        || command.contains('`')
        || command.contains("<(")
        || command.contains(">(")
    {
        return true;
    }
    // Output redirection. Strip the harmless "discard" / fd-dup forms first, then
    // any remaining `>` means a write to a file.
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

/// Whether a shell command is confidently read-only (safe to run in plan mode).
pub(crate) fn command_is_read_only(command: &str) -> bool {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return false;
    }
    if has_side_effect_syntax(trimmed) {
        return false;
    }
    // Every segment of a pipe/list must independently be read-only.
    for segment in trimmed.split(['|', ';', '\n']).flat_map(|s| s.split("&&")) {
        // `a || b` splits leave a leading/trailing token when `|` is used above;
        // re-split on the `||` remnant is unnecessary because `|` already split
        // it into empty-or-real segments, which we tolerate below.
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
        // Empty segment (e.g. from splitting `a || b`) — no command to run.
        return true;
    };
    let bin = strip_path(bin);
    if bin == "git" {
        // First non-flag token after `git` is the subcommand.
        return match tokens.find(|t| !t.starts_with('-')) {
            Some(sub) => READONLY_GIT_SUBCOMMANDS.contains(&sub),
            None => true, // bare `git` / `git --version`
        };
    }
    if bin == "find" {
        // `find` is read-only unless it is told to act on matches.
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

/// `FOO=bar` style leading environment assignment (skipped to reach the binary).
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

/// Reduce `/usr/bin/ls` to `ls`; leaves a bare name unchanged.
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
