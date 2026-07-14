import { dbMentionTables } from "../../lib/tauri"
import type { UiMentionHit } from "../types"

/** @-mention provider for tables on live Database plugin connections
 * for the active project cwd. */
export const searchDatabaseMentions = async (
  query: string,
  cwd: string | undefined,
): Promise<UiMentionHit[]> => {
  const projectKey = cwd?.trim() ?? ""
  if (!projectKey) return []
  try {
    const hits = await dbMentionTables(query, projectKey)
    return hits.map((h) => ({
      kind: "table" as const,
      name: h.name,
      path: h.path,
      insertText: h.insertText,
    }))
  } catch {
    return []
  }
}
