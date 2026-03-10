import * as React from 'react';
import * as DialogPrimitive from '@radix-ui/react-dialog';
import { motion } from 'framer-motion';
import { X } from 'lucide-react';
import { cn } from '@/lib/utils';

const Dialog = DialogPrimitive.Root;
const DialogTrigger = DialogPrimitive.Trigger;
const DialogClose = DialogPrimitive.Close;
const DialogPortal = DialogPrimitive.Portal;

const DialogOverlay = React.forwardRef<
  React.ComponentRef<typeof DialogPrimitive.Overlay>,
  React.ComponentPropsWithoutRef<typeof DialogPrimitive.Overlay>
>(({ className, ...props }, ref) => (
  <DialogPrimitive.Overlay ref={ref} asChild {...props}>
    <motion.div
      className={cn(
        'dialog-overlay fixed inset-0 z-[100] bg-[var(--ui-scrim)]',
        className,
      )}
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      transition={{ duration: 0.2 }}
    />
  </DialogPrimitive.Overlay>
));
DialogOverlay.displayName = 'DialogOverlay';

interface DialogContentProps extends React.ComponentPropsWithoutRef<typeof DialogPrimitive.Content> {
  /** Override default max-width (e.g. 'max-w-5xl') */
  size?: string;
  /** When true, hides the default close button */
  hideClose?: boolean;
}

const DialogContent = React.forwardRef<
  React.ComponentRef<typeof DialogPrimitive.Content>,
  DialogContentProps
>(({ className, children, size, hideClose, ...props }, ref) => (
  <DialogPortal>
    <DialogOverlay />
    <div className="dialog-shell fixed inset-0 z-[100] flex items-center justify-center p-4 sm:p-6">
      <DialogPrimitive.Content
        ref={ref}
        asChild
        aria-describedby={props["aria-describedby"] ?? undefined}
        {...props}
      >
        <motion.div
          className={cn(
            'dialog-content material-surface-elevated relative z-[101]',
            'flex max-h-[calc(100vh-2rem)] w-full flex-col overflow-hidden',
            size ?? 'max-w-md',
            'rounded-[1.85rem]',
            'focus:outline-none',
            className,
          )}
          initial={{ opacity: 0, scale: 0.965, y: 10 }}
          animate={{ opacity: 1, scale: 1, y: 0 }}
          transition={{ type: 'spring', stiffness: 380, damping: 30 }}
        >
          {children}
          {!hideClose && (
            <DialogPrimitive.Close className="dialog-close-btn ui-icon-button absolute right-3 top-3 p-1.5">
              <X className="w-4 h-4" />
            </DialogPrimitive.Close>
          )}
        </motion.div>
      </DialogPrimitive.Content>
    </div>
  </DialogPortal>
));
DialogContent.displayName = 'DialogContent';

function DialogHeader({ className, ...props }: React.HTMLAttributes<HTMLDivElement>) {
  return (
    <div className={cn('dialog-header flex items-center justify-between p-5 pb-0', className)} {...props} />
  );
}

function DialogTitle({ className, ...props }: React.HTMLAttributes<HTMLHeadingElement>) {
  return (
    <DialogPrimitive.Title asChild>
      <h3 className={cn('dialog-title text-sm font-semibold text-[var(--on-surface)]', className)} {...props} />
    </DialogPrimitive.Title>
  );
}

function DialogBody({ className, ...props }: React.HTMLAttributes<HTMLDivElement>) {
  return (
    <div className={cn('dialog-body p-5', className)} {...props} />
  );
}

function DialogFooter({ className, ...props }: React.HTMLAttributes<HTMLDivElement>) {
  return (
    <div className={cn('dialog-footer flex justify-end gap-2 p-5 pt-0', className)} {...props} />
  );
}

export {
  Dialog,
  DialogTrigger,
  DialogClose,
  DialogPortal,
  DialogOverlay,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogBody,
  DialogFooter,
};
