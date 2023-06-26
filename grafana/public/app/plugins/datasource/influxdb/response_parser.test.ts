import { size } from 'lodash';
import { of } from 'rxjs';

import { AnnotationEvent, DataQueryRequest, dateTime, FieldType, MutableDataFrame } from '@grafana/data';
import { FetchResponse } from '@grafana/runtime';
import config from 'app/core/config';
import { backendSrv } from 'app/core/services/backend_srv'; // will use the version in __mocks__

import InfluxQueryModel from './influx_query_model';
import ResponseParser, { getSelectedParams } from './response_parser';
import { getMockDS, getMockDSInstanceSettings } from './specs/mocks';
import { InfluxQuery } from './types';

jest.mock('@grafana/runtime', () => ({
  ...jest.requireActual('@grafana/runtime'),
  getBackendSrv: () => backendSrv,
}));

describe('influxdb response parser', () => {
  const parser = new ResponseParser();

  describe('SHOW TAG response', () => {
    const query = 'SHOW TAG KEYS FROM "cpu"';
    const response = {
      results: [
        {
          series: [
            {
              name: 'cpu',
              columns: ['tagKey'],
              values: [['datacenter'], ['hostname'], ['source']],
            },
          ],
        },
      ],
    };

    const result = parser.parse(query, response);

    it('expects three results', () => {
      expect(size(result)).toBe(3);
    });
  });

  describe('SHOW TAG VALUES response', () => {
    const query = 'SHOW TAG VALUES FROM "cpu" WITH KEY = "hostname"';

    describe('response from 0.10.0', () => {
      const response = {
        results: [
          {
            series: [
              {
                name: 'hostnameTagValues',
                columns: ['hostname'],
                values: [['server1'], ['server2'], ['server2']],
              },
            ],
          },
        ],
      };

      const result = parser.parse(query, response);

      it('should get two responses', () => {
        expect(size(result)).toBe(2);
        expect(result[0].text).toBe('server1');
        expect(result[1].text).toBe('server2');
      });
    });

    describe('response from 0.12.0', () => {
      const response = {
        results: [
          {
            series: [
              {
                name: 'cpu',
                columns: ['key', 'value'],
                values: [
                  ['source', 'site'],
                  ['source', 'api'],
                ],
              },
              {
                name: 'logins',
                columns: ['key', 'value'],
                values: [
                  ['source', 'site'],
                  ['source', 'webapi'],
                ],
              },
            ],
          },
        ],
      };

      const result = parser.parse(query, response);

      it('should get two responses', () => {
        expect(size(result)).toBe(3);
        expect(result[0].text).toBe('site');
        expect(result[1].text).toBe('api');
        expect(result[2].text).toBe('webapi');
      });
    });
  });

  describe('SELECT response', () => {
    const query = 'SELECT "usage_iowait" FROM "cpu" LIMIT 10';
    const response = {
      results: [
        {
          series: [
            {
              name: 'cpu',
              columns: ['time', 'usage_iowait'],
              values: [
                [1488465190006040638, 0.0],
                [1488465190006040638, 15.0],
                [1488465190006040638, 20.2],
              ],
            },
          ],
        },
      ],
    };

    const result = parser.parse(query, response);

    it('should return second column', () => {
      expect(size(result)).toBe(3);
      expect(result[0].text).toBe('0');
      expect(result[1].text).toBe('15');
      expect(result[2].text).toBe('20.2');
    });
  });

  describe('SELECT response where ordering matters', () => {
    const query = 'SELECT "val" from "num"';
    const response = {
      results: [
        {
          series: [
            {
              name: 'num',
              columns: ['time', 'val'],
              values: [
                [1620041231000, 2],
                [1620041233000, 3],
                [1620041235000, 4],
                [1620041238000, 5],
                [1620041239000, 1],
              ],
            },
          ],
        },
      ],
    };

    it('should keep the order returned by influxdb, even for numbers', () => {
      expect(parser.parse(query, response)).toStrictEqual([
        { text: '2' },
        { text: '3' },
        { text: '4' },
        { text: '5' },
        { text: '1' },
      ]);
    });
  });

  describe('SHOW FIELD response', () => {
    const query = 'SHOW FIELD KEYS FROM "cpu"';

    describe('response from pre-1.0', () => {
      const response = {
        results: [
          {
            series: [
              {
                name: 'cpu',
                columns: ['fieldKey'],
                values: [['value']],
              },
            ],
          },
        ],
      };

      const result = parser.parse(query, response);

      it('should get two responses', () => {
        expect(size(result)).toBe(1);
      });
    });

    describe('response from 1.0', () => {
      const response = {
        results: [
          {
            series: [
              {
                name: 'cpu',
                columns: ['fieldKey', 'fieldType'],
                values: [['time', 'float']],
              },
            ],
          },
        ],
      };

      const result = parser.parse(query, response);

      it('should return first column', () => {
        expect(size(result)).toBe(1);
        expect(result[0].text).toBe('time');
      });
    });
  });

  describe('Should name the selected params correctly', () => {
    it('when there are no duplicates', () => {
      const query = new InfluxQueryModel({
        refId: 'A',
        select: [[{ type: 'field', params: ['usage_iowait'] }], [{ type: 'field', params: ['usage_idle'] }]],
      });

      const selectedParams = getSelectedParams(query.target);

      expect(selectedParams.length).toBe(2);
      expect(selectedParams[0]).toBe('usage_iowait');
      expect(selectedParams[1]).toBe('usage_idle');
    });

    it('when there are duplicates', () => {
      const query = new InfluxQueryModel({
        refId: 'A',
        select: [
          [{ type: 'field', params: ['usage_iowait'] }],
          [{ type: 'field', params: ['usage_iowait'] }],
          [{ type: 'field', params: ['usage_iowait'] }],
          [{ type: 'field', params: ['usage_idle'] }],
        ],
      });

      const selectedParams = getSelectedParams(query.target);

      expect(selectedParams.length).toBe(4);
      expect(selectedParams[0]).toBe('usage_iowait');
      expect(selectedParams[1]).toBe('usage_iowait_1');
      expect(selectedParams[2]).toBe('usage_iowait_2');
      expect(selectedParams[3]).toBe('usage_idle');
    });
  });

  describe('Should get the table', () => {
    const dataFrame = new MutableDataFrame({
      fields: [
        { name: 'time', type: FieldType.time, values: [1640257340000] },
        { name: 'value', type: FieldType.number, values: [3234232323] },
      ],
      meta: {
        executedQueryString: 'SELECT everything!',
      },
    });

    const query = new InfluxQueryModel({
      refId: 'A',
      select: [[{ type: 'field', params: ['usage_iowait'] }], [{ type: 'field', params: ['usage_idle'] }]],
    });

    const table = parser.getTable([dataFrame], query.target, {
      preferredVisualisationType: 'table',
    });

    it('columns correctly', () => {
      expect(table.columns.length).toBe(3);
      expect(table.columns[0].text).toBe('Time');
      expect(table.columns[1].text).toBe('usage_iowait');
      expect(table.columns[2].text).toBe('usage_idle');
    });

    it('rows correctly', () => {
      expect(table.rows.length).toBe(1);
      expect(table.rows[0].length).toBe(2);
      expect(table.rows[0][0]).toBe(1640257340000);
      expect(table.rows[0][1]).toBe(3234232323);
    });

    it('preferredVisualisationType correctly', () => {
      expect(table.meta?.preferredVisualisationType).toBe('table');
    });

    it('executedQueryString correctly', () => {
      expect(table.meta?.executedQueryString).toBe('SELECT everything!');
    });
  });

  describe('When issuing annotationQuery', () => {
    const ctx = {
      ds: getMockDS(getMockDSInstanceSettings()),
    };

    const fetchMock = jest.spyOn(backendSrv, 'fetch');

    const annotation: InfluxQuery = {
      refId: 'A',
      fromAnnotations: true,
      name: 'Anno',
      query: 'select * from logs where time >= now() - 15m and time <= now()',
      textColumn: 'textColumn',
      tagsColumn: 'host,path',
    };

    const queryOptions: DataQueryRequest = {
      app: 'explore',
      interval: '',
      intervalMs: 0,
      requestId: '',
      scopedVars: {},
      startTime: 0,
      timezone: '',
      targets: [annotation],
      range: {
        from: dateTime().subtract(1, 'h'),
        to: dateTime(),
        raw: { from: '1h', to: 'now' },
      },
    };
    let response: AnnotationEvent[];

    beforeEach(async () => {
      const mockResponse: FetchResponse = {
        config: { url: '' },
        headers: new Headers(),
        ok: false,
        redirected: false,
        status: 0,
        statusText: '',
        type: 'basic',
        url: '',
        data: {
          results: {
            metricFindQuery: {
              frames: [
                {
                  schema: {
                    name: 'logs.host',
                    fields: [
                      {
                        name: 'time',
                        type: 'time',
                      },
                      {
                        name: 'value',
                        type: 'string',
                      },
                    ],
                  },
                  data: {
                    values: [
                      [1645208701000, 1645208702000],
                      ['cbfa07e0e3bb 1', 'cbfa07e0e3bb 2'],
                    ],
                  },
                },
                {
                  schema: {
                    name: 'logs.message',
                    fields: [
                      {
                        name: 'time',
                        type: 'time',
                      },
                      {
                        name: 'value',
                        type: 'string',
                      },
                    ],
                  },
                  data: {
                    values: [
                      [1645208701000, 1645208702000],
                      [
                        'Station softwareupdated[447]: Adding client 1',
                        'Station softwareupdated[447]: Adding client 2',
                      ],
                    ],
                  },
                },
                {
                  schema: {
                    name: 'logs.path',
                    fields: [
                      {
                        name: 'time',
                        type: 'time',
                      },
                      {
                        name: 'value',
                        type: 'string',
                      },
                    ],
                  },
                  data: {
                    values: [
                      [1645208701000, 1645208702000],
                      ['/var/log/host/install.log 1', '/var/log/host/install.log 2'],
                    ],
                  },
                },
                {
                  schema: {
                    name: 'textColumn',
                    fields: [
                      {
                        name: 'time',
                        type: 'time',
                      },
                      {
                        name: 'value',
                        type: 'string',
                      },
                    ],
                  },
                  data: {
                    values: [
                      [1645208701000, 1645208702000],
                      ['text 1', 'text 2'],
                    ],
                  },
                },
              ],
            },
          },
        },
      };

      fetchMock.mockImplementation(() => {
        return of(mockResponse);
      });

      config.featureToggles.influxdbBackendMigration = true;
      response = await ctx.ds.annotationEvents(queryOptions, annotation);
    });

    it('should return annotation list', () => {
      expect(response.length).toBe(2);
      expect(response[0].time).toBe(1645208701000);
      expect(response[0].title).toBe('Station softwareupdated[447]: Adding client 1');
      expect(response[0].text).toBe('text 1');
      expect(response[0].tags?.[0]).toBe('cbfa07e0e3bb 1');
      expect(response[0].tags?.[1]).toBe('/var/log/host/install.log 1');
      expect(response[1].time).toBe(1645208702000);
      expect(response[1].title).toBe('Station softwareupdated[447]: Adding client 2');
      expect(response[1].text).toBe('text 2');
      expect(response[1].tags?.[0]).toBe('cbfa07e0e3bb 2');
      expect(response[1].tags?.[1]).toBe('/var/log/host/install.log 2');
    });
  });
});
