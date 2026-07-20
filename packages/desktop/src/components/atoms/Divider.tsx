import { Separator } from "@/components/ui/separator"
import { cn } from "@/lib/utils"

type DividerProps = {
  className?: string
  label?: string
}

export const Divider = ({ className, label }: DividerProps) => {
  if (label) {
    return (
      <div className={cn("flex items-center gap-3", className)} role="separator">
        <Separator className="flex-1" />
        <span className="text-xs text-muted-foreground">{label}</span>
        <Separator className="flex-1" />
      </div>
    )
  }

  return <Separator className={className} />
}
