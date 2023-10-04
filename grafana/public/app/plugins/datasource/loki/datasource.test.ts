import { of } from 'rxjs';
import { take } from 'rxjs/operators';
import { getQueryOptions } from 'test/helpers/getQueryOptions';

import {
  AbstractLabelOperator,
  AnnotationQueryRequest,
  CoreApp,
  DataFrame,
  dataFrameToJSON,
  DataQueryResponse,
  DataSourceInstanceSettings,
  dateTime,
  FieldType,
  SupplementaryQueryType,
  ToggleFilterAction,
} from '@grafana/data';
import {
  BackendSrv,
  BackendSrvRequest,
  config,
  FetchResponse,
  getBackendSrv,
  reportInteraction,
  setBackendSrv,
} from '@grafana/runtime';
import { TemplateSrv } from 'app/features/templating/template_srv';

import { initialCustomVariableModelState } from '../../../features/variables/custom/reducer';
import { CustomVariableModel } from '../../../features/variables/types';

import { LokiDatasource, REF_ID_DATA_SAMPLES } from './datasource';
import { createLokiDatasource, createMetadataRequest } from './mocks';
import { runSplitQuery } from './querySplitting';
import { parseToNodeNamesArray } from './queryUtils';
import { LokiOptions, LokiQuery, LokiQueryType, LokiVariableQueryType, SupportingQueryType } from './types';
import { LokiVariableSupport } from './variables';

jest.mock('@grafana/runtime', () => {
  return {
    ...jest.requireActual('@grafana/runtime'),
    reportInteraction: jest.fn(),
  };
});

jest.mock('./querySplitting');

const templateSrvStub = {
  getAdhocFilters: jest.fn(() => [] as unknown[]),
  replace: jest.fn((a: string, ...rest: unknown[]) => a),
} as unknown as TemplateSrv;

const testFrame: DataFrame = {
  refId: 'A',
  fields: [
    {
      name: 'Time',
      type: FieldType.time,
      config: {},
      values: [1, 2],
    },
    {
      name: 'Line',
      type: FieldType.string,
      config: {},
      values: ['line1', 'line2'],
    },
    {
      name: 'labels',
      type: FieldType.other,
      config: {},
      values: [
        {
          label: 'value',
          label2: 'value ',
        },
        {
          label: '',
          label2: 'value2',
          label3: ' ',
        },
      ],
    },
    {
      name: 'tsNs',
      type: FieldType.string,
      config: {},
      values: ['1000000', '2000000'],
    },
    {
      name: 'id',
      type: FieldType.string,
      config: {},
      values: ['id1', 'id2'],
    },
  ],
  length: 2,
};

const testLogsResponse: FetchResponse = {
  data: {
    results: {
      A: {
        frames: [dataFrameToJSON(testFrame)],
      },
    },
  },
  ok: true,
  headers: {} as unknown as Headers,
  redirected: false,
  status: 200,
  statusText: 'Success',
  type: 'default',
  url: '',
  config: {} as unknown as BackendSrvRequest,
};

interface AdHocFilter {
  condition: string;
  key: string;
  operator: string;
  value: string;
}

