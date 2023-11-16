import { render, screen, waitFor } from '@testing-library/react';
import React, { ComponentProps } from 'react';

import { DataFrame, FieldType, LogsSortOrder, standardTransformersRegistry, toUtc } from '@grafana/data';
import { organizeFieldsTransformer } from '@grafana/data/src/transformations/transformers/organize';
import { config } from '@grafana/runtime';
import { extractFieldsTransformer } from 'app/features/transformers/extractFields/extractFields';

import { LogsTable } from './LogsTable';
import { getMockElasticFrame, getMockLokiFrame, getMockLokiFrameDataPlane } from './utils/testMocks.test';

jest.mock('@grafana/runtime', () => {
  const actual = jest.requireActual('@grafana/runtime');
  return {
    ...actual,
    getTemplateSrv: () => ({
      replace: (val: string) => (val ? val.replace('$input', '10').replace('$window', '10s') : val),
    }),
  };
});

const getComponent = (partialProps?: Partial<ComponentProps<typeof LogsTable>>, logs?: DataFrame) => {
  const testDataFrame = {
    fields: [
      {
        config: {},
        name: 'Time',
        type: FieldType.time,
        values: ['2019-01-01 10:00:00', '2019-01-01 11:00:00', '2019-01-01 12:00:00'],
      },
      {
        config: {},
        name: 'line',
        type: FieldType.string,
        values: ['log message 1', 'log message 2', 'log message 3'],
      },
      {
        config: {},
        name: 'tsNs',
        type: FieldType.string,
        values: ['ts1', 'ts2', 'ts3'],
      },
      {
        config: {},
        name: 'labels',
        type: FieldType.other,
        typeInfo: {
          frame: 'json.RawMessage',
        },
        values: [{ foo: 'bar' }, { foo: 'bar' }, { foo: 'bar' }],
      },
    ],
    length: 3,
  };
  return (
    <LogsTable
      height={400}
      columnsWithMeta={{}}
      logsSortOrder={LogsSortOrder.Descending}
      splitOpen={() => undefined}
      timeZone={'utc'}
      width={50}
      range={{
        from: toUtc('2019-01-01 10:00:00'),
        to: toUtc('2019-01-01 16:00:00'),
        raw: { from: 'now-1h', to: 'now' },
      }}
      dataFrame={logs ?? testDataFrame}
      {...partialProps}
    />
  );
};
const setup = (partialProps?: Partial<ComponentProps<typeof LogsTable>>, logs?: DataFrame) => {
  return render(
    getComponent(
      {
        ...partialProps,
      },
      logs
    )
  );
};

