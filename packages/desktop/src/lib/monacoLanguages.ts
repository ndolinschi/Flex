export const languageForPath = (path: string): string => {
  const name = path.split("/").pop()?.toLowerCase() ?? ""
  const ext = name.includes(".") ? name.slice(name.lastIndexOf(".") + 1) : name
  switch (ext) {
    case "ts":
    case "mts":
    case "cts":
      return "typescript"
    case "tsx":
      return "typescript"
    case "js":
    case "mjs":
    case "cjs":
      return "javascript"
    case "jsx":
      return "javascript"
    case "json":
      return "json"
    case "css":
      return "css"
    case "scss":
      return "scss"
    case "less":
      return "less"
    case "html":
    case "htm":
      return "html"
    case "md":
    case "mdx":
      return "markdown"
    case "rs":
      return "rust"
    case "py":
      return "python"
    case "toml":
      return "ini"
    case "yml":
    case "yaml":
      return "yaml"
    case "sh":
    case "bash":
    case "zsh":
      return "shell"
    case "sql":
      return "sql"
    case "xml":
    case "svg":
      return "xml"
    case "go":
      return "go"
    case "java":
      return "java"
    case "kt":
      return "kotlin"
    case "swift":
      return "swift"
    case "c":
    case "h":
      return "c"
    case "cpp":
    case "cc":
    case "hpp":
      return "cpp"
    case "rb":
      return "ruby"
    case "php":
      return "php"
    case "dockerfile":
      return "dockerfile"
    default:
      if (name === "dockerfile") return "dockerfile"
      if (name === "makefile") return "plaintext"
      return "plaintext"
  }
}