describe('LokiDatasource', () => {
  let origBackendSrv: BackendSrv;

  beforeEach(() => {
    origBackendSrv = getBackendSrv();
  });

  afterEach(() => {
    setBackendSrv(origBackendSrv);
    (reportInteraction as jest.Mock).mockClear();
  });

  describe('when doing logs queries with limits', () => {
    const runTest = async (
      queryMaxLines: number | undefined,
      dsMaxLines: string | undefined,
      expectedMaxLines: number,
      app: CoreApp | undefined
    ) => {
      const settings = {
        jsonData: {
          maxLines: dsMaxLines,
        },
      } as DataSourceInstanceSettings<LokiOptions>;

      const ds = createLokiDatasource(templateSrvStub, settings);

      // we need to check the final query before it is sent out,
      // and applyTemplateVariables is a convenient place to do that.
      const spy = jest.spyOn(ds, 'applyTemplateVariables');

      const options = getQueryOptions<LokiQuery>({
        targets: [{ expr: '{a="b"}', refId: 'B', maxLines: queryMaxLines }],
        app: app ?? CoreApp.Dashboard,
      });

      const fetchMock = jest.fn().mockReturnValue(of({ data: testLogsResponse }));
      setBackendSrv({ ...origBackendSrv, fetch: fetchMock });

      await expect(ds.query(options).pipe(take(1))).toEmitValuesWith(() => {
        expect(fetchMock.mock.calls.length).toBe(1);
        expect(spy.mock.calls[0][0].maxLines).toBe(expectedMaxLines);
      });
    };

    it('should use datasource max lines when no query max lines', async () => {
      await runTest(undefined, '40', 40, undefined);
    });

    it('should use query max lines, if exists', async () => {
      await runTest(80, undefined, 80, undefined);
    });

    it('should use query max lines, if both exist, even if it is higher than ds max lines', async () => {
      await runTest(80, '40', 80, undefined);
    });

    it('should use query max lines, if both exist, even if it is 0', async () => {
      await runTest(0, '40', 0, undefined);
    });

    it('should report query interaction', async () => {
      await runTest(80, '40', 80, CoreApp.Explore);
      expect(reportInteraction).toHaveBeenCalledWith(
        'grafana_loki_query_executed',
        expect.objectContaining({
          query_type: 'logs',
          line_limit: 80,
          parsed_query: parseToNodeNamesArray('{a="b"}').join(','),
        })
      );
    });

    it('should not report query interaction for dashboard query', async () => {
      await runTest(80, '40', 80, CoreApp.Dashboard);
      expect(reportInteraction).not.toBeCalled();
    });

    it('should not report query interaction for panel edit query', async () => {
      await runTest(80, '40', 80, CoreApp.PanelEditor);
      expect(reportInteraction).toHaveBeenCalledWith(
        'grafana_loki_query_executed',
        expect.objectContaining({
          query_type: 'logs',
          line_limit: 80,
          parsed_query: parseToNodeNamesArray('{a="b"}').join(','),
        })
      );
    });
  });

  describe('When using adhoc filters', () => {
    const DEFAULT_EXPR = 'rate({bar="baz", job="foo"} |= "bar" [5m])';
    const query: LokiQuery = { expr: DEFAULT_EXPR, refId: 'A' };
    const mockedGetAdhocFilters = templateSrvStub.getAdhocFilters as jest.Mock;
    const ds = createLokiDatasource(templateSrvStub);

    it('should not modify expression with no filters', async () => {
      expect(ds.applyTemplateVariables(query, {}).expr).toBe(DEFAULT_EXPR);
    });

    it('should add filters to expression', async () => {
      mockedGetAdhocFilters.mockReturnValue([
        {
          key: 'k1',
          operator: '=',
          value: 'v1',
        },
        {
          key: 'k2',
          operator: '!=',
          value: 'v2',
        },
      ]);

      expect(ds.applyTemplateVariables(query, {}).expr).toBe(
        'rate({bar="baz", job="foo", k1="v1", k2!="v2"} |= "bar" [5m])'
      );
    });

    it('should add escaping if needed to regex filter expressions', async () => {
      mockedGetAdhocFilters.mockReturnValue([
        {
          key: 'k1',
          operator: '=~',
          value: 'v.*',
        },
        {
          key: 'k2',
          operator: '=~',
          value: `v'.*`,
        },
      ]);
      expect(ds.applyTemplateVariables(query, {}).expr).toBe(
        'rate({bar="baz", job="foo", k1=~"v.*", k2=~"v\\\\\'.*"} |= "bar" [5m])'
      );
    });
  });

  describe('when interpolating variables', () => {
    let ds: LokiDatasource;
    let variable: CustomVariableModel;

    beforeEach(() => {
      ds = createLokiDatasource(templateSrvStub);
      variable = { ...initialCustomVariableModelState };
    });

    it('should only escape single quotes', () => {
      expect(ds.interpolateQueryExpr("abc'$^*{}[]+?.()|", variable)).toEqual("abc\\\\'$^*{}[]+?.()|");
    });

    it('should return a number', () => {
      expect(ds.interpolateQueryExpr(1000, variable)).toEqual(1000);
    });

    describe('and variable allows multi-value', () => {
      beforeEach(() => {
        variable.multi = true;
      });

      it('should regex escape values if the value is a string', () => {
        expect(ds.interpolateQueryExpr('looking*glass', variable)).toEqual('looking\\\\*glass');
      });

      it('should return pipe separated values if the value is an array of strings', () => {
        expect(ds.interpolateQueryExpr(['a|bc', 'de|f'], variable)).toEqual('a\\\\|bc|de\\\\|f');
      });
    });

    describe('and variable allows all', () => {
      beforeEach(() => {
        variable.includeAll = true;
      });

      it('should regex escape values if the array is a string', () => {
        expect(ds.interpolateQueryExpr('looking*glass', variable)).toEqual('looking\\\\*glass');
      });

      it('should return pipe separated values if the value is an array of strings', () => {
        expect(ds.interpolateQueryExpr(['a|bc', 'de|f'], variable)).toEqual('a\\\\|bc|de\\\\|f');
      });
    });
  });

  describe('when running interpolateVariablesInQueries', () => {
    it('should call addAdHocFilters', () => {
      const ds = createLokiDatasource(templateSrvStub);
      ds.addAdHocFilters = jest.fn();
      const expr = 'rate({bar="baz", job="foo"} [5m]';
      const queries = [
        {
          refId: 'A',
          expr,
        },
      ];
      ds.interpolateVariablesInQueries(queries, {});
      expect(ds.addAdHocFilters).toHaveBeenCalledWith(expr);
    });
  });

  describe('when calling annotationQuery', () => {
    const getTestContext = (frame: DataFrame, options = {}) => {
      const query = makeAnnotationQueryRequest(options);

      const ds = createLokiDatasource(templateSrvStub);
      const response: DataQueryResponse = {
        data: [frame],
      };
      ds.query = () => of(response);
      return ds.annotationQuery(query);
    };

    it('should transform the loki data to annotation response', async () => {
      const testFrame: DataFrame = {
        refId: 'A',
        fields: [
          {
            name: 'Time',
            type: FieldType.time,
            config: {},
            values: [1, 2],
          },
          {
            name: 'Line',
            type: FieldType.string,
            config: {},
            values: ['hello', 'hello 2'],
          },
          {
            name: 'labels',
            type: FieldType.other,
            config: {},
            values: [
              {
                label: 'value',
                label2: 'value ',
              },
              {
                label: '',
                label2: 'value2',
                label3: ' ',
              },
            ],
          },
          {
            name: 'tsNs',
            type: FieldType.string,
            config: {},
            values: ['1000000', '2000000'],
          },
          {
            name: 'id',
            type: FieldType.string,
            config: {},
            values: ['id1', 'id2'],
          },
        ],
        length: 2,
      };
      const res = await getTestContext(testFrame, { stepInterval: '15s' });

      expect(res.length).toBe(2);
      expect(res[0].text).toBe('hello');
      expect(res[0].tags).toEqual(['value']);

      expect(res[1].text).toBe('hello 2');
      expect(res[1].tags).toEqual(['value2']);
    });

    describe('Formatting', () => {
      const testFrame: DataFrame = {
        refId: 'A',
        fields: [
          {
            name: 'Time',
            type: FieldType.time,
            config: {},
            values: [1],
          },
          {
            name: 'Line',
            type: FieldType.string,
            config: {},
            values: ['hello'],
          },
          {
            name: 'labels',
            type: FieldType.other,
            config: {},
            values: [
              {
                label: 'value',
                label2: 'value2',
                label3: 'value3',
              },
            ],
          },
          {
            name: 'tsNs',
            type: FieldType.string,
            config: {},
            values: ['1000000'],
          },
          {
            name: 'id',
            type: FieldType.string,
            config: {},
            values: ['id1'],
          },
        ],
        length: 1,
      };
      describe('When tagKeys is set', () => {
        it('should only include selected labels', async () => {
          const res = await getTestContext(testFrame, { tagKeys: 'label2,label3', stepInterval: '15s' });

          expect(res.length).toBe(1);
          expect(res[0].text).toBe('hello');
          expect(res[0].tags).toEqual(['value2', 'value3']);
        });
      });
      describe('When textFormat is set', () => {
        it('should format the text accordingly', async () => {
          const res = await getTestContext(testFrame, { textFormat: 'hello {{label2}}', stepInterval: '15s' });

          expect(res.length).toBe(1);
          expect(res[0].text).toBe('hello value2');
        });
      });
      describe('When titleFormat is set', () => {
        it('should format the title accordingly', async () => {
          const res = await getTestContext(testFrame, { titleFormat: 'Title {{label2}}', stepInterval: '15s' });

          expect(res.length).toBe(1);
          expect(res[0].title).toBe('Title value2');
          expect(res[0].text).toBe('hello');
        });
      });
    });
  });

  describe('metricFindQuery', () => {
    const getTestContext = () => {
      const ds = createLokiDatasource(templateSrvStub);
      jest
        .spyOn(ds, 'metadataRequest')
        .mockImplementation(
          createMetadataRequest(
            { label1: ['value1', 'value2'], label2: ['value3', 'value4'] },
            { '{label1="value1", label2="value2"}': [{ label5: 'value5' }] }
          )
        );

      return { ds };
    };

    it('should return label names for Loki', async () => {
      const { ds } = getTestContext();

      const legacyResult = await ds.metricFindQuery('label_names()');
      const result = await ds.metricFindQuery({ refId: 'test', type: LokiVariableQueryType.LabelNames });

      expect(legacyResult).toEqual(result);
      expect(result).toEqual([{ text: 'label1' }, { text: 'label2' }]);
    });

    it('should return label values for Loki when no matcher', async () => {
      const { ds } = getTestContext();

      const legacyResult = await ds.metricFindQuery('label_values(label1)');
      const result = await ds.metricFindQuery({
        refId: 'test',
        type: LokiVariableQueryType.LabelValues,
        label: 'label1',
      });

      expect(legacyResult).toEqual(result);
      expect(result).toEqual([{ text: 'value1' }, { text: 'value2' }]);
    });

    it('should return label values for Loki with matcher', async () => {
      const { ds } = getTestContext();

      const legacyResult = await ds.metricFindQuery('label_values({label1="value1", label2="value2"},label5)');
      const result = await ds.metricFindQuery({
        refId: 'test',
        type: LokiVariableQueryType.LabelValues,
        stream: '{label1="value1", label2="value2"}',
        label: 'label5',
      });

      expect(legacyResult).toEqual(result);
      expect(result).toEqual([{ text: 'value5' }]);
    });

    it('should return empty array when incorrect query for Loki', async () => {
      const { ds } = getTestContext();

      const result = await ds.metricFindQuery('incorrect_query');

      expect(result).toEqual([]);
    });

    it('should interpolate strings in the query', async () => {
      const { ds } = getTestContext();
      const scopedVars = { scopedVar1: { value: 'A' } };

      await ds.metricFindQuery('label_names()', { scopedVars });
      await ds.metricFindQuery(
        {
          refId: 'test',
          type: LokiVariableQueryType.LabelValues,
          stream: '{label1="value1", label2="value2"}',
          label: 'label5',
        },
        { scopedVars }
      );

      expect(templateSrvStub.replace).toHaveBeenCalledWith('label_names()', scopedVars, expect.any(Function));
      expect(templateSrvStub.replace).toHaveBeenCalledWith(
        '{label1="value1", label2="value2"}',
        scopedVars,
        expect.any(Function)
      );
      expect(templateSrvStub.replace).toHaveBeenCalledWith('label5', scopedVars, expect.any(Function));
    });
  });

  describe('modifyQuery', () => {
    describe('when called with ADD_FILTER', () => {
      let ds: LokiDatasource;
      beforeEach(() => {
        ds = createLokiDatasource(templateSrvStub);
        ds.languageProvider.labelKeys = ['bar', 'job'];
      });

      describe('and query has no parser', () => {
        it('then the correct label should be added for logs query', () => {
          const query: LokiQuery = { refId: 'A', expr: '{bar="baz"}' };
          const action = { options: { key: 'job', value: 'grafana' }, type: 'ADD_FILTER' };
          const result = ds.modifyQuery(query, action);

          expect(result.refId).toEqual('A');
          expect(result.expr).toEqual('{bar="baz", job="grafana"}');
        });

        it('then the correctly escaped label should be added for logs query', () => {
          const query: LokiQuery = { refId: 'A', expr: '{bar="baz"}' };
          const action = { options: { key: 'job', value: '\\test' }, type: 'ADD_FILTER' };
          const result = ds.modifyQuery(query, action);

          expect(result.refId).toEqual('A');
          expect(result.expr).toEqual('{bar="baz", job="\\\\test"}');
        });

        it('then the correct label should be added for metrics query', () => {
          const query: LokiQuery = { refId: 'A', expr: 'rate({bar="baz"}[5m])' };
          const action = { options: { key: 'job', value: 'grafana' }, type: 'ADD_FILTER' };
          const result = ds.modifyQuery(query, action);

          expect(result.refId).toEqual('A');
          expect(result.expr).toEqual('rate({bar="baz", job="grafana"}[5m])');
        });

        it('then the correct label should be added for non-indexed metadata as LabelFilter', () => {
          const query: LokiQuery = { refId: 'A', expr: '{bar="baz"}' };
          const action = { options: { key: 'job', value: 'grafana' }, type: 'ADD_FILTER' };
          ds.languageProvider.labelKeys = ['bar'];
          const result = ds.modifyQuery(query, action);

          expect(result.refId).toEqual('A');
          expect(result.expr).toEqual('{bar="baz"} | job=`grafana`');
        });
      });
      describe('and query has parser', () => {
        it('then the correct label should be added for logs query', () => {
          const query: LokiQuery = { refId: 'A', expr: '{bar="baz"} | logfmt' };
          const action = { options: { key: 'job', value: 'grafana' }, type: 'ADD_FILTER' };
          const result = ds.modifyQuery(query, action);

          expect(result.refId).toEqual('A');
          expect(result.expr).toEqual('{bar="baz"} | logfmt | job=`grafana`');
        });
        it('then the correct label should be added for metrics query', () => {
          const query: LokiQuery = { refId: 'A', expr: 'rate({bar="baz"} | logfmt [5m])' };
          const action = { options: { key: 'job', value: 'grafana' }, type: 'ADD_FILTER' };
          const result = ds.modifyQuery(query, action);

          expect(result.refId).toEqual('A');
          expect(result.expr).toEqual('rate({bar="baz"} | logfmt | job=`grafana` [5m])');
        });
      });
    });

    describe('when called with ADD_FILTER_OUT', () => {
      let ds: LokiDatasource;
      beforeEach(() => {
        ds = createLokiDatasource(templateSrvStub);
        ds.languageProvider.labelKeys = ['bar', 'job'];
      });
      describe('and query has no parser', () => {
        it('then the correct label should be added for logs query', () => {
          const query: LokiQuery = { refId: 'A', expr: '{bar="baz"}' };
          const action = { options: { key: 'job', value: 'grafana' }, type: 'ADD_FILTER_OUT' };
          const result = ds.modifyQuery(query, action);

          expect(result.refId).toEqual('A');
          expect(result.expr).toEqual('{bar="baz", job!="grafana"}');
        });

        it('then the correctly escaped label should be added for logs query', () => {
          const query: LokiQuery = { refId: 'A', expr: '{bar="baz"}' };
          const action = { options: { key: 'job', value: '"test' }, type: 'ADD_FILTER_OUT' };
          const result = ds.modifyQuery(query, action);

          expect(result.refId).toEqual('A');
          expect(result.expr).toEqual('{bar="baz", job!="\\"test"}');
        });

        it('then the correct label should be added for metrics query', () => {
          const query: LokiQuery = { refId: 'A', expr: 'rate({bar="baz"}[5m])' };
          const action = { options: { key: 'job', value: 'grafana' }, type: 'ADD_FILTER_OUT' };
          const result = ds.modifyQuery(query, action);

          expect(result.refId).toEqual('A');
          expect(result.expr).toEqual('rate({bar="baz", job!="grafana"}[5m])');
        });
      });
      describe('and query has parser', () => {
        let ds: LokiDatasource;
        beforeEach(() => {
          ds = createLokiDatasource(templateSrvStub);
          ds.languageProvider.labelKeys = ['bar', 'job'];
        });

        it('then the correct label should be added for logs query', () => {
          const query: LokiQuery = { refId: 'A', expr: '{bar="baz"} | logfmt' };
          const action = { options: { key: 'job', value: 'grafana' }, type: 'ADD_FILTER_OUT' };
          const result = ds.modifyQuery(query, action);

          expect(result.refId).toEqual('A');
          expect(result.expr).toEqual('{bar="baz"} | logfmt | job!=`grafana`');
        });
        it('then the correct label should be added for metrics query', () => {
          const query: LokiQuery = { refId: 'A', expr: 'rate({bar="baz"} | logfmt [5m])' };
          const action = { options: { key: 'job', value: 'grafana' }, type: 'ADD_FILTER_OUT' };
          const result = ds.modifyQuery(query, action);

          expect(result.refId).toEqual('A');
          expect(result.expr).toEqual('rate({bar="baz"} | logfmt | job!=`grafana` [5m])');
        });
      });
    });
  });

  describe('toggleQueryFilter', () => {
    describe('when called with FILTER', () => {
      let ds: LokiDatasource;
      beforeEach(() => {
        ds = createLokiDatasource(templateSrvStub);
      });

      describe('and query has no parser', () => {
        it('then the correct label should be added for logs query', () => {
          const query: LokiQuery = { refId: 'A', expr: '{bar="baz"}' };
          const action: ToggleFilterAction = { options: { key: 'job', value: 'grafana' }, type: 'FILTER_FOR' };
          const result = ds.toggleQueryFilter(query, action);

          expect(result.refId).toEqual('A');
          expect(result.expr).toEqual('{bar="baz", job="grafana"}');
        });

        it('then the correctly escaped label should be added for logs query', () => {
          const query: LokiQuery = { refId: 'A', expr: '{bar="baz"}' };
          const action: ToggleFilterAction = { options: { key: 'job', value: '\\test' }, type: 'FILTER_FOR' };
          const result = ds.toggleQueryFilter(query, action);

          expect(result.refId).toEqual('A');
          expect(result.expr).toEqual('{bar="baz", job="\\\\test"}');
        });

        it('then the correct label should be added for metrics query', () => {
          const query: LokiQuery = { refId: 'A', expr: 'rate({bar="baz"}[5m])' };
          const action: ToggleFilterAction = { options: { key: 'job', value: 'grafana' }, type: 'FILTER_FOR' };
          const result = ds.toggleQueryFilter(query, action);

          expect(result.refId).toEqual('A');
          expect(result.expr).toEqual('rate({bar="baz", job="grafana"}[5m])');
        });

        describe('and the filter is already present', () => {
          it('then it should remove the filter', () => {
            const query: LokiQuery = { refId: 'A', expr: '{bar="baz", job="grafana"}' };
            const action: ToggleFilterAction = { options: { key: 'job', value: 'grafana' }, type: 'FILTER_FOR' };
            const result = ds.toggleQueryFilter(query, action);

            expect(result.refId).toEqual('A');
            expect(result.expr).toEqual('{bar="baz"}');
          });

          it('then it should remove the filter with escaped value', () => {
            const query: LokiQuery = { refId: 'A', expr: '{place="luna", job="\\"grafana/data\\""}' };
            const action: ToggleFilterAction = { options: { key: 'job', value: '"grafana/data"' }, type: 'FILTER_FOR' };
            const result = ds.toggleQueryFilter(query, action);

            expect(result.refId).toEqual('A');
            expect(result.expr).toEqual('{place="luna"}');
          });
        });
      });

      describe('and query has parser', () => {
        it('then the correct label should be added for logs query', () => {
          const query: LokiQuery = { refId: 'A', expr: '{bar="baz"} | logfmt' };
          const action: ToggleFilterAction = { options: { key: 'job', value: 'grafana' }, type: 'FILTER_FOR' };
          const result = ds.toggleQueryFilter(query, action);

          expect(result.refId).toEqual('A');
          expect(result.expr).toEqual('{bar="baz"} | logfmt | job=`grafana`');
        });

        it('then the correct label should be added for metrics query', () => {
          const query: LokiQuery = { refId: 'A', expr: 'rate({bar="baz"} | logfmt [5m])' };
          const action: ToggleFilterAction = { options: { key: 'job', value: 'grafana' }, type: 'FILTER_FOR' };
          const result = ds.toggleQueryFilter(query, action);

          expect(result.refId).toEqual('A');
          expect(result.expr).toEqual('rate({bar="baz"} | logfmt | job=`grafana` [5m])');
        });

        describe('and the filter is already present', () => {
          it('then it should remove the filter', () => {
            const query: LokiQuery = { refId: 'A', expr: '{bar="baz"} | logfmt | job="grafana"' };
            const action: ToggleFilterAction = { options: { key: 'job', value: 'grafana' }, type: 'FILTER_FOR' };
            const result = ds.toggleQueryFilter(query, action);

            expect(result.refId).toEqual('A');
            expect(result.expr).toEqual('{bar="baz"} | logfmt');
          });
        });
      });
    });

    describe('when called with FILTER_OUT', () => {
      describe('and query has no parser', () => {
        let ds: LokiDatasource;
        beforeEach(() => {
          ds = createLokiDatasource(templateSrvStub);
        });

        it('then the correct label should be added for logs query', () => {
          const query: LokiQuery = { refId: 'A', expr: '{bar="baz"}' };
          const action: ToggleFilterAction = { options: { key: 'job', value: 'grafana' }, type: 'FILTER_OUT' };
          const result = ds.toggleQueryFilter(query, action);

          expect(result.refId).toEqual('A');
          expect(result.expr).toEqual('{bar="baz", job!="grafana"}');
        });

        it('then the correctly escaped label should be added for logs query', () => {
          const query: LokiQuery = { refId: 'A', expr: '{bar="baz"}' };
          const action: ToggleFilterAction = { options: { key: 'job', value: '"test' }, type: 'FILTER_OUT' };
          const result = ds.toggleQueryFilter(query, action);

          expect(result.refId).toEqual('A');
          expect(result.expr).toEqual('{bar="baz", job!="\\"test"}');
        });

        it('then the correct label should be added for metrics query', () => {
          const query: LokiQuery = { refId: 'A', expr: 'rate({bar="baz"}[5m])' };
          const action: ToggleFilterAction = { options: { key: 'job', value: 'grafana' }, type: 'FILTER_OUT' };
          const result = ds.toggleQueryFilter(query, action);

          expect(result.refId).toEqual('A');
          expect(result.expr).toEqual('rate({bar="baz", job!="grafana"}[5m])');
        });

        describe('and the opposite filter is present', () => {
          it('then it should remove the filter', () => {
            const query: LokiQuery = { refId: 'A', expr: '{bar="baz", job="grafana"}' };
            const action: ToggleFilterAction = { options: { key: 'job', value: 'grafana' }, type: 'FILTER_OUT' };
            const result = ds.toggleQueryFilter(query, action);

            expect(result.refId).toEqual('A');
            expect(result.expr).toEqual('{bar="baz", job!="grafana"}');
          });
        });
      });

      describe('and query has parser', () => {
        let ds: LokiDatasource;
        beforeEach(() => {
          ds = createLokiDatasource(templateSrvStub);
        });

        it('then the correct label should be added for logs query', () => {
          const query: LokiQuery = { refId: 'A', expr: '{bar="baz"} | logfmt' };
          const action: ToggleFilterAction = { options: { key: 'job', value: 'grafana' }, type: 'FILTER_OUT' };
          const result = ds.toggleQueryFilter(query, action);

          expect(result.refId).toEqual('A');
          expect(result.expr).toEqual('{bar="baz"} | logfmt | job!=`grafana`');
        });

        it('then the correct label should be added for metrics query', () => {
          const query: LokiQuery = { refId: 'A', expr: 'rate({bar="baz"} | logfmt [5m])' };
          const action: ToggleFilterAction = { options: { key: 'job', value: 'grafana' }, type: 'FILTER_OUT' };
          const result = ds.toggleQueryFilter(query, action);

          expect(result.refId).toEqual('A');
          expect(result.expr).toEqual('rate({bar="baz"} | logfmt | job!=`grafana` [5m])');
        });

        describe('and the filter is already present', () => {
          it('then it should remove the filter', () => {
            const query: LokiQuery = { refId: 'A', expr: '{bar="baz"} | logfmt | job="grafana"' };
            const action: ToggleFilterAction = { options: { key: 'job', value: 'grafana' }, type: 'FILTER_OUT' };
            const result = ds.toggleQueryFilter(query, action);

            expect(result.refId).toEqual('A');
            expect(result.expr).toEqual('{bar="baz"} | logfmt | job!=`grafana`');
          });
        });
      });
    });
  });

  describe('addAdHocFilters', () => {
    let ds: LokiDatasource;
    const createTemplateSrvMock = (options: { adHocFilters: AdHocFilter[] }) => {
      return {
        getAdhocFilters: (): AdHocFilter[] => options.adHocFilters,
        replace: (a: string) => a,
      } as unknown as TemplateSrv;
    };
    describe('when called with "=" operator', () => {
      beforeEach(() => {
        const defaultAdHocFilters: AdHocFilter[] = [
          {
            condition: '',
            key: 'job',
            operator: '=',
            value: 'grafana',
          },
        ];
        ds = createLokiDatasource(createTemplateSrvMock({ adHocFilters: defaultAdHocFilters }));
      });
      describe('and query has no parser', () => {
        it('then the correct label should be added for logs query', () => {
          assertAdHocFilters('{bar="baz"}', '{bar="baz", job="grafana"}', ds);
        });

        it('then the correct label should be added for metrics query', () => {
          assertAdHocFilters('rate({bar="baz"}[5m])', 'rate({bar="baz", job="grafana"}[5m])', ds);
        });

        it('then the correct label should be added for metrics query and variable', () => {
          assertAdHocFilters('rate({bar="baz"}[$__interval])', 'rate({bar="baz", job="grafana"}[$__interval])', ds);
        });

        it('then the correct label should be added for logs query with empty selector', () => {
          assertAdHocFilters('{}', '{job="grafana"}', ds);
        });

        it('then the correct label should be added for metrics query with empty selector', () => {
          assertAdHocFilters('rate({}[5m])', 'rate({job="grafana"}[5m])', ds);
        });

        it('then the correct label should be added for metrics query with empty selector and variable', () => {
          assertAdHocFilters('rate({}[$__interval])', 'rate({job="grafana"}[$__interval])', ds);
        });
        it('should correctly escape special characters in ad hoc filter', () => {
          const ds = createLokiDatasource(
            createTemplateSrvMock({
              adHocFilters: [
                {
                  condition: '',
                  key: 'instance',
                  operator: '=',
                  value: '"test"',
                },
              ],
            })
          );
          assertAdHocFilters('{job="grafana"}', '{job="grafana", instance="\\"test\\""}', ds);
        });
      });
      describe('and query has parser', () => {
        it('then the correct label should be added for logs query', () => {
          assertAdHocFilters('{bar="baz"} | logfmt', '{bar="baz"} | logfmt | job=`grafana`', ds);
        });
        it('then the correct label should be added for metrics query', () => {
          assertAdHocFilters('rate({bar="baz"} | logfmt [5m])', 'rate({bar="baz"} | logfmt | job=`grafana` [5m])', ds);
        });
      });
    });

    describe('when called with "!=" operator', () => {
      beforeEach(() => {
        const defaultAdHocFilters: AdHocFilter[] = [
          {
            condition: '',
            key: 'job',
            operator: '!=',
            value: 'grafana',
          },
        ];
        ds = createLokiDatasource(createTemplateSrvMock({ adHocFilters: defaultAdHocFilters }));
      });
      describe('and query has no parser', () => {
        it('then the correct label should be added for logs query', () => {
          assertAdHocFilters('{bar="baz"}', '{bar="baz", job!="grafana"}', ds);
        });

        it('then the correct label should be added for metrics query', () => {
          assertAdHocFilters('rate({bar="baz"}[5m])', 'rate({bar="baz", job!="grafana"}[5m])', ds);
        });
      });
      describe('and query has parser', () => {
        it('then the correct label should be added for logs query', () => {
          assertAdHocFilters('{bar="baz"} | logfmt', '{bar="baz"} | logfmt | job!=`grafana`', ds);
        });
        it('then the correct label should be added for metrics query', () => {
          assertAdHocFilters('rate({bar="baz"} | logfmt [5m])', 'rate({bar="baz"} | logfmt | job!=`grafana` [5m])', ds);
        });
      });
    });

    describe('when called with regex operator', () => {
      beforeEach(() => {
        const defaultAdHocFilters: AdHocFilter[] = [
          {
            condition: '',
            key: 'instance',
            operator: '=~',
            value: '.*',
          },
        ];
        ds = createLokiDatasource(createTemplateSrvMock({ adHocFilters: defaultAdHocFilters }));
      });
      it('should not escape special characters in ad hoc filter', () => {
        assertAdHocFilters('{job="grafana"}', '{job="grafana", instance=~".*"}', ds);
      });
    });
  });

  describe('logs volume data provider', () => {
    let ds: LokiDatasource;
    beforeEach(() => {
      ds = createLokiDatasource(templateSrvStub);
    });

    it('creates provider for logs query', () => {
      const options = getQueryOptions<LokiQuery>({
        targets: [{ expr: '{label=value}', refId: 'A', queryType: LokiQueryType.Range }],
      });

      expect(ds.getDataProvider(SupplementaryQueryType.LogsVolume, options)).toBeDefined();
    });

    it('does not create provider for metrics query', () => {
      const options = getQueryOptions<LokiQuery>({
        targets: [{ expr: 'rate({label=value}[1m])', refId: 'A' }],
      });

      expect(ds.getDataProvider(SupplementaryQueryType.LogsVolume, options)).not.toBeDefined();
    });

    it('creates provider if at least one query is a logs query', () => {
      const options = getQueryOptions<LokiQuery>({
        targets: [
          { expr: 'rate({label=value}[1m])', queryType: LokiQueryType.Range, refId: 'A' },
          { expr: '{label=value}', queryType: LokiQueryType.Range, refId: 'B' },
        ],
      });

      expect(ds.getDataProvider(SupplementaryQueryType.LogsVolume, options)).toBeDefined();
    });

    it('does not create provider if there is only an instant logs query', () => {
      const options = getQueryOptions<LokiQuery>({
        targets: [{ expr: '{label=value', refId: 'A', queryType: LokiQueryType.Instant }],
      });

      expect(ds.getDataProvider(SupplementaryQueryType.LogsVolume, options)).not.toBeDefined();
    });
  });

  describe('logs sample data provider', () => {
    let ds: LokiDatasource;
    beforeEach(() => {
      ds = createLokiDatasource(templateSrvStub);
    });

    it('creates provider for metrics query', () => {
      const options = getQueryOptions<LokiQuery>({
        targets: [{ expr: 'rate({label=value}[5m])', refId: 'A' }],
      });

      expect(ds.getDataProvider(SupplementaryQueryType.LogsSample, options)).toBeDefined();
    });

    it('does not create provider for log query', () => {
      const options = getQueryOptions<LokiQuery>({
        targets: [{ expr: '{label=value}', refId: 'A' }],
      });

      expect(ds.getDataProvider(SupplementaryQueryType.LogsSample, options)).not.toBeDefined();
    });

    it('creates provider if at least one query is a metric query', () => {
      const options = getQueryOptions<LokiQuery>({
        targets: [
          { expr: 'rate({label=value}[1m])', refId: 'A' },
          { expr: '{label=value}', refId: 'B' },
        ],
      });

      expect(ds.getDataProvider(SupplementaryQueryType.LogsSample, options)).toBeDefined();
    });
  });

  describe('getSupplementaryQuery', () => {
    let ds: LokiDatasource;
    beforeEach(() => {
      ds = createLokiDatasource(templateSrvStub);
    });

    describe('logs volume', () => {
      // The default queryType value is Range
      it('returns logs volume for query with no queryType', () => {
        expect(
          ds.getSupplementaryQuery(
            { type: SupplementaryQueryType.LogsVolume },
            {
              expr: '{label=value}',
              refId: 'A',
            }
          )
        ).toEqual({
          expr: 'sum by (level) (count_over_time({label=value}[$__auto]))',
          queryType: LokiQueryType.Range,
          refId: 'log-volume-A',
          supportingQueryType: SupportingQueryType.LogsVolume,
        });
      });

      it('returns logs volume query for range log query', () => {
        expect(
          ds.getSupplementaryQuery(
            { type: SupplementaryQueryType.LogsVolume },
            {
              expr: '{label=value}',
              queryType: LokiQueryType.Range,
              refId: 'A',
            }
          )
        ).toEqual({
          expr: 'sum by (level) (count_over_time({label=value}[$__auto]))',
          queryType: LokiQueryType.Range,
          refId: 'log-volume-A',
          supportingQueryType: SupportingQueryType.LogsVolume,
        });
      });

      it('does not return logs volume query for instant log query', () => {
        expect(
          ds.getSupplementaryQuery(
            { type: SupplementaryQueryType.LogsVolume },
            {
              expr: '{label=value}',
              queryType: LokiQueryType.Instant,
              refId: 'A',
            }
          )
        ).toEqual(undefined);
      });

      it('does not return logs volume query for metric query', () => {
        expect(
          ds.getSupplementaryQuery(
            { type: SupplementaryQueryType.LogsVolume },
            {
              expr: 'rate({label=value}[5m]',
              queryType: LokiQueryType.Range,
              refId: 'A',
            }
          )
        ).toEqual(undefined);
      });
    });

    describe('logs sample', () => {
      it('returns logs sample query for range metric query', () => {
        expect(
          ds.getSupplementaryQuery(
            { type: SupplementaryQueryType.LogsSample },
            {
              expr: 'rate({label=value}[5m]',
              queryType: LokiQueryType.Range,
              refId: 'A',
            }
          )
        ).toEqual({
          expr: '{label=value}',
          queryType: 'range',
          refId: 'log-sample-A',
          maxLines: 20,
        });
      });

      it('returns logs sample query for instant metric query', () => {
        expect(
          ds.getSupplementaryQuery(
            { type: SupplementaryQueryType.LogsSample },
            {
              expr: 'rate({label=value}[5m]',
              queryType: LokiQueryType.Instant,
              refId: 'A',
            }
          )
        ).toEqual({
          expr: '{label=value}',
          queryType: LokiQueryType.Range,
          refId: 'log-sample-A',
          maxLines: 20,
        });
      });

      it('correctly overrides maxLines if limit is set', () => {
        expect(
          ds.getSupplementaryQuery(
            { type: SupplementaryQueryType.LogsSample, limit: 5 },
            {
              expr: 'rate({label=value}[5m]',
              queryType: LokiQueryType.Instant,
              refId: 'A',
            }
          )
        ).toEqual({
          expr: '{label=value}',
          queryType: LokiQueryType.Range,
          refId: 'log-sample-A',
          maxLines: 5,
        });
      });

      it('does not return logs sample query for log query query', () => {
        expect(
          ds.getSupplementaryQuery(
            { type: SupplementaryQueryType.LogsSample },
            {
              expr: '{label=value}',
              queryType: LokiQueryType.Range,
              refId: 'A',
            }
          )
        ).toEqual(undefined);
      });
    });
  });

  describe('importing queries', () => {
    let ds: LokiDatasource;
    beforeEach(() => {
      ds = createLokiDatasource(templateSrvStub);
    });

    it('keeps all labels when no labels are loaded', async () => {
      ds.getResource = <T>() => Promise.resolve({ data: [] } as T);
      const queries = await ds.importFromAbstractQueries([
        {
          refId: 'A',
          labelMatchers: [
            { name: 'foo', operator: AbstractLabelOperator.Equal, value: 'bar' },
            { name: 'foo2', operator: AbstractLabelOperator.Equal, value: 'bar2' },
          ],
        },
      ]);
      expect(queries[0].expr).toBe('{foo="bar", foo2="bar2"}');
    });

    it('filters out non existing labels', async () => {
      ds.getResource = <T>() => Promise.resolve({ data: ['foo'] } as T);
      const queries = await ds.importFromAbstractQueries([
        {
          refId: 'A',
          labelMatchers: [
            { name: 'foo', operator: AbstractLabelOperator.Equal, value: 'bar' },
            { name: 'foo2', operator: AbstractLabelOperator.Equal, value: 'bar2' },
          ],
        },
      ]);
      expect(queries[0].expr).toBe('{foo="bar"}');
    });
  });

  describe('getDataSamples', () => {
    let ds: LokiDatasource;
    beforeEach(() => {
      ds = createLokiDatasource(templateSrvStub);
    });
    it('ignores invalid queries', () => {
      const spy = jest.spyOn(ds, 'query');
      ds.getDataSamples({ expr: 'not a query', refId: 'A' });
      expect(spy).not.toHaveBeenCalled();
    });
    it('ignores metric queries', () => {
      const spy = jest.spyOn(ds, 'query');
      ds.getDataSamples({ expr: 'count_over_time({a="b"}[1m])', refId: 'A' });
      expect(spy).not.toHaveBeenCalled();
    });
    it('uses the current interval in the request', () => {
      const spy = jest.spyOn(ds, 'query').mockImplementation(() => of({} as DataQueryResponse));
      ds.getDataSamples({ expr: '{job="bar"}', refId: 'A' });
      expect(spy).toHaveBeenCalledWith(
        expect.objectContaining({
          range: ds.getTimeRange(),
        })
      );
    });
    it('hides the request from the inspector', () => {
      const spy = jest.spyOn(ds, 'query').mockImplementation(() => of({} as DataQueryResponse));
      ds.getDataSamples({ expr: '{job="bar"}', refId: 'A' });
      expect(spy).toHaveBeenCalledWith(
        expect.objectContaining({
          hideFromInspector: true,
          requestId: REF_ID_DATA_SAMPLES,
        })
      );
    });
  });

  describe('Query splitting', () => {
    beforeAll(() => {
      config.featureToggles.lokiQuerySplitting = true;
      jest.mocked(runSplitQuery).mockReturnValue(
        of({
          data: [],
        })
      );
    });
    afterAll(() => {
      config.featureToggles.lokiQuerySplitting = false;
    });
    it.each([
      [[{ expr: 'count_over_time({a="b"}[1m])', refId: 'A' }]],
      [[{ expr: '{a="b"}', refId: 'A' }]],
      [
        [
          { expr: 'count_over_time({a="b"}[1m])', refId: 'A', hide: true },
          { expr: '{a="b"}', refId: 'B' },
        ],
      ],
    ])('supports query splitting when the requirements are met', async (targets: LokiQuery[]) => {
      const ds = createLokiDatasource(templateSrvStub);
      const query = getQueryOptions<LokiQuery>({
        targets,
        app: CoreApp.Dashboard,
      });

      await expect(ds.query(query)).toEmitValuesWith(() => {
        expect(runSplitQuery).toHaveBeenCalled();
      });
    });
  });

  describe('getQueryStats', () => {
    let ds: LokiDatasource;
    let query: LokiQuery;
    beforeEach(() => {
      ds = createLokiDatasource(templateSrvStub);
      ds.statsMetadataRequest = jest.fn().mockResolvedValue({ streams: 1, chunks: 1, bytes: 1, entries: 1 });
      ds.interpolateString = jest.fn().mockImplementation((value: string) => value.replace('$__interval', '1m'));

      query = { refId: 'A', expr: '', queryType: LokiQueryType.Range };
    });

    it('uses statsMetadataRequest', async () => {
      query.expr = '{foo="bar"}';
      const result = await ds.getQueryStats(query);

      expect(ds.statsMetadataRequest).toHaveBeenCalled();
      expect(result).toEqual({ streams: 1, chunks: 1, bytes: 1, entries: 1 });
    });

    it('supports queries with template variables', async () => {
      query.expr = 'rate({instance="server\\1"}[$__interval])';
      const result = await ds.getQueryStats(query);

      expect(result).toEqual({
        streams: 1,
        chunks: 1,
        bytes: 1,
        entries: 1,
      });
    });

    it('does not call stats if the query is invalid', async () => {
      query.expr = 'rate({label="value"}';
      const result = await ds.getQueryStats(query);

      expect(ds.statsMetadataRequest).not.toHaveBeenCalled();
      expect(result).toBe(undefined);
    });

    it('combines the stats of each label matcher', async () => {
      query.expr = 'count_over_time({foo="bar"}[1m]) + count_over_time({test="test"}[1m])';
      const result = await ds.getQueryStats(query);

      expect(ds.statsMetadataRequest).toHaveBeenCalled();
      expect(result).toEqual({ streams: 2, chunks: 2, bytes: 2, entries: 2 });
    });
  });

  describe('statsMetadataRequest', () => {
    it('throws error if url starts with /', () => {
      const ds = createLokiDatasource();
      expect(async () => {
        await ds.statsMetadataRequest('/index');
      }).rejects.toThrow('invalid metadata request url: /index');
    });
  });
});

