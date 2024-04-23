import { screen, waitFor, waitForElementToBeRemoved } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import React from 'react';
import { renderRuleEditor, ui } from 'test/helpers/alertingRuleEditor';
import { clickSelectOption } from 'test/helpers/selectOptionInTest';
import { byText } from 'testing-library-selector';
import 'whatwg-fetch';

import { setDataSourceSrv } from '@grafana/runtime';
import { contextSrv } from 'app/core/services/context_srv';
import { mockApi, setupMswServer } from 'app/features/alerting/unified/mockApi';
import { AccessControlAction } from 'app/types';
import { PromApplication } from 'app/types/unified-alerting-dto';

import { searchFolders } from '../../manage-dashboards/state/actions';

import { discoverFeatures } from './api/buildInfo';
import { fetchRulerRules, fetchRulerRulesGroup, fetchRulerRulesNamespace, setRulerRuleGroup } from './api/ruler';
import { RecordingRuleEditorProps } from './components/rule-editor/RecordingRuleEditor';
import { MockDataSourceSrv, grantUserPermissions, labelsPluginMetaMock, mockDataSource } from './mocks';
import { fetchRulerRulesIfNotFetchedYet } from './state/actions';
import * as config from './utils/config';

jest.mock('./components/rule-editor/RecordingRuleEditor', () => ({
  RecordingRuleEditor: ({ queries, onChangeQuery }: Pick<RecordingRuleEditorProps, 'queries' | 'onChangeQuery'>) => {
    const onChange = (expr: string) => {
      const query = queries[0];

      const merged = {
        ...query,
        expr,
        model: {
          ...query.model,
          expr,
        },
      };

      onChangeQuery([merged]);
    };

    return <input data-testid="expr" onChange={(e) => onChange(e.target.value)} />;
  },
}));

jest.mock('app/core/components/AppChrome/AppChromeUpdate', () => ({
  AppChromeUpdate: ({ actions }: { actions: React.ReactNode }) => <div>{actions}</div>,
}));

jest.mock('./api/buildInfo');
jest.mock('./api/ruler');
jest.mock('../../../../app/features/manage-dashboards/state/actions');
// there's no angular scope in test and things go terribly wrong when trying to render the query editor row.
// lets just skip it
jest.mock('app/features/query/components/QueryEditorRow', () => ({
  // eslint-disable-next-line react/display-name
  QueryEditorRow: () => <p>hi</p>,
}));

jest.spyOn(config, 'getAllDataSources');

const dataSources = {
  default: mockDataSource(
    {
      type: 'prometheus',
      name: 'Prom',
      isDefault: true,
    },
    { alerting: true }
  ),
};

jest.mock('@grafana/runtime', () => ({
  ...jest.requireActual('@grafana/runtime'),
  getDataSourceSrv: jest.fn(() => ({
    getInstanceSettings: () => dataSources.default,
    get: () => dataSources.default,
    getList: () => Object.values(dataSources),
  })),
}));

jest.setTimeout(60 * 1000);

const mocks = {
  getAllDataSources: jest.mocked(config.getAllDataSources),
  searchFolders: jest.mocked(searchFolders),
  api: {
    discoverFeatures: jest.mocked(discoverFeatures),
    fetchRulerRulesGroup: jest.mocked(fetchRulerRulesGroup),
    setRulerRuleGroup: jest.mocked(setRulerRuleGroup),
    fetchRulerRulesNamespace: jest.mocked(fetchRulerRulesNamespace),
    fetchRulerRules: jest.mocked(fetchRulerRules),
    fetchRulerRulesIfNotFetchedYet: jest.mocked(fetchRulerRulesIfNotFetchedYet),
  },
};

const server = setupMswServer();
mockApi(server).plugins.getPluginSettings({ ...labelsPluginMetaMock, enabled: false });
mockApi(server).eval({ results: { A: { frames: [] } } });

describe('RuleEditor recording rules', () => {
  beforeEach(() => {
    mockApi(server).eval({ results: {} });
    jest.clearAllMocks();
    contextSrv.isEditor = true;
    contextSrv.hasEditPermissionInFolders = true;
    grantUserPermissions([
      AccessControlAction.AlertingRuleRead,
      AccessControlAction.AlertingRuleUpdate,
      AccessControlAction.AlertingRuleDelete,
      AccessControlAction.AlertingRuleCreate,
      AccessControlAction.DataSourcesRead,
      AccessControlAction.DataSourcesWrite,
      AccessControlAction.DataSourcesCreate,
      AccessControlAction.FoldersWrite,
      AccessControlAction.FoldersRead,
      AccessControlAction.AlertingRuleExternalRead,
      AccessControlAction.AlertingRuleExternalWrite,
    ]);
  });

  it('can create a new cloud recording rule', async () => {
    setDataSourceSrv(new MockDataSourceSrv(dataSources));
    mocks.getAllDataSources.mockReturnValue(Object.values(dataSources));
    mocks.api.setRulerRuleGroup.mockResolvedValue();
    mocks.api.fetchRulerRulesNamespace.mockResolvedValue([]);
    mocks.api.fetchRulerRulesGroup.mockResolvedValue({
      name: 'group2',
      rules: [],
    });
    mocks.api.fetchRulerRules.mockResolvedValue({
      namespace1: [
        {
          name: 'group1',
          rules: [],
        },
      ],
      namespace2: [
        {
          name: 'group2',
          rules: [],
        },
      ],
    });
    mocks.searchFolders.mockResolvedValue([]);

    mocks.api.discoverFeatures.mockResolvedValue({
      application: PromApplication.Cortex,
      features: {
        rulerApiEnabled: true,
      },
    });

    renderRuleEditor(undefined, true);
    await waitForElementToBeRemoved(screen.getAllByTestId('Spinner'));
    await userEvent.type(await ui.inputs.name.find(), 'my great new recording rule');

    const dataSourceSelect = ui.inputs.dataSource.get();
    await userEvent.click(dataSourceSelect);

    await userEvent.click(screen.getByText('Prom'));
    await clickSelectOption(ui.inputs.namespace.get(), 'namespace2');
    await clickSelectOption(ui.inputs.group.get(), 'group2');

    await userEvent.type(await ui.inputs.expr.find(), 'up == 1');

    // try to save, find out that recording rule name is invalid
    await userEvent.click(ui.buttons.saveAndExit.get());
    await waitFor(() =>
      expect(
        byText(
          'Recording rule name must be valid metric name. It may only contain letters, numbers, and colons. It may not contain whitespace.'
        ).get()
      ).toBeInTheDocument()
    );
    expect(mocks.api.setRulerRuleGroup).not.toBeCalled();

    // fix name and re-submit
    await userEvent.clear(await ui.inputs.name.find());
    await userEvent.type(await ui.inputs.name.find(), 'my:great:new:recording:rule');

    // save and check what was sent to backend
    await userEvent.click(ui.buttons.saveAndExit.get());
    await waitFor(() => expect(mocks.api.setRulerRuleGroup).toHaveBeenCalled());
    expect(mocks.api.setRulerRuleGroup).toHaveBeenCalledWith(
      { dataSourceName: 'Prom', apiVersion: 'legacy' },
      'namespace2',
      {
        name: 'group2',
        rules: [
          {
            record: 'my:great:new:recording:rule',
            labels: {},
            expr: 'up == 1',
          },
        ],
      }
    );
  });
});
