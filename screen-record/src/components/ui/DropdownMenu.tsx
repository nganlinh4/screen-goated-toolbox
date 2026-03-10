import * as React from 'react';
import * as DropdownMenuPrimitive from '@radix-ui/react-dropdown-menu';
import { motion } from 'framer-motion';
import { cn } from '@/lib/utils';
import { Check } from 'lucide-react';

function DropdownMenu(props: React.ComponentPropsWithoutRef<typeof DropdownMenuPrimitive.Root>) {
  return <DropdownMenuPrimitive.Root modal={false} {...props} />;
}
const DropdownMenuTrigger = DropdownMenuPrimitive.Trigger;
const DropdownMenuGroup = DropdownMenuPrimitive.Group;

const DropdownMenuContent = React.forwardRef<
  React.ComponentRef<typeof DropdownMenuPrimitive.Content>,
  React.ComponentPropsWithoutRef<typeof DropdownMenuPrimitive.Content>
>(({ className, sideOffset = 4, ...props }, ref) => (
  <DropdownMenuPrimitive.Portal>
    <DropdownMenuPrimitive.Content ref={ref} sideOffset={sideOffset} asChild {...props}>
      <motion.div
        className={cn(
          'dropdown-menu ui-surface-elevated relative z-[95] min-w-[180px] overflow-hidden rounded-xl p-1.5',
          className
        )}
        initial={{ opacity: 0, scale: 0.95, y: -4 }}
        animate={{ opacity: 1, scale: 1, y: 0 }}
        transition={{ type: 'spring', stiffness: 500, damping: 30 }}
      >
        {props.children}
      </motion.div>
    </DropdownMenuPrimitive.Content>
  </DropdownMenuPrimitive.Portal>
));
DropdownMenuContent.displayName = 'DropdownMenuContent';

const DropdownMenuItem = React.forwardRef<
  React.ComponentRef<typeof DropdownMenuPrimitive.Item>,
  React.ComponentPropsWithoutRef<typeof DropdownMenuPrimitive.Item> & {
    selected?: boolean;
  }
>(({ className, selected, children, ...props }, ref) => (
  <DropdownMenuPrimitive.Item
    ref={ref}
    className={cn(
      'dropdown-menu-item relative flex w-full cursor-pointer items-center rounded-md px-2 py-1.5 text-[11px] leading-tight outline-none transition-colors',
      selected
        ? 'bg-[color-mix(in_srgb,var(--primary-color)_14%,var(--ui-surface-3))] text-[var(--primary-color)]'
        : 'text-[var(--on-surface-variant)] hover:bg-[color-mix(in_srgb,var(--primary-color)_12%,var(--ui-surface-3))] hover:text-[var(--primary-color)]',
      'focus:bg-[color-mix(in_srgb,var(--primary-color)_12%,var(--ui-surface-3))] focus:text-[var(--primary-color)]',
      className
    )}
    {...props}
  >
    {selected !== undefined && (
      <span className="dropdown-menu-check mr-2 flex h-3.5 w-3.5 items-center justify-center">
        {selected && <Check className="h-3.5 w-3.5 text-[var(--primary-color)]" />}
      </span>
    )}
    {children}
  </DropdownMenuPrimitive.Item>
));
DropdownMenuItem.displayName = 'DropdownMenuItem';

const DropdownMenuSeparator = React.forwardRef<
  React.ComponentRef<typeof DropdownMenuPrimitive.Separator>,
  React.ComponentPropsWithoutRef<typeof DropdownMenuPrimitive.Separator>
>(({ className, ...props }, ref) => (
  <DropdownMenuPrimitive.Separator
    ref={ref}
    className={cn('dropdown-menu-separator -mx-1 my-1 h-px bg-[var(--outline-variant)]/50', className)}
    {...props}
  />
));
DropdownMenuSeparator.displayName = 'DropdownMenuSeparator';

const DropdownMenuLabel = React.forwardRef<
  React.ComponentRef<typeof DropdownMenuPrimitive.Label>,
  React.ComponentPropsWithoutRef<typeof DropdownMenuPrimitive.Label>
>(({ className, ...props }, ref) => (
  <DropdownMenuPrimitive.Label
    ref={ref}
    className={cn('dropdown-menu-label px-2 py-1.5 text-[10px] uppercase tracking-wide text-[var(--on-surface-variant)] opacity-60', className)}
    {...props}
  />
));
DropdownMenuLabel.displayName = 'DropdownMenuLabel';

export {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuGroup,
  DropdownMenuSeparator,
  DropdownMenuLabel,
};