describe('applyTemplateVariables', () => {
  it('should add the adhoc filter to the query', () => {
    const ds = createLokiDatasource(templateSrvStub);
    const spy = jest.spyOn(ds, 'addAdHocFilters');
    ds.applyTemplateVariables({ expr: '{test}', refId: 'A' }, {});
    expect(spy).toHaveBeenCalledWith('{test}');
  });

  describe('with template and built-in variables', () => {
    const scopedVars = {
      __interval: { text: '1m', value: '1m' },
      __interval_ms: { text: '1000', value: '1000' },
      __range: { text: '1m', value: '1m' },
      __range_ms: { text: '1000', value: '1000' },
      __range_s: { text: '60', value: '60' },
      testVariable: { text: 'foo', value: 'foo' },
    };

    it('should not interpolate __interval variables', () => {
      const templateSrvMock = {
        getAdhocFilters: jest.fn().mockImplementation((query: string) => query),
        replace: jest.fn((a: string, ...rest: unknown[]) => a),
      } as unknown as TemplateSrv;

      const ds = createLokiDatasource(templateSrvMock);
      ds.addAdHocFilters = jest.fn().mockImplementation((query: string) => query);
      ds.applyTemplateVariables(
        { expr: 'rate({job="grafana"}[$__interval]) + rate({job="grafana"}[$__interval_ms])', refId: 'A' },
        scopedVars
      );
      expect(templateSrvMock.replace).toHaveBeenCalledTimes(2);
      // Interpolated legend
      expect(templateSrvMock.replace).toHaveBeenCalledWith(
        undefined,
        expect.not.objectContaining({
          __interval: { text: '1m', value: '1m' },
          __interval_ms: { text: '1000', value: '1000' },
        })
      );
      // Interpolated expr
      expect(templateSrvMock.replace).toHaveBeenCalledWith(
        'rate({job="grafana"}[$__interval]) + rate({job="grafana"}[$__interval_ms])',
        expect.not.objectContaining({
          __interval: { text: '1m', value: '1m' },
          __interval_ms: { text: '1000', value: '1000' },
        }),
        expect.any(Function)
      );
    });

    it('should not interpolate __range variables', () => {
      const templateSrvMock = {
        getAdhocFilters: jest.fn().mockImplementation((query: string) => query),
        replace: jest.fn((a: string, ...rest: unknown[]) => a),
      } as unknown as TemplateSrv;

      const ds = createLokiDatasource(templateSrvMock);
      ds.addAdHocFilters = jest.fn().mockImplementation((query: string) => query);
      ds.applyTemplateVariables(
        {
          expr: 'rate({job="grafana"}[$__range]) + rate({job="grafana"}[$__range_ms]) + rate({job="grafana"}[$__range_s])',
          refId: 'A',
        },
        scopedVars
      );
      expect(templateSrvMock.replace).toHaveBeenCalledTimes(2);
      // Interpolated legend
      expect(templateSrvMock.replace).toHaveBeenCalledWith(
        undefined,
        expect.not.objectContaining({
          __range: { text: '1m', value: '1m' },
          __range_ms: { text: '1000', value: '1000' },
          __range_s: { text: '60', value: '60' },
        })
      );
      // Interpolated expr
      expect(templateSrvMock.replace).toHaveBeenCalledWith(
        'rate({job="grafana"}[$__range]) + rate({job="grafana"}[$__range_ms]) + rate({job="grafana"}[$__range_s])',
        expect.not.objectContaining({
          __range: { text: '1m', value: '1m' },
          __range_ms: { text: '1000', value: '1000' },
          __range_s: { text: '60', value: '60' },
        }),
        expect.any(Function)
      );
    });
  });

  describe('getStatsTimeRange', () => {
    let query: LokiQuery;
    let datasource: LokiDatasource;

    beforeEach(() => {
      query = { refId: 'A', expr: '', queryType: LokiQueryType.Range };
      datasource = createLokiDatasource();

      datasource.getTimeRangeParams = jest.fn().mockReturnValue({
        start: 1672552800000000000, // 01 Jan 2023 06:00:00 GMT
        end: 1672639200000000000, //   02 Jan 2023 06:00:00 GMT
      });
    });

    it('should return the ds picker timerange for a logs query with range type', () => {
      // log queries with range type should request the ds picker timerange
      // in this case (1 day)
      query.expr = '{job="grafana"}';

      expect(datasource.getStatsTimeRange(query, 0)).toEqual({
        start: 1672552800000000000, // 01 Jan 2023 06:00:00 GMT
        end: 1672639200000000000, //   02 Jan 2023 06:00:00 GMT
      });
    });

    it('should return nothing for a logs query with instant type', () => {
      // log queries with instant type should be invalid.
      query.queryType = LokiQueryType.Instant;
      query.expr = '{job="grafana"}';

      expect(datasource.getStatsTimeRange(query, 0)).toEqual({
        start: undefined,
        end: undefined,
      });
    });

    it('should return the ds picker timerange', () => {
      // metric queries with range type should request ds picker timerange
      // in this case (1 day)
      query.expr = 'rate({job="grafana"}[5m])';

      expect(datasource.getStatsTimeRange(query, 0)).toEqual({
        start: 1672552800000000000, // 01 Jan 2023 06:00:00 GMT
        end: 1672639200000000000, //   02 Jan 2023 06:00:00 GMT
      });
    });

    it('should return the range duration for an instant metric query', () => {
      // metric queries with instant type should request range duration
      // in this case (5 minutes)
      query.queryType = LokiQueryType.Instant;
      query.expr = 'rate({job="grafana"}[5m])';

      expect(datasource.getStatsTimeRange(query, 0)).toEqual({
        start: 1672638900000000000, // 02 Jan 2023 05:55:00 GMT
        end: 1672639200000000000, //   02 Jan 2023 06:00:00 GMT
      });
    });
  });
});

