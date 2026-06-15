import type { ReactNode } from 'react';
import { X } from '@/components/ui/MaterialIcon';
import { PanelSelect } from '@/components/ui/PanelSelect';

export interface LanguageConfigItem {
  languageCode: string;
  languageName: string;
}

export interface LanguageOption {
  languageCode: string;
  languageName: string;
}

export function RemoveLanguageButton({
  onClick,
  title,
}: {
  onClick: () => void;
  title: string;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="ui-icon-button h-6 w-6 rounded-full text-on-surface-variant hover:text-[var(--tertiary-color)]"
      title={title}
    >
      <X className="h-3 w-3" />
    </button>
  );
}

interface LanguageConfigListProps<T extends LanguageConfigItem> {
  items: T[];
  /** Header label rendered above the list. */
  title: ReactNode;
  /** Per-row control rendered between the language name and the remove button. */
  renderControl: (item: T, index: number) => ReactNode;
  onAdd: (languageCode: string, languageName: string) => void;
  onRemove: (index: number) => void;
  /** Placeholder shown on the "add language" select trigger. */
  addLabel: string;
  /** Remove button tooltip/title (shared across every row). */
  removeTitle: string;
  availableLanguages: LanguageOption[];
  /** Optional status content (e.g. loading/error spans) rendered before the rows. */
  statusContent?: ReactNode;
  /** Whether the per-row language name should truncate. Gemini keeps it un-truncated. */
  truncateLanguageName?: boolean;
  className?: string;
  rowClassName?: string;
  addTriggerClassName?: string;
  addContentClassName?: string;
}

export function LanguageConfigList<T extends LanguageConfigItem>({
  items,
  title,
  renderControl,
  onAdd,
  onRemove,
  addLabel,
  removeTitle,
  availableLanguages,
  statusContent,
  truncateLanguageName = true,
  className = 'mb-2',
  rowClassName,
  addTriggerClassName,
  addContentClassName,
}: LanguageConfigListProps<T>) {
  return (
    <div className={`language-config-list flex flex-col gap-1.5 ${className}`}>
      <span className="text-[11px] font-medium text-on-surface-variant">{title}</span>
      {statusContent}
      {items.map((item, index) => (
        <div
          key={`${item.languageCode}-${index}`}
          className={`language-config-row flex items-center gap-1.5 ${rowClassName ?? ''}`}
        >
          <span
            className={`language-config-name w-20 shrink-0 ${truncateLanguageName ? 'truncate ' : ''}text-[11px] font-medium text-[var(--secondary-color)]`}
          >
            {item.languageName}
          </span>
          {renderControl(item, index)}
          <RemoveLanguageButton onClick={() => onRemove(index)} title={removeTitle} />
        </div>
      ))}
      {availableLanguages.length > 0 && (
        <PanelSelect
          value={addLabel}
          options={availableLanguages.map((language) => ({
            value: language.languageCode,
            label: language.languageName,
          }))}
          onChange={(value) => {
            if (!value) return;
            const language = availableLanguages.find((item) => item.languageCode === value);
            if (language) onAdd(language.languageCode, language.languageName);
          }}
          triggerClassName={addTriggerClassName ?? 'language-config-add h-8 self-start rounded-lg px-2.5 text-[11px]'}
          contentClassName={addContentClassName ?? 'language-config-add-menu'}
          searchable
        />
      )}
    </div>
  );
}
