import { dbMentionTables } from "../../lib/tauri"
import type { UiMentionHit } from "../types"

/** @-mention provider for tables on live Database plugin connections. */
export const searchDatabaseMentions = async (
  query: string,
  _cwd: string | undefined,
): Promise<UiMentionHit[]> => {
  try {
    const hits = await dbMentionTables(query)
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