describe('makeStatsRequest', () => {
  const datasource = createLokiDatasource();
  let query: LokiQuery;

  beforeEach(() => {
    query = { refId: 'A', expr: '', queryType: LokiQueryType.Range };
  });

  it('should return null if there is no query', () => {
    query.expr = '';
    expect(datasource.getStats(query)).resolves.toBe(null);
  });

  it('should return null if the query is invalid', () => {
    query.expr = '{job="grafana",';
    expect(datasource.getStats(query)).resolves.toBe(null);
  });

  it('should return null if the response has no data', () => {
    query.expr = '{job="grafana"}';
    datasource.getQueryStats = jest.fn().mockResolvedValue({ streams: 0, chunks: 0, bytes: 0, entries: 0 });
    expect(datasource.getStats(query)).resolves.toBe(null);
  });

  it('should return the stats if the response has data', () => {
    query.expr = '{job="grafana"}';

    datasource.getQueryStats = jest
      .fn()
      .mockResolvedValue({ streams: 1, chunks: 12611, bytes: 12913664, entries: 78344 });
    expect(datasource.getStats(query)).resolves.toEqual({
      streams: 1,
      chunks: 12611,
      bytes: 12913664,
      entries: 78344,
    });
  });

  it('should support queries with variables', () => {
    query.expr = 'count_over_time({job="grafana"}[$__interval])';

    datasource.interpolateString = jest
      .fn()
      .mockImplementationOnce((value: string) => value.replace('$__interval', '1h'));
    datasource.getQueryStats = jest
      .fn()
      .mockResolvedValue({ streams: 1, chunks: 12611, bytes: 12913664, entries: 78344 });
    expect(datasource.getStats(query)).resolves.toEqual({
      streams: 1,
      chunks: 12611,
      bytes: 12913664,
      entries: 78344,
    });
  });
});

