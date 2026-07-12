import type { LanguageFn } from "highlight.js"
import rehypeHighlight from "rehype-highlight"

/** Core language pack for rehype-highlight — loaded as a separate chunk so
 * the main timeline bundle does not pull full highlight.js until settled
 * markdown needs it. Aliases mirror highlight.js common names. */
import bash from "highlight.js/lib/languages/bash"
import css from "highlight.js/lib/languages/css"
import go from "highlight.js/lib/languages/go"
import java from "highlight.js/lib/languages/java"
import javascript from "highlight.js/lib/languages/javascript"
import json from "highlight.js/lib/languages/json"
import markdown from "highlight.js/lib/languages/markdown"
import python from "highlight.js/lib/languages/python"
import rust from "highlight.js/lib/languages/rust"
import shell from "highlight.js/lib/languages/shell"
import sql from "highlight.js/lib/languages/sql"
import typescript from "highlight.js/lib/languages/typescript"
import xml from "highlight.js/lib/languages/xml"
import yaml from "highlight.js/lib/languages/yaml"

const languages: Record<string, LanguageFn> = {
  bash,
  sh: bash,
  shell,
  zsh: bash,
  css,
  go,
  java,
  javascript,
  js: javascript,
  jsx: javascript,
  json,
  markdown,
  md: markdown,
  python,
  py: python,
  rust,
  rs: rust,
  sql,
  typescript,
  ts: typescript,
  tsx: typescript,
  xml,
  html: xml,
  svg: xml,
  yaml,
  yml: yaml,
}

/** rehype-highlight configured with the core language subset. */
export const rehypeHighlightPlugin: [typeof rehypeHighlight, { languages: Record<string, LanguageFn> }] = [
  rehypeHighlight,
  { languages },
]
