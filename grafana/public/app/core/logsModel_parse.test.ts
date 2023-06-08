import { DataFrame, FieldType, dateTimeFormatISO, DateTimeInput, DateTimeOptions } from '@grafana/data';

import { logSeriesToLogsModel } from './logsModel';

jest.mock('@grafana/data', () => ({
  ...jest.requireActual('@grafana/data'),
  // this produces relative time, so the test-results would keep changing,
  // so we have to mock it
  dateTimeFormatTimeAgo: (p1: DateTimeInput, p2?: DateTimeOptions) =>
    `mock:dateTimeFormatTimeAgo:${dateTimeFormatISO(p1, p2)}`,
}));

describe('logSeriesToLogsModel should parse different logs-dataframe formats', () => {
  it('should parse old Loki-style (grafana8.x) frames ( multi-frame )', () => {
    const frames: DataFrame[] = [
      {
        meta: {},
        refId: 'A',
        fields: [
          {
            name: 'ts',
            type: FieldType.time,
            config: { displayName: 'Time' },
            values: ['2023-06-07T12:18:36.839Z'],
          },
          {
            name: 'line',
            type: FieldType.string,
            config: {},
            values: ['line1'],
            labels: {
              counter: '34543',
              label: 'val3',
              level: 'info',
            },
          },
          {
            name: 'id',
            type: FieldType.string,
            config: {},
            values: ['id1'],
          },
          {
            name: 'tsNs',
            type: FieldType.time,
            config: { displayName: 'Time ns' },
            values: ['1686140316839544212'],
          },
        ],
        length: 1,
      },
      {
        meta: {},
        refId: 'A',
        fields: [
          {
            name: 'ts',
            type: FieldType.time,
            config: { displayName: 'Time' },
            values: ['2023-06-07T12:18:34.632Z'],
          },
          {
            name: 'line',
            type: FieldType.string,
            config: {},
            values: ['line2'],
            labels: {
              counter: '34540',
              label: 'val3',
              level: 'error',
            },
          },
          {
            name: 'id',
            type: FieldType.string,
            config: {},
            values: ['id2'],
          },
          {
            name: 'tsNs',
            type: FieldType.time,
            config: { displayName: 'Time ns' },
            values: ['1686140314632163066'],
          },
        ],
        length: 1,
      },
      {
        meta: {},
        refId: 'A',
        fields: [
          {
            name: 'ts',
            type: FieldType.time,
            config: { displayName: 'Time' },
            values: ['2023-06-07T12:18:35.565Z'],
          },
          {
            name: 'line',
            type: FieldType.string,
            config: {},
            values: ['line3'],
            labels: {
              counter: '34541',
              label: 'val3',
              level: 'error',
            },
          },
          {
            name: 'id',
            type: FieldType.string,
            config: {},
            values: ['id3'],
          },
          {
            name: 'tsNs',
            type: FieldType.time,
            config: { displayName: 'Time ns' },
            values: ['1686140315565682856'],
          },
        ],
        length: 1,
      },
    ];

    expect(logSeriesToLogsModel(frames)).toMatchInlineSnapshot(`
      {
        "hasUniqueLabels": true,
        "meta": [
          {
            "kind": 2,
            "label": "Common labels",
            "value": {
              "label": "val3",
            },
          },
        ],
        "rows": [
          {
            "dataFrame": {
              "fields": [
                {
                  "config": {
                    "displayName": "Time",
                  },
                  "name": "ts",
                  "type": "time",
                  "values": [
                    "2023-06-07T12:18:36.839Z",
                  ],
                },
                {
                  "config": {},
                  "labels": {
                    "counter": "34543",
                    "label": "val3",
                    "level": "info",
                  },
                  "name": "line",
                  "type": "string",
                  "values": [
                    "line1",
                  ],
                },
                {
                  "config": {},
                  "name": "id",
                  "type": "string",
                  "values": [
                    "id1",
                  ],
                },
                {
                  "config": {
                    "displayName": "Time ns",
                  },
                  "name": "tsNs",
                  "type": "time",
                  "values": [
                    "1686140316839544212",
                  ],
                },
              ],
              "length": 1,
              "meta": {},
              "refId": "A",
            },
            "datasourceType": undefined,
            "entry": "line1",
            "entryFieldIndex": 1,
            "hasAnsi": false,
            "hasUnescapedContent": false,
            "labels": {
              "counter": "34543",
              "label": "val3",
              "level": "info",
            },
            "logLevel": "info",
            "raw": "line1",
            "rowIndex": 0,
            "searchWords": [],
            "timeEpochMs": 1686140316839,
            "timeEpochNs": "1686140316839544212",
            "timeFromNow": "mock:dateTimeFormatTimeAgo:2023-06-07T06:18:36-06:00",
            "timeLocal": "2023-06-07 06:18:36",
            "timeUtc": "2023-06-07 12:18:36",
            "uid": "A_id1",
            "uniqueLabels": {
              "counter": "34543",
              "level": "info",
            },
          },
          {
            "dataFrame": {
              "fields": [
                {
                  "config": {
                    "displayName": "Time",
                  },
                  "name": "ts",
                  "type": "time",
                  "values": [
                    "2023-06-07T12:18:34.632Z",
                  ],
                },
                {
                  "config": {},
                  "labels": {
                    "counter": "34540",
                    "label": "val3",
                    "level": "error",
                  },
                  "name": "line",
                  "type": "string",
                  "values": [
                    "line2",
                  ],
                },
                {
                  "config": {},
                  "name": "id",
                  "type": "string",
                  "values": [
                    "id2",
                  ],
                },
                {
                  "config": {
                    "displayName": "Time ns",
                  },
                  "name": "tsNs",
                  "type": "time",
                  "values": [
                    "1686140314632163066",
                  ],
                },
              ],
              "length": 1,
              "meta": {},
              "refId": "A",
            },
            "datasourceType": undefined,
            "entry": "line2",
            "entryFieldIndex": 1,
            "hasAnsi": false,
            "hasUnescapedContent": false,
            "labels": {
              "counter": "34540",
              "label": "val3",
              "level": "error",
            },
            "logLevel": "error",
            "raw": "line2",
            "rowIndex": 0,
            "searchWords": [],
            "timeEpochMs": 1686140314632,
            "timeEpochNs": "1686140314632163066",
            "timeFromNow": "mock:dateTimeFormatTimeAgo:2023-06-07T06:18:34-06:00",
            "timeLocal": "2023-06-07 06:18:34",
            "timeUtc": "2023-06-07 12:18:34",
            "uid": "A_id2",
            "uniqueLabels": {
              "counter": "34540",
              "level": "error",
            },
          },
          {
            "dataFrame": {
              "fields": [
                {
                  "config": {
                    "displayName": "Time",
                  },
                  "name": "ts",
                  "type": "time",
                  "values": [
                    "2023-06-07T12:18:35.565Z",
                  ],
                },
                {
                  "config": {},
                  "labels": {
                    "counter": "34541",
                    "label": "val3",
                    "level": "error",
                  },
                  "name": "line",
                  "type": "string",
                  "values": [
                    "line3",
                  ],
                },
                {
                  "config": {},
                  "name": "id",
                  "type": "string",
                  "values": [
                    "id3",
                  ],
                },
                {
                  "config": {
                    "displayName": "Time ns",
                  },
                  "name": "tsNs",
                  "type": "time",
                  "values": [
                    "1686140315565682856",
                  ],
                },
              ],
              "length": 1,
              "meta": {},
              "refId": "A",
            },
            "datasourceType": undefined,
            "entry": "line3",
            "entryFieldIndex": 1,
            "hasAnsi": false,
            "hasUnescapedContent": false,
            "labels": {
              "counter": "34541",
              "label": "val3",
              "level": "error",
            },
            "logLevel": "error",
            "raw": "line3",
            "rowIndex": 0,
            "searchWords": [],
            "timeEpochMs": 1686140315565,
            "timeEpochNs": "1686140315565682856",
            "timeFromNow": "mock:dateTimeFormatTimeAgo:2023-06-07T06:18:35-06:00",
            "timeLocal": "2023-06-07 06:18:35",
            "timeUtc": "2023-06-07 12:18:35",
            "uid": "A_id3",
            "uniqueLabels": {
              "counter": "34541",
              "level": "error",
            },
          },
        ],
      }
    `);
  });

  it('should parse a Loki-style frame (single-frame, labels-in-json)', () => {
    const frames: DataFrame[] = [
      {
        refId: 'A',
        fields: [
          {
            name: 'labels',
            type: FieldType.other,
            config: {},
            values: [
              {
                counter: '38141',
                label: 'val2',
                level: 'warning',
              },
              {
                counter: '38143',
                label: 'val2',
                level: 'info',
              },
              {
                counter: '38142',
                label: 'val3',
                level: 'info',
              },
            ],
          },
          {
            name: 'Time',
            type: FieldType.time,
            config: {},
            values: [1686142519756, 1686142520411, 1686142519997],
            nanos: [641000, 0, 0],
          },
          {
            name: 'Line',
            type: FieldType.string,
            config: {},
            values: ['line1', 'line2', 'line3'],
          },
          {
            name: 'tsNs',
            type: FieldType.string,
            config: {},
            values: ['1686142519756641000', '1686142520411000000', '1686142519997000000'],
          },
          {
            name: 'id',
            type: FieldType.string,
            config: {},
            values: ['id1', 'id2', 'id3'],
          },
        ],
        length: 3,
        meta: {
          custom: {
            frameType: 'LabeledTimeValues',
          },
        },
      },
    ];

    expect(logSeriesToLogsModel(frames)).toMatchInlineSnapshot(`
      {
        "hasUniqueLabels": true,
        "meta": [],
        "rows": [
          {
            "dataFrame": {
              "fields": [
                {
                  "config": {},
                  "name": "labels",
                  "type": "other",
                  "values": [
                    {
                      "counter": "38141",
                      "label": "val2",
                      "level": "warning",
                    },
                    {
                      "counter": "38143",
                      "label": "val2",
                      "level": "info",
                    },
                    {
                      "counter": "38142",
                      "label": "val3",
                      "level": "info",
                    },
                  ],
                },
                {
                  "config": {},
                  "name": "Time",
                  "nanos": [
                    641000,
                    0,
                    0,
                  ],
                  "type": "time",
                  "values": [
                    1686142519756,
                    1686142520411,
                    1686142519997,
                  ],
                },
                {
                  "config": {},
                  "name": "Line",
                  "type": "string",
                  "values": [
                    "line1",
                    "line2",
                    "line3",
                  ],
                },
                {
                  "config": {},
                  "name": "tsNs",
                  "type": "string",
                  "values": [
                    "1686142519756641000",
                    "1686142520411000000",
                    "1686142519997000000",
                  ],
                },
                {
                  "config": {},
                  "name": "id",
                  "type": "string",
                  "values": [
                    "id1",
                    "id2",
                    "id3",
                  ],
                },
              ],
              "length": 3,
              "meta": {
                "custom": {
                  "frameType": "LabeledTimeValues",
                },
              },
              "refId": "A",
            },
            "datasourceType": undefined,
            "entry": "line1",
            "entryFieldIndex": 2,
            "hasAnsi": false,
            "hasUnescapedContent": false,
            "labels": {
              "counter": "38141",
              "label": "val2",
              "level": "warning",
            },
            "logLevel": "warning",
            "raw": "line1",
            "rowIndex": 0,
            "searchWords": [],
            "timeEpochMs": 1686142519756,
            "timeEpochNs": "1686142519756641000",
            "timeFromNow": "mock:dateTimeFormatTimeAgo:2023-06-07T06:55:19-06:00",
            "timeLocal": "2023-06-07 06:55:19",
            "timeUtc": "2023-06-07 12:55:19",
            "uid": "A_id1",
            "uniqueLabels": {
              "counter": "38141",
              "label": "val2",
              "level": "warning",
            },
          },
          {
            "dataFrame": {
              "fields": [
                {
                  "config": {},
                  "name": "labels",
                  "type": "other",
                  "values": [
                    {
                      "counter": "38141",
                      "label": "val2",
                      "level": "warning",
                    },
                    {
                      "counter": "38143",
                      "label": "val2",
                      "level": "info",
                    },
                    {
                      "counter": "38142",
                      "label": "val3",
                      "level": "info",
                    },
                  ],
                },
                {
                  "config": {},
                  "name": "Time",
                  "nanos": [
                    641000,
                    0,
                    0,
                  ],
                  "type": "time",
                  "values": [
                    1686142519756,
                    1686142520411,
                    1686142519997,
                  ],
                },
                {
                  "config": {},
                  "name": "Line",
                  "type": "string",
                  "values": [
                    "line1",
                    "line2",
                    "line3",
                  ],
                },
                {
                  "config": {},
                  "name": "tsNs",
                  "type": "string",
                  "values": [
                    "1686142519756641000",
                    "1686142520411000000",
                    "1686142519997000000",
                  ],
                },
                {
                  "config": {},
                  "name": "id",
                  "type": "string",
                  "values": [
                    "id1",
                    "id2",
                    "id3",
                  ],
                },
              ],
              "length": 3,
              "meta": {
                "custom": {
                  "frameType": "LabeledTimeValues",
                },
              },
              "refId": "A",
            },
            "datasourceType": undefined,
            "entry": "line2",
            "entryFieldIndex": 2,
            "hasAnsi": false,
            "hasUnescapedContent": false,
            "labels": {
              "counter": "38143",
              "label": "val2",
              "level": "info",
            },
            "logLevel": "info",
            "raw": "line2",
            "rowIndex": 1,
            "searchWords": [],
            "timeEpochMs": 1686142520411,
            "timeEpochNs": "1686142520411000000",
            "timeFromNow": "mock:dateTimeFormatTimeAgo:2023-06-07T06:55:20-06:00",
            "timeLocal": "2023-06-07 06:55:20",
            "timeUtc": "2023-06-07 12:55:20",
            "uid": "A_id2",
            "uniqueLabels": {
              "counter": "38143",
              "label": "val2",
              "level": "info",
            },
          },
          {
            "dataFrame": {
              "fields": [
                {
                  "config": {},
                  "name": "labels",
                  "type": "other",
                  "values": [
                    {
                      "counter": "38141",
                      "label": "val2",
                      "level": "warning",
                    },
                    {
                      "counter": "38143",
                      "label": "val2",
                      "level": "info",
                    },
                    {
                      "counter": "38142",
                      "label": "val3",
                      "level": "info",
                    },
                  ],
                },
                {
                  "config": {},
                  "name": "Time",
                  "nanos": [
                    641000,
                    0,
                    0,
                  ],
                  "type": "time",
                  "values": [
                    1686142519756,
                    1686142520411,
                    1686142519997,
                  ],
                },
                {
                  "config": {},
                  "name": "Line",
                  "type": "string",
                  "values": [
                    "line1",
                    "line2",
                    "line3",
                  ],
                },
                {
                  "config": {},
                  "name": "tsNs",
                  "type": "string",
                  "values": [
                    "1686142519756641000",
                    "1686142520411000000",
                    "1686142519997000000",
                  ],
                },
                {
                  "config": {},
                  "name": "id",
                  "type": "string",
                  "values": [
                    "id1",
                    "id2",
                    "id3",
                  ],
                },
              ],
              "length": 3,
              "meta": {
                "custom": {
                  "frameType": "LabeledTimeValues",
                },
              },
              "refId": "A",
            },
            "datasourceType": undefined,
            "entry": "line3",
            "entryFieldIndex": 2,
            "hasAnsi": false,
            "hasUnescapedContent": false,
            "labels": {
              "counter": "38142",
              "label": "val3",
              "level": "info",
            },
            "logLevel": "info",
            "raw": "line3",
            "rowIndex": 2,
            "searchWords": [],
            "timeEpochMs": 1686142519997,
            "timeEpochNs": "1686142519997000000",
            "timeFromNow": "mock:dateTimeFormatTimeAgo:2023-06-07T06:55:19-06:00",
            "timeLocal": "2023-06-07 06:55:19",
            "timeUtc": "2023-06-07 12:55:19",
            "uid": "A_id3",
            "uniqueLabels": {
              "counter": "38142",
              "label": "val3",
              "level": "info",
            },
          },
        ],
      }
    `);
  });

  it('should parse an Elasticsearch-style frame', () => {
    const frames: DataFrame[] = [
      {
        refId: 'A',
        meta: {},
        fields: [
          {
            name: '@timestamp',
            type: FieldType.time,
            config: {},
            values: [1686143280325, 1686143279324, 1686143278324],
          },
          {
            name: 'line',
            type: FieldType.string,
            config: {},
            values: ['line1', 'line2', 'line3'],
          },
          {
            name: '_id',
            type: FieldType.string,
            config: {},
            values: ['id1', 'id2', 'id3'],
          },
          {
            name: '_index',
            type: FieldType.string,
            config: {},
            values: ['logs-2023.06.07', 'logs-2023.06.07', 'logs-2023.06.07'],
          },
          {
            name: '_source',
            type: FieldType.other,
            config: {},
            values: [
              {
                '@timestamp': '2023-06-07T13:08:00.325Z',
                counter: '300',
                label: 'val2',
                level: 'info',
                line: 'line1',
                shapes: [{ type: 'triangle' }, { type: 'triangle' }, { type: 'triangle' }, { type: 'square' }],
              },
              {
                '@timestamp': '2023-06-07T13:07:59.324Z',
                counter: '299',
                label: 'val1',
                level: 'error',
                line: 'line2',
                shapes: [{ type: 'triangle' }, { type: 'triangle' }, { type: 'triangle' }, { type: 'square' }],
              },
              {
                '@timestamp': '2023-06-07T13:07:58.324Z',
                counter: '298',
                label: 'val2',
                level: 'error',
                line: 'line3',
                shapes: [{ type: 'triangle' }, { type: 'triangle' }, { type: 'triangle' }, { type: 'square' }],
              },
            ],
          },
          {
            name: 'counter',
            type: FieldType.string,
            config: {},
            values: ['300', '299', '298'],
          },
          {
            name: 'label',
            type: FieldType.string,
            config: {},
            values: ['val2', 'val1', 'val2'],
          },
          {
            name: 'level',
            type: FieldType.string,
            config: {},
            values: ['info', 'error', 'error'],
          },
          {
            name: 'shapes',
            type: FieldType.other,
            config: {},
            values: [
              [{ type: 'triangle' }, { type: 'triangle' }, { type: 'triangle' }, { type: 'square' }],
              [{ type: 'triangle' }, { type: 'triangle' }, { type: 'triangle' }, { type: 'square' }],
              [{ type: 'triangle' }, { type: 'triangle' }, { type: 'triangle' }, { type: 'square' }],
            ],
          },
        ],
        length: 3,
      },
    ];

    expect(logSeriesToLogsModel(frames)).toMatchInlineSnapshot(`
      {
        "hasUniqueLabels": false,
        "meta": [],
        "rows": [
          {
            "dataFrame": {
              "fields": [
                {
                  "config": {},
                  "name": "@timestamp",
                  "type": "time",
                  "values": [
                    1686143280325,
                    1686143279324,
                    1686143278324,
                  ],
                },
                {
                  "config": {},
                  "name": "line",
                  "type": "string",
                  "values": [
                    "line1",
                    "line2",
                    "line3",
                  ],
                },
                {
                  "config": {},
                  "name": "_id",
                  "type": "string",
                  "values": [
                    "id1",
                    "id2",
                    "id3",
                  ],
                },
                {
                  "config": {},
                  "name": "_index",
                  "type": "string",
                  "values": [
                    "logs-2023.06.07",
                    "logs-2023.06.07",
                    "logs-2023.06.07",
                  ],
                },
                {
                  "config": {},
                  "name": "_source",
                  "type": "other",
                  "values": [
                    {
                      "@timestamp": "2023-06-07T13:08:00.325Z",
                      "counter": "300",
                      "label": "val2",
                      "level": "info",
                      "line": "line1",
                      "shapes": [
                        {
                          "type": "triangle",
                        },
                        {
                          "type": "triangle",
                        },
                        {
                          "type": "triangle",
                        },
                        {
                          "type": "square",
                        },
                      ],
                    },
                    {
                      "@timestamp": "2023-06-07T13:07:59.324Z",
                      "counter": "299",
                      "label": "val1",
                      "level": "error",
                      "line": "line2",
                      "shapes": [
                        {
                          "type": "triangle",
                        },
                        {
                          "type": "triangle",
                        },
                        {
                          "type": "triangle",
                        },
                        {
                          "type": "square",
                        },
                      ],
                    },
                    {
                      "@timestamp": "2023-06-07T13:07:58.324Z",
                      "counter": "298",
                      "label": "val2",
                      "level": "error",
                      "line": "line3",
                      "shapes": [
                        {
                          "type": "triangle",
                        },
                        {
                          "type": "triangle",
                        },
                        {
                          "type": "triangle",
                        },
                        {
                          "type": "square",
                        },
                      ],
                    },
                  ],
                },
                {
                  "config": {},
                  "name": "counter",
                  "type": "string",
                  "values": [
                    "300",
                    "299",
                    "298",
                  ],
                },
                {
                  "config": {},
                  "name": "label",
                  "type": "string",
                  "values": [
                    "val2",
                    "val1",
                    "val2",
                  ],
                },
                {
                  "config": {},
                  "name": "level",
                  "type": "string",
                  "values": [
                    "info",
                    "error",
                    "error",
                  ],
                },
                {
                  "config": {},
                  "name": "shapes",
                  "type": "other",
                  "values": [
                    [
                      {
                        "type": "triangle",
                      },
                      {
                        "type": "triangle",
                      },
                      {
                        "type": "triangle",
                      },
                      {
                        "type": "square",
                      },
                    ],
                    [
                      {
                        "type": "triangle",
                      },
                      {
                        "type": "triangle",
                      },
                      {
                        "type": "triangle",
                      },
                      {
                        "type": "square",
                      },
                    ],
                    [
                      {
                        "type": "triangle",
                      },
                      {
                        "type": "triangle",
                      },
                      {
                        "type": "triangle",
                      },
                      {
                        "type": "square",
                      },
                    ],
                  ],
                },
              ],
              "length": 3,
              "meta": {},
              "refId": "A",
            },
            "datasourceType": undefined,
            "entry": "line1",
            "entryFieldIndex": 1,
            "hasAnsi": false,
            "hasUnescapedContent": false,
            "labels": {},
            "logLevel": "info",
            "raw": "line1",
            "rowIndex": 0,
            "searchWords": [],
            "timeEpochMs": 1686143280325,
            "timeEpochNs": "1686143280325000000",
            "timeFromNow": "mock:dateTimeFormatTimeAgo:2023-06-07T07:08:00-06:00",
            "timeLocal": "2023-06-07 07:08:00",
            "timeUtc": "2023-06-07 13:08:00",
            "uid": "A_0",
            "uniqueLabels": {},
          },
          {
            "dataFrame": {
              "fields": [
                {
                  "config": {},
                  "name": "@timestamp",
                  "type": "time",
                  "values": [
                    1686143280325,
                    1686143279324,
                    1686143278324,
                  ],
                },
                {
                  "config": {},
                  "name": "line",
                  "type": "string",
                  "values": [
                    "line1",
                    "line2",
                    "line3",
                  ],
                },
                {
                  "config": {},
                  "name": "_id",
                  "type": "string",
                  "values": [
                    "id1",
                    "id2",
                    "id3",
                  ],
                },
                {
                  "config": {},
                  "name": "_index",
                  "type": "string",
                  "values": [
                    "logs-2023.06.07",
                    "logs-2023.06.07",
                    "logs-2023.06.07",
                  ],
                },
                {
                  "config": {},
                  "name": "_source",
                  "type": "other",
                  "values": [
                    {
                      "@timestamp": "2023-06-07T13:08:00.325Z",
                      "counter": "300",
                      "label": "val2",
                      "level": "info",
                      "line": "line1",
                      "shapes": [
                        {
                          "type": "triangle",
                        },
                        {
                          "type": "triangle",
                        },
                        {
                          "type": "triangle",
                        },
                        {
                          "type": "square",
                        },
                      ],
                    },
                    {
                      "@timestamp": "2023-06-07T13:07:59.324Z",
                      "counter": "299",
                      "label": "val1",
                      "level": "error",
                      "line": "line2",
                      "shapes": [
                        {
                          "type": "triangle",
                        },
                        {
                          "type": "triangle",
                        },
                        {
                          "type": "triangle",
                        },
                        {
                          "type": "square",
                        },
                      ],
                    },
                    {
                      "@timestamp": "2023-06-07T13:07:58.324Z",
                      "counter": "298",
                      "label": "val2",
                      "level": "error",
                      "line": "line3",
                      "shapes": [
                        {
                          "type": "triangle",
                        },
                        {
                          "type": "triangle",
                        },
                        {
                          "type": "triangle",
                        },
                        {
                          "type": "square",
                        },
                      ],
                    },
                  ],
                },
                {
                  "config": {},
                  "name": "counter",
                  "type": "string",
                  "values": [
                    "300",
                    "299",
                    "298",
                  ],
                },
                {
                  "config": {},
                  "name": "label",
                  "type": "string",
                  "values": [
                    "val2",
                    "val1",
                    "val2",
                  ],
                },
                {
                  "config": {},
                  "name": "level",
                  "type": "string",
                  "values": [
                    "info",
                    "error",
                    "error",
                  ],
                },
                {
                  "config": {},
                  "name": "shapes",
                  "type": "other",
                  "values": [
                    [
                      {
                        "type": "triangle",
                      },
                      {
                        "type": "triangle",
                      },
                      {
                        "type": "triangle",
                      },
                      {
                        "type": "square",
                      },
                    ],
                    [
                      {
                        "type": "triangle",
                      },
                      {
                        "type": "triangle",
                      },
                      {
                        "type": "triangle",
                      },
                      {
                        "type": "square",
                      },
                    ],
                    [
                      {
                        "type": "triangle",
                      },
                      {
                        "type": "triangle",
                      },
                      {
                        "type": "triangle",
                      },
                      {
                        "type": "square",
                      },
                    ],
                  ],
                },
              ],
              "length": 3,
              "meta": {},
              "refId": "A",
            },
            "datasourceType": undefined,
            "entry": "line2",
            "entryFieldIndex": 1,
            "hasAnsi": false,
            "hasUnescapedContent": false,
            "labels": {},
            "logLevel": "error",
            "raw": "line2",
            "rowIndex": 1,
            "searchWords": [],
            "timeEpochMs": 1686143279324,
            "timeEpochNs": "1686143279324000000",
            "timeFromNow": "mock:dateTimeFormatTimeAgo:2023-06-07T07:07:59-06:00",
            "timeLocal": "2023-06-07 07:07:59",
            "timeUtc": "2023-06-07 13:07:59",
            "uid": "A_1",
            "uniqueLabels": {},
          },
          {
            "dataFrame": {
              "fields": [
                {
                  "config": {},
                  "name": "@timestamp",
                  "type": "time",
                  "values": [
                    1686143280325,
                    1686143279324,
                    1686143278324,
                  ],
                },
                {
                  "config": {},
                  "name": "line",
                  "type": "string",
                  "values": [
                    "line1",
                    "line2",
                    "line3",
                  ],
                },
                {
                  "config": {},
                  "name": "_id",
                  "type": "string",
                  "values": [
                    "id1",
                    "id2",
                    "id3",
                  ],
                },
                {
                  "config": {},
                  "name": "_index",
                  "type": "string",
                  "values": [
                    "logs-2023.06.07",
                    "logs-2023.06.07",
                    "logs-2023.06.07",
                  ],
                },
                {
                  "config": {},
                  "name": "_source",
                  "type": "other",
                  "values": [
                    {
                      "@timestamp": "2023-06-07T13:08:00.325Z",
                      "counter": "300",
                      "label": "val2",
                      "level": "info",
                      "line": "line1",
                      "shapes": [
                        {
                          "type": "triangle",
                        },
                        {
                          "type": "triangle",
                        },
                        {
                          "type": "triangle",
                        },
                        {
                          "type": "square",
                        },
                      ],
                    },
                    {
                      "@timestamp": "2023-06-07T13:07:59.324Z",
                      "counter": "299",
                      "label": "val1",
                      "level": "error",
                      "line": "line2",
                      "shapes": [
                        {
                          "type": "triangle",
                        },
                        {
                          "type": "triangle",
                        },
                        {
                          "type": "triangle",
                        },
                        {
                          "type": "square",
                        },
                      ],
                    },
                    {
                      "@timestamp": "2023-06-07T13:07:58.324Z",
                      "counter": "298",
                      "label": "val2",
                      "level": "error",
                      "line": "line3",
                      "shapes": [
                        {
                          "type": "triangle",
                        },
                        {
                          "type": "triangle",
                        },
                        {
                          "type": "triangle",
                        },
                        {
                          "type": "square",
                        },
                      ],
                    },
                  ],
                },
                {
                  "config": {},
                  "name": "counter",
                  "type": "string",
                  "values": [
                    "300",
                    "299",
                    "298",
                  ],
                },
                {
                  "config": {},
                  "name": "label",
                  "type": "string",
                  "values": [
                    "val2",
                    "val1",
                    "val2",
                  ],
                },
                {
                  "config": {},
                  "name": "level",
                  "type": "string",
                  "values": [
                    "info",
                    "error",
                    "error",
                  ],
                },
                {
                  "config": {},
                  "name": "shapes",
                  "type": "other",
                  "values": [
                    [
                      {
                        "type": "triangle",
                      },
                      {
                        "type": "triangle",
                      },
                      {
                        "type": "triangle",
                      },
                      {
                        "type": "square",
                      },
                    ],
                    [
                      {
                        "type": "triangle",
                      },
                      {
                        "type": "triangle",
                      },
                      {
                        "type": "triangle",
                      },
                      {
                        "type": "square",
                      },
                    ],
                    [
                      {
                        "type": "triangle",
                      },
                      {
                        "type": "triangle",
                      },
                      {
                        "type": "triangle",
                      },
                      {
                        "type": "square",
                      },
                    ],
                  ],
                },
              ],
              "length": 3,
              "meta": {},
              "refId": "A",
            },
            "datasourceType": undefined,
            "entry": "line3",
            "entryFieldIndex": 1,
            "hasAnsi": false,
            "hasUnescapedContent": false,
            "labels": {},
            "logLevel": "error",
            "raw": "line3",
            "rowIndex": 2,
            "searchWords": [],
            "timeEpochMs": 1686143278324,
            "timeEpochNs": "1686143278324000000",
            "timeFromNow": "mock:dateTimeFormatTimeAgo:2023-06-07T07:07:58-06:00",
            "timeLocal": "2023-06-07 07:07:58",
            "timeUtc": "2023-06-07 13:07:58",
            "uid": "A_2",
            "uniqueLabels": {},
          },
        ],
      }
    `);
  });
});
