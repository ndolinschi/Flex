import { Switch as SwitchPrimitive } from "@base-ui/react/switch"

import { cn } from "@/lib/utils"

/** Settings binary control — ON track is green (`--color-switch-on`), not
 * primary/accent. Sizes match the former Toggle atom (30×18 / 24×14). */
function Switch({
  className,
  size = "default",
  ...props
}: SwitchPrimitive.Root.Props & {
  size?: "sm" | "default"
}) {
  return (
    <SwitchPrimitive.Root
      data-slot="switch"
      data-size={size}
      className={cn(
        "peer group/switch relative inline-flex shrink-0 items-center rounded-full border border-transparent transition-colors duration-200 outline-none after:absolute after:-inset-x-3 after:-inset-y-2 focus-visible:border-stroke-2 focus-visible:ring-1 focus-visible:ring-stroke-2 aria-invalid:border-destructive aria-invalid:ring-3 aria-invalid:ring-destructive/20 data-[size=default]:h-[18px] data-[size=default]:w-[30px] data-[size=sm]:h-3.5 data-[size=sm]:w-6 data-checked:bg-switch-on data-checked:shadow-[0_0_0_1px_var(--color-border)] data-unchecked:bg-fill-2 data-disabled:cursor-not-allowed data-disabled:opacity-50",
        className,
      )}
      {...props}
    >
      <SwitchPrimitive.Thumb
        data-slot="switch-thumb"
        className={cn(
          "pointer-events-none block rounded-full bg-white ring-0 transition-transform duration-200",
          "group-data-[size=default]/switch:size-3.5 group-data-[size=sm]/switch:size-2.5",
          "group-data-[size=default]/switch:data-unchecked:translate-x-0.5 group-data-[size=sm]/switch:data-unchecked:translate-x-0.5",
          "group-data-[size=default]/switch:data-checked:translate-x-[13px] group-data-[size=sm]/switch:data-checked:translate-x-[11px]",
        )}
      />
    </SwitchPrimitive.Root>
  )
}

export { Switch }
