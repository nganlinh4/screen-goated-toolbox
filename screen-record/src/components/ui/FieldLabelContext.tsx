import { createContext, useContext, useId, type ReactNode } from "react";

export const FieldLabelContext = createContext<string | undefined>(undefined);

export function useFieldLabelId(explicitId?: string): string | undefined {
  const inheritedId = useContext(FieldLabelContext);
  return explicitId ?? inheritedId;
}

/** Visible text that names the field controls rendered after it (via aria-labelledby). */
export function LabelledField({
  label,
  labelClassName,
  children,
}: {
  label: ReactNode;
  labelClassName?: string;
  children: ReactNode;
}) {
  const labelId = useId();
  const inheritedId = useContext(FieldLabelContext);
  return (
    <>
      <span id={labelId} className={labelClassName}>{label}</span>
      <FieldLabelContext.Provider value={inheritedId ? `${inheritedId} ${labelId}` : labelId}>
        {children}
      </FieldLabelContext.Provider>
    </>
  );
}