describe('LogsTable', () => {
  beforeAll(() => {
    const transformers = [extractFieldsTransformer, organizeFieldsTransformer];
    standardTransformersRegistry.setInit(() => {
      return transformers.map((t) => {
        return {
          id: t.id,
          aliasIds: t.aliasIds,
          name: t.name,
          transformation: t,
          description: t.description,
          editor: () => null,
        };
      });
    });
  });

  let originalVisualisationTypeValue = config.featureToggles.logsExploreTableVisualisation;

  beforeAll(() => {
    originalVisualisationTypeValue = config.featureToggles.logsExploreTableVisualisation;
    config.featureToggles.logsExploreTableVisualisation = true;
  });

  afterAll(() => {
    config.featureToggles.logsExploreTableVisualisation = originalVisualisationTypeValue;
  });

  it('should render 4 table rows', async () => {
    setup();

    await waitFor(() => {
      const rows = screen.getAllByRole('row');
      // tableFrame has 3 rows + 1 header row
      expect(rows.length).toBe(4);
    });
  });

  it('should render extracted labels as columns (elastic)', async () => {
    setup({
      dataFrame: getMockElasticFrame(),
      columnsWithMeta: {
        counter: { active: true, percentOfLinesWithLabel: 3 },
        level: { active: true, percentOfLinesWithLabel: 3 },
        line: { active: true, percentOfLinesWithLabel: 3 },
        '@timestamp': { active: true, percentOfLinesWithLabel: 3 },
      },
    });

    await waitFor(() => {
      const columns = screen.getAllByRole('columnheader');
      expect(columns[0].textContent).toContain('@timestamp');
      expect(columns[1].textContent).toContain('line');
      expect(columns[2].textContent).toContain('counter');
      expect(columns[3].textContent).toContain('level');
    });
  });

  it('should render extracted labels as columns (loki)', async () => {
    setup({
      columnsWithMeta: {
        foo: { active: true, percentOfLinesWithLabel: 3 },
        Time: { active: true, percentOfLinesWithLabel: 3 },
        line: { active: true, percentOfLinesWithLabel: 3 },
      },
    });

    await waitFor(() => {
      const columns = screen.getAllByRole('columnheader');

      expect(columns[0].textContent).toContain('Time');
      expect(columns[1].textContent).toContain('line');
      expect(columns[2].textContent).toContain('foo');
    });
  });

  it('should not render `tsNs`', async () => {
    setup(undefined, getMockLokiFrame());

    await waitFor(() => {
      const columns = screen.queryAllByRole('columnheader', { name: 'tsNs' });

      expect(columns.length).toBe(0);
    });
  });

  it('should not render `labels`', async () => {
    setup();

    await waitFor(() => {
      const columns = screen.queryAllByRole('columnheader', { name: 'labels' });

      expect(columns.length).toBe(0);
    });
  });

  describe('LogsTable (loki dataplane)', () => {
    let originalVisualisationTypeValue = config.featureToggles.logsExploreTableVisualisation;
    let originalLokiDataplaneValue = config.featureToggles.lokiLogsDataplane;

    beforeAll(() => {
      originalVisualisationTypeValue = config.featureToggles.logsExploreTableVisualisation;
      originalLokiDataplaneValue = config.featureToggles.lokiLogsDataplane;
      config.featureToggles.logsExploreTableVisualisation = true;
      config.featureToggles.lokiLogsDataplane = true;
    });

    afterAll(() => {
      config.featureToggles.logsExploreTableVisualisation = originalVisualisationTypeValue;
      config.featureToggles.lokiLogsDataplane = originalLokiDataplaneValue;
    });

    it('should render 4 table rows', async () => {
      setup(undefined, getMockLokiFrameDataPlane());

      await waitFor(() => {
        const rows = screen.getAllByRole('row');
        // tableFrame has 3 rows + 1 header row
        expect(rows.length).toBe(4);
      });
    });

    it('should render a datalink for each row', async () => {
      render(
        getComponent(
          {
            columnsWithMeta: {
              traceID: { active: true, percentOfLinesWithLabel: 3 },
            },
          },
          getMockLokiFrameDataPlane()
        )
      );

      await waitFor(() => {
        const links = screen.getAllByRole('link');

        expect(links.length).toBe(3);
      });
    });

    it('should not render `labels`', async () => {
      setup(undefined, getMockLokiFrameDataPlane());

      await waitFor(() => {
        const columns = screen.queryAllByRole('columnheader', { name: 'labels' });

        expect(columns.length).toBe(0);
      });
    });

    it('should not render `tsNs`', async () => {
      setup(undefined, getMockLokiFrameDataPlane());

      await waitFor(() => {
        const columns = screen.queryAllByRole('columnheader', { name: 'tsNs' });

        expect(columns.length).toBe(0);
      });
    });

    it('should render extracted labels as columns (loki dataplane)', async () => {
      setup({
        columnsWithMeta: {
          foo: { active: true, percentOfLinesWithLabel: 3 },
          line: { active: true, percentOfLinesWithLabel: 3 },
          Time: { active: true, percentOfLinesWithLabel: 3 },
        },
      });

      await waitFor(() => {
        const columns = screen.getAllByRole('columnheader');
        expect(columns[0].textContent).toContain('Time');
        expect(columns[1].textContent).toContain('line');
        expect(columns[2].textContent).toContain('foo');
      });
    });
  });
});
