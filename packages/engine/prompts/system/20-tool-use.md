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
