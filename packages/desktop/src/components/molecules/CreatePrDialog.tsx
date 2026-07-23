import { useEffect, useState } from "react"
import { Textarea } from "@/components/ui/textarea"

import { ConfirmDialog } from "./ConfirmDialog"
import { FormField } from "./FormField"
import { Input } from "@/components/ui/input"

type CreatePrDialogProps = {
  open: boolean
  initialTitle?: string
  initialBody?: string
  isLoading?: boolean
  onConfirm: (title: string, body: string) => void
  onCancel: () => void
}

export const CreatePrDialog = ({
  open,
  initialTitle = "",
  initialBody = "",
  isLoading = false,
  onConfirm,
  onCancel,
}: CreatePrDialogProps) => {
  const [title, setTitle] = useState(initialTitle)
  const [body, setBody] = useState(initialBody)

  useEffect(() => {
    if (!open) return
    setTitle(initialTitle)
    setBody(initialBody)
  }, [open, initialTitle, initialBody])

  const trimmedTitle = title.trim()

  return (
    <ConfirmDialog
      open={open}
      title="Create pull request"
      description="Title and description are sent to GitHub. Leave the description empty if the title is enough."
      confirmLabel="Create PR"
      isLoading={isLoading}
      confirmDisabled={!trimmedTitle}
      onCancel={onCancel}
      onConfirm={() => {
        if (!trimmedTitle || isLoading) return
        onConfirm(trimmedTitle, body)
      }}
    >
      <div className="flex flex-col gap-3">
        <FormField label="Title" htmlFor="create-pr-title">
          <Input
            id="create-pr-title"
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            placeholder="Pull request title"
            aria-label="Pull request title"
            disabled={isLoading}
          />
        </FormField>
        <FormField label="Description" htmlFor="create-pr-body" hint="Optional">
          <Textarea
            id="create-pr-body"
            value={body}
            onChange={(e) => setBody(e.target.value)}
            placeholder="What should reviewers know?"
            aria-label="Pull request description"
            rows={5}
            disabled={isLoading}
            className="min-h-[7rem] resize-y text-sm"
          />
        </FormField>
      </div>
    </ConfirmDialog>
  )
}
