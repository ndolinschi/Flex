import { ConfirmDialog } from "../../molecules"
import { Input } from "@/components/ui/input"
import { basename } from "../../../lib/utils"

export type FileExplorerDialogState =
  | { kind: "create"; prefix?: string }
  | { kind: "rename"; path: string }
  | { kind: "delete"; path: string }
  | null

type FileExplorerDialogsProps = {
  dialog: FileExplorerDialogState
  draftPath: string
  setDraftPath: (value: string) => void
  busy: boolean
  confirmDisabled: boolean
  onConfirm: () => void
  onCancel: () => void
}

export const FileExplorerDialogs = ({
  dialog,
  draftPath,
  setDraftPath,
  busy,
  confirmDisabled,
  onConfirm,
  onCancel,
}: FileExplorerDialogsProps) => (
  <>
    <ConfirmDialog
      open={dialog?.kind === "create"}
      title="New file"
      description="Repo-relative path (e.g. src/utils.ts). Parent folders are created automatically."
      confirmLabel="Create"
      confirmDisabled={confirmDisabled}
      isLoading={busy}
      onConfirm={onConfirm}
      onCancel={onCancel}
    >
      <Input
        value={draftPath}
        onChange={(e) => setDraftPath(e.target.value)}
        placeholder="path/to/file.ts"
        aria-label="New file path"
        onKeyDown={(e) => {
          if (e.key === "Enter" && !confirmDisabled && !busy) {
            e.preventDefault()
            onConfirm()
          }
        }}
      />
    </ConfirmDialog>

    <ConfirmDialog
      open={dialog?.kind === "rename"}
      title="Rename file"
      description={
        dialog?.kind === "rename"
          ? `Rename ${basename(dialog.path)}`
          : undefined
      }
      confirmLabel="Rename"
      confirmDisabled={confirmDisabled}
      isLoading={busy}
      onConfirm={onConfirm}
      onCancel={onCancel}
    >
      <Input
        value={draftPath}
        onChange={(e) => setDraftPath(e.target.value)}
        placeholder="filename.ts"
        aria-label="New file name"
        onKeyDown={(e) => {
          if (e.key === "Enter" && !confirmDisabled && !busy) {
            e.preventDefault()
            onConfirm()
          }
        }}
      />
    </ConfirmDialog>

    <ConfirmDialog
      open={dialog?.kind === "delete"}
      title="Delete file"
      description={
        dialog?.kind === "delete"
          ? `Permanently delete ${dialog.path}? This cannot be undone.`
          : undefined
      }
      confirmLabel="Delete"
      danger
      isLoading={busy}
      onConfirm={onConfirm}
      onCancel={onCancel}
    />
  </>
)
