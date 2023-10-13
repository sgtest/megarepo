import { render, screen } from '@testing-library/react';
import React from 'react';
import { AutoSizerProps } from 'react-virtualized-auto-sizer';
import { TestProvider } from 'test/helpers/TestProvider';

import { CoreApp, createTheme, DataSourceApi, EventBusSrv, LoadingState, PluginExtensionTypes } from '@grafana/data';
import { selectors } from '@grafana/e2e-selectors';
import { getPluginLinkExtensions } from '@grafana/runtime';
import { configureStore } from 'app/store/configureStore';

import { ContentOutlineContextProvider } from './ContentOutline/ContentOutlineContext';
import { Explore, Props } from './Explore';
import { initialExploreState } from './state/main';
import { scanStopAction } from './state/query';
import { createEmptyQueryResponse, makeExplorePaneState } from './state/utils';

const resizeWindow = (x: number, y: number) => {
  global.innerWidth = x;
  global.innerHeight = y;
  global.dispatchEvent(new Event('resize'));
};

const makeEmptyQueryResponse = (loadingState: LoadingState) => {
  const baseEmptyResponse = createEmptyQueryResponse();

  baseEmptyResponse.request = {
    requestId: '1',
    intervalMs: 0,
    interval: '1s',
    panelId: 1,
    range: baseEmptyResponse.timeRange,
    scopedVars: {
      apps: {
        value: 'value',
        text: 'text',
      },
    },
    targets: [
      {
        refId: 'A',
      },
    ],
    timezone: 'UTC',
    app: CoreApp.Explore,
    startTime: 0,
  };

  baseEmptyResponse.state = loadingState;

  return baseEmptyResponse;
};

const dummyProps: Props = {
  logsResult: undefined,
  changeSize: jest.fn(),
  datasourceInstance: {
    meta: {
      metrics: true,
      logs: true,
    },
    components: {
      QueryEditorHelp: {},
    },
  } as DataSourceApi,
  exploreId: 'left',
  loading: false,
  modifyQueries: jest.fn(),
  scanStart: jest.fn(),
  scanStopAction: scanStopAction,
  setQueries: jest.fn(),
  queryKeys: [],
  queries: [],
  isLive: false,
  syncedTimes: false,
  updateTimeRange: jest.fn(),
  graphResult: [],
  absoluteRange: {
    from: 0,
    to: 0,
  },
  timeZone: 'UTC',
  queryResponse: makeEmptyQueryResponse(LoadingState.NotStarted),
  addQueryRow: jest.fn(),
  theme: createTheme(),
  showMetrics: true,
  showLogs: true,
  showTable: true,
  showTrace: true,
  showCustom: true,
  showNodeGraph: true,
  showFlameGraph: true,
  splitOpen: jest.fn(),
  splitted: false,
  eventBus: new EventBusSrv(),
  showRawPrometheus: false,
  showLogsSample: false,
  logsSample: { enabled: false },
  setSupplementaryQueryEnabled: jest.fn(),
  correlationEditorDetails: undefined,
  correlationEditorHelperData: undefined,
};

jest.mock('@grafana/runtime/src/services/dataSourceSrv', () => {
  return {
    getDataSourceSrv: () => ({
      get: () => Promise.resolve({}),
      getList: () => [],
      getInstanceSettings: () => {},
    }),
  };
});

jest.mock('app/core/core', () => ({
  contextSrv: {
    hasPermission: () => true,
    getValidIntervals: (defaultIntervals: string[]) => defaultIntervals,
  },
}));

jest.mock('@grafana/runtime', () => ({
  ...jest.requireActual('@grafana/runtime'),
  getPluginLinkExtensions: jest.fn(() => ({ extensions: [] })),
}));

// for the AutoSizer component to have a width
jest.mock('react-virtualized-auto-sizer', () => {
  return ({ children }: AutoSizerProps) => children({ height: 1, width: 1 });
});

const getPluginLinkExtensionsMock = jest.mocked(getPluginLinkExtensions);

const setup = (overrideProps?: Partial<Props>) => {
  const store = configureStore({
    explore: {
      ...initialExploreState,
      panes: {
        left: makeExplorePaneState(),
      },
    },
  });
  const exploreProps = { ...dummyProps, ...overrideProps };

  return render(
    <TestProvider store={store}>
      <ContentOutlineContextProvider>
        <Explore {...exploreProps} />
      </ContentOutlineContextProvider>
    </TestProvider>
  );
};

describe('Explore', () => {
  it('should not render no data with not started loading state', async () => {
    setup();

    // Wait for the Explore component to render
    await screen.findByTestId(selectors.components.DataSourcePicker.container);

    expect(screen.queryByTestId('explore-no-data')).not.toBeInTheDocument();
  });

  it('should render no data with done loading state', async () => {
    setup({ queryResponse: makeEmptyQueryResponse(LoadingState.Done) });

    // Wait for the Explore component to render
    await screen.findByTestId(selectors.components.DataSourcePicker.container);

    expect(screen.getByTestId('explore-no-data')).toBeInTheDocument();
  });

  it('should render toolbar extension point if extensions is available', async () => {
    getPluginLinkExtensionsMock.mockReturnValueOnce({
      extensions: [
        {
          id: '1',
          pluginId: 'grafana',
          title: 'Test 1',
          description: '',
          type: PluginExtensionTypes.link,
          onClick: () => {},
        },
        {
          id: '2',
          pluginId: 'grafana',
          title: 'Test 2',
          description: '',
          type: PluginExtensionTypes.link,
          onClick: () => {},
        },
      ],
    });

    setup({ queryResponse: makeEmptyQueryResponse(LoadingState.Done) });
    // Wait for the Explore component to render
    await screen.findByTestId(selectors.components.DataSourcePicker.container);

    expect(screen.getByRole('button', { name: 'Add' })).toBeVisible();
  });

  describe('On small screens', () => {
    const windowWidth = global.innerWidth,
      windowHeight = global.innerHeight;

    beforeAll(() => {
      resizeWindow(500, 500);
    });

    afterAll(() => {
      resizeWindow(windowWidth, windowHeight);
    });

    it('should render data source picker', async () => {
      setup();

      const dataSourcePicker = await screen.findByTestId(selectors.components.DataSourcePicker.container);

      expect(dataSourcePicker).toBeInTheDocument();
    });
  });
});
