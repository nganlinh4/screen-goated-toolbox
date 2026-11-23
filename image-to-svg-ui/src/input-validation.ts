import { t } from "./i18n";
import { inputValidationPrimaryAction, runInputValidation } from "./input-validation-flow.ts";
import { showInputValidationDialog } from "./input-validation-dialog.ts";
import "../../3d-generator-ui/src/input-dialog.css";
import "./input-validation.css";

export async function validateSvgInput(
  invoke: <T>(cmd: string, args?: unknown) => Promise<T>,
  path: string, name: string, choose: () => void,
  signal: AbortSignal, current: () => boolean,
): Promise<boolean> {
  return runInputValidation(
    () => invoke<{ error: string | null }>("validate_image", { path }),
    (error, activeSignal) => {
      const primary = inputValidationPrimaryAction(error);
      const retry = primary === "retry";
      return showInputValidationDialog({ name,
        title:t(retry ? "imageCheckTitle" : "imageChooseAnother"),
        message:t(retry ? "imageCheckUnavailable" : error === "image_too_large" ? "imageTooLarge" : "imageInvalid"),
        primary:t(retry ? "imageCheckRetry" : "imageChooseAnother"),close:t("close"),
      },primary,activeSignal);
    },
    choose,signal,current,
  );
}
