import * as React from 'react';
import * as Popover from '@radix-ui/react-popover';
import { Check, ChevronDown, Search } from '@/components/ui/MaterialIcon';
import { motion } from 'framer-motion';

import { cn } from '@/lib/utils';

export interface PanelSelectOption {
  value: string;
  label: string;
  triggerLabel?: string;
  disabled?: boolean;
  keywords?: string[];
  trailing?: React.ReactNode;
  action?: {
    label: string;
    onClick: () => void;
    icon?: React.ReactNode;
    disabled?: boolean;
    keepMenuOpen?: boolean;
    tone?: 'default' | 'danger';
  };
}

interface PanelSelectProps {
  value: string;
  options: PanelSelectOption[];
  onChange: (value: string) => void;
  disabled?: boolean;
  searchable?: boolean;
  searchPlaceholder?: string;
  emptyStateLabel?: string;
  triggerClassName?: string;
  contentClassName?: string;
}

export function PanelSelect({
  value,
  options,
  onChange,
  disabled = false,
  searchable = false,
  searchPlaceholder = 'Search...',
  emptyStateLabel = 'No results',
  triggerClassName,
  contentClassName,
}: PanelSelectProps) {
  const [open, setOpen] = React.useState(false);
  const [query, setQuery] = React.useState('');
  const searchInputRef = React.useRef<HTMLInputElement | null>(null);
  const normalizedValue = value.trim().toLocaleLowerCase();

  const selectedOption = React.useMemo(
    () =>
      options.find((option) => {
        if (option.value === value) {
          return true;
        }
        if (option.label.toLocaleLowerCase() === normalizedValue) {
          return true;
        }
        return (option.keywords ?? []).some(
          (keyword) => keyword.toLocaleLowerCase() === normalizedValue,
        );
      }) ?? null,
    [normalizedValue, options, value],
  );

  const filteredOptions = React.useMemo(() => {
    const normalizedQuery = query.trim().toLocaleLowerCase();
    if (!normalizedQuery) {
      return options;
    }

    return options.filter((option) => {
      const haystack = [
        option.label,
        option.value,
        ...(option.keywords ?? []),
      ]
        .join(' ')
        .toLocaleLowerCase();
      return haystack.includes(normalizedQuery);
    });
  }, [options, query]);

  React.useEffect(() => {
    if (!open) {
      setQuery('');
      return;
    }

    if (!searchable) {
      return;
    }

    const frame = window.requestAnimationFrame(() => {
      searchInputRef.current?.focus();
      searchInputRef.current?.select();
    });

    return () => window.cancelAnimationFrame(frame);
  }, [open, searchable]);

  return (
    <Popover.Root open={open} onOpenChange={setOpen} modal={false}>
      <Popover.Trigger asChild>
        <button
          type="button"
          disabled={disabled}
          className={cn(
            'panel-select-trigger ui-toolbar-button flex h-9 w-full items-center justify-between gap-2 rounded-xl px-3 text-left text-sm',
            triggerClassName,
          )}
        >
          <span className="panel-select-trigger-label min-w-0 flex-1 truncate">
            {selectedOption?.triggerLabel ?? selectedOption?.label ?? value}
          </span>
          <ChevronDown className="panel-select-trigger-icon h-4 w-4 flex-shrink-0 opacity-70" />
        </button>
      </Popover.Trigger>

      <Popover.Portal>
        <Popover.Content
          sideOffset={6}
          align="start"
          className="panel-select-popover z-[95]"
          onOpenAutoFocus={(event) => event.preventDefault()}
        >
          <motion.div
            className={cn(
              'panel-select-menu dropdown-menu ui-surface-elevated min-w-[var(--radix-popover-trigger-width)] overflow-hidden rounded-xl p-1.5',
              contentClassName,
            )}
            initial={{ opacity: 0, scale: 0.95, y: -4 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            transition={{ type: 'spring', stiffness: 500, damping: 30 }}
          >
            {searchable ? (
              <div className="panel-select-search-shell px-1 pb-1.5">
                <div className="panel-select-search-row flex items-center gap-2 rounded-lg border border-[var(--outline-variant)]/70 bg-[var(--ui-surface-2)] px-2.5 py-2 text-[11px] text-[var(--on-surface-variant)]">
                  <Search className="panel-select-search-icon h-3.5 w-3.5 flex-shrink-0 opacity-70" />
                  <input
                    ref={searchInputRef}
                    value={query}
                    onChange={(event) => setQuery(event.target.value)}
                    placeholder={searchPlaceholder}
                    className="panel-select-search-input min-w-0 flex-1 bg-transparent text-[var(--on-surface)] outline-none placeholder:text-[var(--on-surface-variant)]/70"
                  />
                </div>
              </div>
            ) : null}

            <div className="panel-select-options thin-scrollbar flex max-h-72 flex-col gap-0.5 overflow-y-auto pr-1">
              {filteredOptions.length > 0 ? (
                filteredOptions.map((option) => {
                  const isSelected = option.value === value;
                  const optionAction = option.action;
                  const rowClassName = option.disabled
                    ? 'cursor-not-allowed opacity-40'
                    : isSelected
                      ? 'bg-[color-mix(in_srgb,var(--primary-color)_14%,var(--ui-surface-3))] text-[var(--primary-color)]'
                      : 'cursor-pointer text-[var(--on-surface-variant)] hover:bg-[color-mix(in_srgb,var(--primary-color)_12%,var(--ui-surface-3))] hover:text-[var(--primary-color)]';
                  return (
                    <div
                      key={option.value}
                      className={cn(
                        'panel-select-option-row dropdown-menu-item relative flex w-full items-center rounded-md text-[11px] leading-tight outline-none transition-colors',
                        rowClassName,
                      )}
                    >
                      <button
                        type="button"
                        disabled={option.disabled}
                        className="panel-select-option flex min-w-0 flex-1 items-center px-2 py-1.5 text-left"
                        onClick={() => {
                          if (option.disabled) {
                            return;
                          }
                          onChange(option.value);
                          setOpen(false);
                        }}
                      >
                        <span className="panel-select-option-check mr-2 flex h-3.5 w-3.5 items-center justify-center">
                          {isSelected ? (
                            <Check className="h-3.5 w-3.5 text-[var(--primary-color)]" />
                          ) : null}
                        </span>
                        <span className="panel-select-option-label flex-1 text-left">
                          {option.label}
                        </span>
                      </button>
                      {option.trailing ? (
                        <div
                          className="panel-select-option-trailing mr-1 flex h-6 flex-shrink-0 items-center justify-center"
                          onClick={(event) => {
                            event.preventDefault();
                            event.stopPropagation();
                          }}
                          onPointerDown={(event) => {
                            event.stopPropagation();
                          }}
                        >
                          {option.trailing}
                        </div>
                      ) : null}
                      {optionAction ? (
                        <button
                          type="button"
                          disabled={optionAction.disabled}
                          className={cn(
                            'panel-select-option-action mr-1 flex h-6 w-6 flex-shrink-0 items-center justify-center rounded-md transition-colors',
                            optionAction.disabled
                              ? 'cursor-not-allowed opacity-40'
                              : optionAction.tone === 'danger'
                                ? 'text-[var(--on-surface-variant)] hover:bg-[color-mix(in_srgb,#ef4444_14%,transparent)] hover:text-[#ef4444]'
                                : 'text-[var(--on-surface-variant)] hover:bg-[color-mix(in_srgb,var(--primary-color)_12%,transparent)] hover:text-[var(--primary-color)]',
                          )}
                          title={optionAction.label}
                          aria-label={optionAction.label}
                          onClick={(event) => {
                            event.preventDefault();
                            event.stopPropagation();
                            if (optionAction.disabled) {
                              return;
                            }
                            optionAction.onClick();
                            if (!optionAction.keepMenuOpen) {
                              setOpen(false);
                            }
                          }}
                        >
                          {optionAction.icon}
                        </button>
                      ) : null}
                    </div>
                  );
                })
              ) : (
                <div className="panel-select-empty px-2 py-3 text-[11px] text-[var(--on-surface-variant)]">
                  {emptyStateLabel}
                </div>
              )}
            </div>
          </motion.div>
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}
