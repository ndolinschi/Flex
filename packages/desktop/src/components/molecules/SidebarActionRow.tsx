import type { ComponentType } from "react"
import { Kbd } from "../atoms"
import {
  Item,
  ItemActions,
  ItemContent,
  ItemMedia,
  ItemTitle,
} from "@/components/ui/item"
import { cn } from "../../lib/utils"

type SidebarActionRowProps = {
  icon: ComponentType<{ className?: string; "aria-hidden"?: boolean }>
  label: string
  kbd?: string
  /** Outlink marker (e.g. `ArrowUpRight`) for rows that open a separate pane
   * — rendered right-aligned in place of the `kbd` slot. */
  trailingIcon?: ComponentType<{ className?: string; "aria-hidden"?: boolean }>
  onClick?: () => void
  disabled?: boolean
}

/** 28px sidebar action row: icon + label + trailing shortcut (shadcn Item). */
export const SidebarActionRow = ({
  icon: Icon,
  label,
  kbd,
  trailingIcon: TrailingIcon,
  onClick,
  disabled = false,
}: SidebarActionRowProps) => {
  return (
    <Item
      asChild
      size="xs"
      variant="default"
      className={cn(
        "min-h-7 rounded-sm border-transparent px-2 py-1.5",
        "text-ink-secondary hover:bg-fill-4 hover:text-ink",
        "transition-colors duration-[var(--duration-fast)] ease-[var(--easing-default)]",
        "focus-visible:ring-0 focus-visible:border-transparent",
        "disabled:pointer-events-none disabled:opacity-50",
      )}
    >
      <button type="button" onClick={onClick} disabled={disabled}>
        <ItemMedia className="translate-y-0 self-center text-icon-2">
          <Icon className="h-3.5 w-3.5 shrink-0" aria-hidden />
        </ItemMedia>
        <ItemContent className="gap-0">
          <ItemTitle className="font-normal text-sm leading-none text-inherit">
            {label}
          </ItemTitle>
        </ItemContent>
        <ItemActions className="gap-0">
          {TrailingIcon ? (
            <TrailingIcon
              className="h-3 w-3 shrink-0 text-ink-faint"
              aria-hidden
            />
          ) : kbd ? (
            <Kbd className="shrink-0 border-0 bg-transparent px-0 font-sans text-xs tracking-[var(--tracking-caption)] text-ink-faint shadow-none">
              {kbd}
            </Kbd>
          ) : null}
        </ItemActions>
      </button>
    </Item>
  )
}