describe('getTimeRange*()', () => {
  it('exposes the current time range', () => {
    const ds = createLokiDatasource();
    const timeRange = ds.getTimeRange();

    expect(timeRange.from).toBeDefined();
    expect(timeRange.to).toBeDefined();
  });

  it('exposes time range as params', () => {
    const ds = createLokiDatasource();
    const params = ds.getTimeRangeParams();

    // Returns a very big integer, so we stringify it for the assertion
    expect(JSON.stringify(params)).toEqual('{"start":1524650400000000000,"end":1524654000000000000}');
  });
});

describe('Variable support', () => {
  it('has Loki variable support', () => {
    const ds = createLokiDatasource(templateSrvStub);

    expect(ds.variables).toBeInstanceOf(LokiVariableSupport);
  });
});

describe('showContextToggle()', () => {
  it('always displays logs context', () => {
    const ds = createLokiDatasource(templateSrvStub);

    expect(ds.showContextToggle()).toBe(true);
  });
});

describe('queryHasFilter()', () => {
  let ds: LokiDatasource;
  beforeEach(() => {
    ds = createLokiDatasource(templateSrvStub);
  });
  it('inspects queries for filter presence', () => {
    const query = { refId: 'A', expr: '{grafana="awesome"}' };
    expect(
      ds.queryHasFilter(query, {
        key: 'grafana',
        value: 'awesome',
      })
    ).toBe(true);
  });
});

function assertAdHocFilters(query: string, expectedResults: string, ds: LokiDatasource) {
  const lokiQuery: LokiQuery = { refId: 'A', expr: query };
  const result = ds.addAdHocFilters(lokiQuery.expr);

  expect(result).toEqual(expectedResults);
}

function makeAnnotationQueryRequest(options = {}): AnnotationQueryRequest<LokiQuery> {
  const timeRange = {
    from: dateTime(),
    to: dateTime(),
  };
  return {
    annotation: {
      expr: '{test=test}',
      refId: '',
      datasource: {
        type: 'loki',
      },
      enable: true,
      name: 'test-annotation',
      iconColor: 'red',
      ...options,
    },
    dashboard: {
      id: 1,
    },
    range: {
      ...timeRange,
      raw: timeRange,
    },
    rangeRaw: timeRange,
  };
}
