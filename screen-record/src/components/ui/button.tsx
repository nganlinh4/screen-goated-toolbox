import * as React from "react"
import { Slot } from "@radix-ui/react-slot"
import { cva, type VariantProps } from "class-variance-authority"

import { cn } from "@/lib/utils"

const buttonVariants = cva(
  "inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-md text-sm font-medium transition-all duration-150 ease-spring focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:pointer-events-none disabled:opacity-50 [&_svg]:pointer-events-none [&_svg]:size-4 [&_svg]:shrink-0 active:scale-[0.98]",
  {
    variants: {
      variant: {
        default:
          "border border-[color-mix(in_srgb,var(--primary-color)_50%,var(--ui-border))] bg-[color-mix(in_srgb,var(--primary-color)_16%,var(--ui-surface-3))] text-[var(--primary-color)] shadow-elevation-1 hover:bg-[color-mix(in_srgb,var(--primary-color)_20%,var(--ui-surface-3))] hover:border-[color-mix(in_srgb,var(--primary-color)_60%,var(--ui-border))] hover:shadow-elevation-2",
        destructive:
          "border border-[color-mix(in_srgb,var(--destructive)_52%,var(--ui-border))] bg-[color-mix(in_srgb,var(--destructive)_16%,var(--ui-surface-3))] text-[hsl(var(--destructive))] shadow-elevation-1 hover:bg-[color-mix(in_srgb,var(--destructive)_20%,var(--ui-surface-3))] hover:border-[color-mix(in_srgb,var(--destructive)_60%,var(--ui-border))] hover:shadow-elevation-2",
        outline:
          "ui-chip-button bg-transparent text-[var(--on-surface)]",
        secondary:
          "ui-chip-button bg-[var(--ui-surface-1)] text-[var(--on-surface)]",
        ghost:
          "text-[var(--on-surface-variant)] hover:bg-[color-mix(in_srgb,var(--primary-color)_12%,var(--ui-surface-2))] hover:text-[var(--primary-color)]",
        link: "text-primary underline-offset-4 hover:underline",
        subtle:
          "ui-chip-button bg-[var(--ui-surface-1)] text-[var(--on-surface)]",
        glow:
          "border border-[color-mix(in_srgb,var(--primary-color)_82%,black_10%)] bg-[color-mix(in_srgb,var(--primary-color)_90%,black_6%)] text-white shadow-elevation-2 hover:bg-[color-mix(in_srgb,var(--primary-color)_94%,black_4%)] hover:border-[color-mix(in_srgb,var(--primary-color)_88%,black_8%)] hover:shadow-glow hover:scale-[1.02]",
      },
      size: {
        default: "h-9 px-4 py-2",
        sm: "h-8 rounded-md px-3 text-xs",
        lg: "h-10 rounded-md px-8",
        icon: "h-9 w-9",
      },
    },
    defaultVariants: {
      variant: "default",
      size: "default",
    },
  }
)

export interface ButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof buttonVariants> {
  asChild?: boolean
}

const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className, variant, size, asChild = false, ...props }, ref) => {
    const Comp = asChild ? Slot : "button"
    return (
      <Comp
        className={cn(buttonVariants({ variant, size, className }))}
        ref={ref}
        {...props}
      />
    )
  }
)
Button.displayName = "Button"

export { Button, buttonVariants }
