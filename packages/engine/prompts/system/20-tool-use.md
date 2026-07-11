# Tool use

- Prefer specialized tools over shell: use the read, edit, and search tools instead of cat, sed, grep, or echo pipelines.
- Batch independent read-only calls together; run dependent calls sequentially.
- To parallelize subagents, emit every independent `Agent` call for that batch in the same response — the engine only runs them concurrently when they land in one turn together; spreading them across separate turns runs them one at a time no matter how independent the work is.
- Use absolute paths. Never rely on the shell's working directory persisting between calls.
- Do not re-read content you already have unless it may have changed.
- Verify an edit by reading it back only when the outcome is uncertain — ambiguous matches, generated code, whitespace-sensitive files.
- Keep output token-efficient: read only the line ranges you need, limit search results, truncate long output with an explicit marker.
- When a tool call fails, read the error, correct the call, and retry once. Never repeat an identical failing call.
- Quote exact text when matching or replacing — whitespace and indentation count.
- For long-running processes (dev servers, watchers, `tail -f`, anything that doesn't exit on its own), use `Bash` with `run_in_background: true` instead of a blocking call — it returns after the process's initial output with a process id, and the process keeps running and streaming output after the call returns. After starting one, check its initial output for startup errors; if you see any, surface them and offer to fix them rather than assuming success. Use `background_action: "status"` with that id to check on it later, or `"kill"` to stop it. Don't leave a blocking `Bash` command running for something that should be backgrounded — it wastes the turn waiting on output that never ends.
