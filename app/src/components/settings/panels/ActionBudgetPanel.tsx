import { useCallback, useEffect, useState } from 'react';

import { useT } from '../../../lib/i18n/I18nContext';
import {
  type ConfigSnapshot,
  isTauri,
  openhumanGetConfig,
  openhumanUpdateAutonomySettings,
} from '../../../utils/tauriCommands';
import SettingsHeader from '../components/SettingsHeader';
import { useSettingsNavigation } from '../hooks/useSettingsNavigation';

const DEFAULT_ACTION_BUDGET = 20;
const MIN_ACTION_BUDGET = 1;
const MAX_ACTION_BUDGET = 1000;

function actionBudgetFromSnapshot(snapshot: ConfigSnapshot): number {
  const autonomy = snapshot.config.autonomy;
  if (autonomy && typeof autonomy === 'object' && 'max_actions_per_hour' in autonomy) {
    const value = (autonomy as { max_actions_per_hour?: unknown }).max_actions_per_hour;
    if (typeof value === 'number' && Number.isFinite(value)) {
      return Math.trunc(value);
    }
  }
  return DEFAULT_ACTION_BUDGET;
}

function parseBudgetInput(value: string): number | null {
  const parsed = Number(value);
  if (!Number.isInteger(parsed) || parsed < MIN_ACTION_BUDGET || parsed > MAX_ACTION_BUDGET) {
    return null;
  }
  return parsed;
}

const ActionBudgetPanel = () => {
  const { t } = useT();
  const tauriRuntime = isTauri();
  const { navigateBack, breadcrumbs } = useSettingsNavigation();
  const [value, setValue] = useState(String(DEFAULT_ACTION_BUDGET));
  const [activeLimit, setActiveLimit] = useState(DEFAULT_ACTION_BUDGET);
  const [isLoading, setIsLoading] = useState(true);
  const [isSaving, setIsSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);

  const loadConfig = useCallback(async () => {
    if (!tauriRuntime) {
      setIsLoading(false);
      setError(t('settings.actionBudget.desktopOnly'));
      return;
    }

    setIsLoading(true);
    setError(null);
    try {
      const response = await openhumanGetConfig();
      const nextLimit = actionBudgetFromSnapshot(response.result);
      setActiveLimit(nextLimit);
      setValue(String(nextLimit));
    } catch (err) {
      setError(err instanceof Error ? err.message : t('settings.actionBudget.loadFailed'));
    } finally {
      setIsLoading(false);
    }
  }, [t, tauriRuntime]);

  useEffect(() => {
    void loadConfig();
  }, [loadConfig]);

  const save = async () => {
    if (!tauriRuntime) {
      setError(t('settings.actionBudget.desktopOnly'));
      return;
    }

    const parsed = parseBudgetInput(value);
    setStatus(null);
    if (parsed === null) {
      setError(t('settings.actionBudget.validation'));
      return;
    }

    setIsSaving(true);
    setError(null);
    try {
      const response = await openhumanUpdateAutonomySettings({ max_actions_per_hour: parsed });
      const savedLimit = actionBudgetFromSnapshot(response.result);
      setActiveLimit(savedLimit);
      setValue(String(savedLimit));
      setStatus(t('settings.actionBudget.saved'));
    } catch (err) {
      setError(err instanceof Error ? err.message : t('settings.actionBudget.saveFailed'));
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <div className="z-10 relative">
      <SettingsHeader
        title={t('settings.actionBudget.title')}
        showBackButton={true}
        onBack={navigateBack}
        breadcrumbs={breadcrumbs}
      />

      <div className="p-4 space-y-4">
        <section className="rounded-xl border border-stone-200 dark:border-neutral-800 bg-stone-50 dark:bg-neutral-800/60 p-4 space-y-3">
          <div className="flex items-center justify-between gap-3">
            <div>
              <h3 className="text-sm font-semibold text-stone-900 dark:text-neutral-100">
                {t('settings.actionBudget.activeLimit')}
              </h3>
              <div className="mt-1 text-xs text-stone-500 dark:text-neutral-400">
                {t('settings.actionBudget.sideEffectingActions')}
              </div>
            </div>
            <div className="rounded-lg border border-primary-200 dark:border-primary-500/30 bg-white dark:bg-neutral-900 px-3 py-2 text-sm font-semibold text-primary-700 dark:text-primary-300">
              {activeLimit} {t('settings.actionBudget.actionsPerHour')}
            </div>
          </div>

          <label className="block">
            <span className="text-xs font-medium text-stone-700 dark:text-neutral-300">
              {t('settings.actionBudget.actionsPerHourLabel')}
            </span>
            <input
              type="number"
              min={MIN_ACTION_BUDGET}
              max={MAX_ACTION_BUDGET}
              step={1}
              value={value}
              onChange={event => {
                setValue(event.target.value);
                setError(null);
                setStatus(null);
              }}
              aria-label={t('settings.actionBudget.actionsPerHourLabel')}
              className="mt-2 w-full rounded-lg border border-stone-200 dark:border-neutral-700 bg-white dark:bg-neutral-900 px-3 py-2 text-sm text-stone-900 dark:text-neutral-100 focus:border-primary-400 focus:outline-none focus:ring-2 focus:ring-primary-100 dark:focus:ring-primary-500/20"
            />
          </label>

          <div className="flex items-center justify-between gap-3">
            <div role="status" aria-live="polite" className="min-h-5 text-xs">
              {isLoading && (
                <span className="text-stone-500 dark:text-neutral-400">{t('common.loading')}</span>
              )}
              {status && <span className="text-sage-700 dark:text-sage-300">{status}</span>}
              {error && <span className="text-coral-600 dark:text-coral-300">{error}</span>}
            </div>
            <button
              type="button"
              onClick={() => void save()}
              disabled={isLoading || isSaving || !tauriRuntime}
              className="rounded-lg bg-primary-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-primary-500 disabled:opacity-60">
              {isSaving ? t('settings.actionBudget.saving') : t('common.save')}
            </button>
          </div>
        </section>
      </div>
    </div>
  );
};

export default ActionBudgetPanel;
