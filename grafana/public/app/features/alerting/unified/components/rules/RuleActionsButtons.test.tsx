import { produce } from 'immer';
import React from 'react';
import { render, screen, userEvent } from 'test/test-utils';
import { byLabelText } from 'testing-library-selector';

import { setPluginExtensionsHook } from '@grafana/runtime';
import { contextSrv } from 'app/core/services/context_srv';
import { RuleActionsButtons } from 'app/features/alerting/unified/components/rules/RuleActionsButtons';
import { setupMswServer } from 'app/features/alerting/unified/mockApi';
import {
  getCloudRule,
  getGrafanaRule,
  grantUserPermissions,
  mockDataSource,
  mockGrafanaRulerRule,
  mockPromAlertingRule,
} from 'app/features/alerting/unified/mocks';
import { configureStore } from 'app/store/configureStore';
import { AccessControlAction } from 'app/types';
import { PromAlertingRuleState } from 'app/types/unified-alerting-dto';

setupMswServer();
jest.mock('app/core/services/context_srv');
const mockContextSrv = jest.mocked(contextSrv);

const ui = {
  moreButton: byLabelText('more-actions'),
};

const grantAllPermissions = () => {
  grantUserPermissions([
    AccessControlAction.AlertingRuleCreate,
    AccessControlAction.AlertingRuleRead,
    AccessControlAction.AlertingRuleUpdate,
    AccessControlAction.AlertingRuleDelete,
    AccessControlAction.AlertingInstanceCreate,
  ]);
  mockContextSrv.hasPermissionInMetadata.mockImplementation(() => true);
  mockContextSrv.hasPermission.mockImplementation(() => true);
};
const grantNoPermissions = () => {
  grantUserPermissions([]);
  mockContextSrv.hasPermissionInMetadata.mockImplementation(() => false);
  mockContextSrv.hasPermission.mockImplementation(() => false);
};

const getMenuContents = async () => {
  await screen.findByRole('menu');
  const allMenuItems = screen.queryAllByRole('menuitem').map((el) => el.textContent);
  const allLinkItems = screen.queryAllByRole('link').map((el) => el.textContent);

  return [...allMenuItems, ...allLinkItems];
};

setPluginExtensionsHook(() => ({
  extensions: [],
  isLoading: false,
}));

describe('RuleActionsButtons', () => {
  it('renders correct options for grafana managed rule', async () => {
    const user = userEvent.setup();
    grantAllPermissions();
    const mockRule = getGrafanaRule();

    render(<RuleActionsButtons rule={mockRule} rulesSource="grafana" showCopyLinkButton />);

    await user.click(await ui.moreButton.find());

    expect(await getMenuContents()).toMatchSnapshot();
  });

  it('renders correct options for Cloud rule', async () => {
    const user = userEvent.setup();
    grantAllPermissions();
    const mockRule = getCloudRule();
    const dataSource = mockDataSource({ id: 1 });

    const defaultState = configureStore().getState();
    render(<RuleActionsButtons rule={mockRule} rulesSource={dataSource} />, {
      preloadedState: produce(defaultState, (store) => {
        store.unifiedAlerting.dataSources[dataSource.name] = {
          loading: false,
          dispatched: true,
          result: {
            id: 'test-ds',
            name: dataSource.name,
            rulerConfig: {
              dataSourceName: dataSource.name,
              apiVersion: 'config',
            },
          },
        };
      }),
    });

    await user.click(await ui.moreButton.find());

    expect(await getMenuContents()).toMatchSnapshot();
  });

  it('renders minimal "More" menu when appropriate', async () => {
    const user = userEvent.setup();
    grantNoPermissions();

    const mockRule = getGrafanaRule({ promRule: mockPromAlertingRule({ state: PromAlertingRuleState.Inactive }) });

    render(<RuleActionsButtons rule={mockRule} rulesSource="grafana" />);

    await user.click(await ui.moreButton.find());

    expect(await getMenuContents()).toMatchSnapshot();
  });

  it('does not allow deletion when rule is provisioned', async () => {
    const user = userEvent.setup();
    grantAllPermissions();
    const mockRule = getGrafanaRule({ rulerRule: mockGrafanaRulerRule({ provenance: 'file' }) });

    render(<RuleActionsButtons rule={mockRule} rulesSource="grafana" />);

    await user.click(await ui.moreButton.find());

    expect(screen.queryByText(/delete/i)).not.toBeInTheDocument();
  });
});
