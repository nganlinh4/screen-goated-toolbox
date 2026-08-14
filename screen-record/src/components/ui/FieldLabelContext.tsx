import { createContext, useContext } from "react";

export const FieldLabelContext = createContext<string | undefined>(undefined);

export function useFieldLabelId(explicitId?: string): string | undefined {
  const inheritedId = useContext(FieldLabelContext);
  return explicitId ?? inheritedId;
}
