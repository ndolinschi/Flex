import type { RefObject } from "react"
import { ChevronDown, Search, X } from "lucide-react"
import { cn } from "../../lib/utils"
import {
  InputGroup,
  InputGroupAddon,
  InputGroupButton,
  InputGroupInput,
  InputGroupText,
} from "@/components/ui/input-group"

export type PlanFindState = {
  query: string
  onQueryChange: (q: string) => void
  matchCount: number
  activeIndex: number
  onNext: () => void
  onPrev: () => void
  open: boolean
  onOpenChange: (open: boolean) => void
}

type PlanFindBarProps = {
  find: PlanFindState
  inputRef: RefObject<HTMLInputElement | null>
}

/** Inline find-in-plan chrome strip (InputGroup, h-6). */
export const PlanFindBar = ({ find, inputRef }: PlanFindBarProps) => (
  <div className="flex h-8 items-center border-y border-border px-2.5">
    <InputGroup
      className={cn(
        "h-6 min-w-0 flex-1 border-0 bg-transparent shadow-none dark:bg-transparent",
        "has-[[data-slot=input-group-control]:focus-visible]:border-transparent",
        "has-[[data-slot=input-group-control]:focus-visible]:ring-0",
      )}
    >
      <InputGroupAddon align="inline-start" className="pl-0 py-0">
        <Search className="size-3.5 text-muted-foreground" aria-hidden />
      </InputGroupAddon>
      <InputGroupInput
        ref={inputRef}
        type="text"
        value={find.query}
        onChange={(e) => find.onQueryChange(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            e.preventDefault()
            find.onOpenChange(false)
            return
          }
          if (e.key === "Enter") {
            e.preventDefault()
            if (e.shiftKey) find.onPrev()
            else find.onNext()
          }
        }}
        placeholder="Find in plan"
        aria-label="Find in plan"
        className="h-6 px-0 text-sm"
      />
      <InputGroupAddon align="inline-end" className="pr-0 py-0 gap-0.5">
        <InputGroupText className="text-xs tabular-nums">
          {find.matchCount > 0
            ? `${find.activeIndex + 1}/${find.matchCount}`
            : "0/0"}
        </InputGroupText>
        <InputGroupButton
          size="icon-xs"
          aria-label="Previous match"
          title="Previous match"
          onClick={find.onPrev}
          disabled={find.matchCount === 0}
          className="text-muted-foreground hover:bg-fill-4 hover:text-foreground"
        >
          <ChevronDown className="rotate-180" aria-hidden />
        </InputGroupButton>
        <InputGroupButton
          size="icon-xs"
          aria-label="Next match"
          title="Next match"
          onClick={find.onNext}
          disabled={find.matchCount === 0}
          className="text-muted-foreground hover:bg-fill-4 hover:text-foreground"
        >
          <ChevronDown aria-hidden />
        </InputGroupButton>
        <InputGroupButton
          size="icon-xs"
          aria-label="Close find"
          title="Close find"
          onClick={() => find.onOpenChange(false)}
          className="text-muted-foreground hover:bg-fill-4 hover:text-foreground"
        >
          <X aria-hidden />
        </InputGroupButton>
      </InputGroupAddon>
    </InputGroup>
  </div>
)
