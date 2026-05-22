import { fireEvent, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

import { renderWithProviders } from '../../../../test/test-utils';
import ActionBudgetPanel from '../ActionBudgetPanel';

const hoisted = vi.hoisted(() => ({
  mockGetConfig: vi.fn(),
  mockUpdateAutonomySettings: vi.fn(),
  mockIsTauri: vi.fn(() => true),
}));

vi.mock('../../../../utils/tauriCommands', () => ({
  isTauri: hoisted.mockIsTauri,
  openhumanGetConfig: hoisted.mockGetConfig,
  openhumanUpdateAutonomySettings: hoisted.mockUpdateAutonomySettings,
}));

vi.mock('../../hooks/useSettingsNavigation', () => ({
  useSettingsNavigation: () => ({ navigateBack: vi.fn(), breadcrumbs: [] }),
}));

const configResponse = (max_actions_per_hour: number) => ({
  result: {
    config: { autonomy: { max_actions_per_hour } },
    workspace_dir: '/tmp/openhuman/workspace',
    config_path: '/tmp/openhuman/config.toml',
  },
  logs: [],
});

describe('ActionBudgetPanel', () => {
  beforeEach(() => {
    hoisted.mockGetConfig.mockReset();
    hoisted.mockUpdateAutonomySettings.mockReset();
    hoisted.mockIsTauri.mockReturnValue(true);
  });

  it('loads and displays the active action budget', async () => {
    hoisted.mockGetConfig.mockResolvedValue(configResponse(64));

    renderWithProviders(<ActionBudgetPanel />);

    await waitFor(() => {
      expect(screen.getByDisplayValue('64')).toBeInTheDocument();
    });
    expect(screen.getByText('64 actions/hour')).toBeInTheDocument();
  });

  it('saves a changed action budget through config RPC', async () => {
    hoisted.mockGetConfig.mockResolvedValue(configResponse(20));
    hoisted.mockUpdateAutonomySettings.mockResolvedValue(configResponse(48));

    renderWithProviders(<ActionBudgetPanel />);

    const input = await screen.findByLabelText(/actions per hour/i);
    fireEvent.change(input, { target: { value: '48' } });
    fireEvent.click(screen.getByRole('button', { name: /save/i }));

    await waitFor(() => {
      expect(hoisted.mockUpdateAutonomySettings).toHaveBeenCalledWith({ max_actions_per_hour: 48 });
    });
    expect(await screen.findByText(/saved/i)).toBeInTheDocument();
    expect(screen.getByText('48 actions/hour')).toBeInTheDocument();
  });

  it('rejects values below one before calling RPC', async () => {
    hoisted.mockGetConfig.mockResolvedValue(configResponse(20));

    renderWithProviders(<ActionBudgetPanel />);

    const input = await screen.findByLabelText(/actions per hour/i);
    fireEvent.change(input, { target: { value: '0' } });
    fireEvent.click(screen.getByRole('button', { name: /save/i }));

    expect(await screen.findByText(/enter a whole number from 1 to 1000/i)).toBeInTheDocument();
    expect(hoisted.mockUpdateAutonomySettings).not.toHaveBeenCalled();
  });

  it('disables save outside the desktop runtime', async () => {
    hoisted.mockIsTauri.mockReturnValue(false);

    renderWithProviders(<ActionBudgetPanel />);

    expect(await screen.findByText(/available in the desktop app/i)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /save/i })).toBeDisabled();
    expect(hoisted.mockGetConfig).not.toHaveBeenCalled();
    expect(hoisted.mockUpdateAutonomySettings).not.toHaveBeenCalled();
  });
});
