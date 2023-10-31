import { act, render } from '@testing-library/react';
import { noop } from 'lodash';
import React from 'react';
import { AutoSizerProps } from 'react-virtualized-auto-sizer';
import { byRole } from 'testing-library-selector';

import 'core-js/stable/structured-clone';

import { TestProvider } from '../../../../../../test/helpers/TestProvider';
import { DashboardSearchItemType } from '../../../../search/types';
import { mockDashboardApi, setupMswServer } from '../../mockApi';
import { mockDashboardDto, mockDashboardSearchItem } from '../../mocks';

import { DashboardPicker } from './DashboardPicker';

jest.mock('react-virtualized-auto-sizer', () => {
  return ({ children }: AutoSizerProps) => children({ height: 600, width: 1 });
});

const server = setupMswServer();

mockDashboardApi(server).search([
  mockDashboardSearchItem({ uid: 'dash-1', type: DashboardSearchItemType.DashDB, title: 'Dashboard 1' }),
  mockDashboardSearchItem({ uid: 'dash-2', type: DashboardSearchItemType.DashDB, title: 'Dashboard 2' }),
  mockDashboardSearchItem({ uid: 'dash-3', type: DashboardSearchItemType.DashDB, title: 'Dashboard 3' }),
]);

mockDashboardApi(server).dashboard(
  mockDashboardDto({
    uid: 'dash-2',
    title: 'Dashboard 2',
    panels: [{ type: 'graph' }, { type: 'timeseries' }],
  })
);

const ui = {
  dashboardButton: (name: RegExp) => byRole('button', { name }),
};

describe('DashboardPicker', () => {
  beforeEach(() => {
    jest.useFakeTimers();
  });

  afterEach(() => {
    jest.useRealTimers();
  });

  it('Renders panels without ids', async () => {
    render(<DashboardPicker isOpen={true} onChange={noop} onDismiss={noop} dashboardUid="dash-2" panelId={2} />, {
      wrapper: TestProvider,
    });
    act(() => {
      jest.advanceTimersByTime(500);
    });

    expect(await ui.dashboardButton(/Dashboard 1/).find()).toBeInTheDocument();
    expect(await ui.dashboardButton(/Dashboard 2/).find()).toBeInTheDocument();
    expect(await ui.dashboardButton(/Dashboard 3/).find()).toBeInTheDocument();

    expect(await ui.dashboardButton(/<No title>/).findAll()).toHaveLength(2);
  });
});
