import type { ComponentType, SVGProps } from "react"

/** Props matching https://www.shadcn.io/icons CLI-installed Lucide components. */
export type IconProps = SVGProps<SVGSVGElement> & {
  size?: number | string
  color?: string
  strokeWidth?: number | string
}

export type Icon = ComponentType<IconProps>
