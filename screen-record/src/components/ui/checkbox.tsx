import * as React from "react";

import { useFieldLabelId } from "@/components/ui/FieldLabelContext";
import { cn } from "@/lib/utils";

export type CheckboxProps = Omit<React.InputHTMLAttributes<HTMLInputElement>, "type">;

const Checkbox = React.forwardRef<HTMLInputElement, CheckboxProps>(
  ({ className, "aria-labelledby": ariaLabelledBy, ...props }, ref) => {
    const labelId = useFieldLabelId(ariaLabelledBy);
    return (
      <input
        ref={ref}
        type="checkbox"
        aria-labelledby={props["aria-label"] ? undefined : labelId}
        className={cn("ui-checkbox-input", className)}
        {...props}
      />
    );
  },
);

Checkbox.displayName = "Checkbox";

export { Checkbox };
