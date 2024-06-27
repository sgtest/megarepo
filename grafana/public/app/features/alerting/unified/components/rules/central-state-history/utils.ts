import { groupBy } from 'lodash';

import { DataFrame, Field as DataFrameField, DataFrameJSON, Field, FieldType } from '@grafana/data';
import { fieldIndexComparer } from '@grafana/data/src/field/fieldComparers';

import { labelsMatchMatchers, parseMatchers } from '../../../utils/alertmanager';
import { LogRecord } from '../state-history/common';
import { isLine, isNumbers } from '../state-history/useRuleHistoryRecords';

import { LABELS_FILTER } from './CentralAlertHistoryScene';

const GROUPING_INTERVAL = 10 * 1000; // 10 seconds
const QUERY_PARAM_PREFIX = 'var-'; // Prefix used by Grafana to sync variables in the URL
/*
 * This function is used to convert the history response to a DataFrame list and filter the data by labels.
 * The response is a list of log records, each log record has a timestamp and a line.
 * We group all records by alert instance (unique set of labels) and create a DataFrame for each group (instance).
 * This allows us to be able to filter by labels in the groupDataFramesByTime function.
 */
export function historyResultToDataFrame(data: DataFrameJSON): DataFrame[] {
  const tsValues = data?.data?.values[0] ?? [];
  const timestamps: number[] = isNumbers(tsValues) ? tsValues : [];
  const lines = data?.data?.values[1] ?? [];

  const logRecords = timestamps.reduce((acc: LogRecord[], timestamp: number, index: number) => {
    const line = lines[index];
    // values property can be undefined for some instance states (e.g. NoData)
    if (isLine(line)) {
      acc.push({ timestamp, line });
    }
    return acc;
  }, []);

  // Group log records by alert instance
  const logRecordsByInstance = groupBy(logRecords, (record: LogRecord) => {
    return JSON.stringify(record.line.labels);
  });

  // Convert each group of log records to a DataFrame
  const dataFrames: DataFrame[] = Object.entries(logRecordsByInstance).map<DataFrame>(([key, records]) => {
    // key is the stringified labels
    return logRecordsToDataFrame(key, records);
  });

  // Group DataFrames by time and filter by labels
  return groupDataFramesByTimeAndFilterByLabels(dataFrames);
}

// Scenes sync variables in the URL adding a prefix to the variable name.
function getFilterInQueryParams() {
  const queryParams = new URLSearchParams(window.location.search);
  return queryParams.get(`${QUERY_PARAM_PREFIX}${LABELS_FILTER}`) ?? '';
}

/*
 * This function groups the data frames by time and filters them by labels.
 * The interval is set to 10 seconds.
 * */
function groupDataFramesByTimeAndFilterByLabels(dataFrames: DataFrame[]): DataFrame[] {
  // Filter data frames by labels. This is used to filter out the data frames that do not match the query.
  const filterValue = getFilterInQueryParams();
  const dataframesFiltered = dataFrames.filter((frame) => {
    const labels = JSON.parse(frame.name ?? ''); // in name we store the labels stringified
    const matchers = Boolean(filterValue) ? parseMatchers(filterValue) : [];
    return labelsMatchMatchers(labels, matchers);
  });
  // Extract time fields from filtered data frames
  const timeFieldList = dataframesFiltered.flatMap((frame) => frame.fields.find((field) => field.name === 'time'));

  // Group time fields by interval
  const groupedTimeFields = groupBy(
    timeFieldList?.flatMap((tf) => tf?.values),
    (time: number) => Math.floor(time / GROUPING_INTERVAL) * GROUPING_INTERVAL
  );

  // Create new time field with grouped time values
  const newTimeField: Field = {
    name: 'time',
    type: FieldType.time,
    values: Object.keys(groupedTimeFields).map(Number),
    config: { displayName: 'Time', custom: { fillOpacity: 100 } },
  };

  // Create count field with count of records in each group
  const countField: Field = {
    name: 'value',
    type: FieldType.number,
    values: Object.values(groupedTimeFields).map((group) => group.length),
    config: {},
  };

  // Return new DataFrame with time and count fields
  return [
    {
      fields: [newTimeField, countField],
      length: newTimeField.values.length,
    },
  ];
}

/*
 * This function is used to convert the log records to a DataFrame.
 * The DataFrame has two fields: time and value.
 * The time field is the timestamp of the log record.
 * The value field is always 1.
 * */
function logRecordsToDataFrame(instanceLabels: string, records: LogRecord[]): DataFrame {
  const timeField: DataFrameField = {
    name: 'time',
    type: FieldType.time,
    values: [...records.map((record) => record.timestamp)],
    config: { displayName: 'Time', custom: { fillOpacity: 100 } },
  };

  // Sort time field values
  const timeIndex = timeField.values.map((_, index) => index);
  timeIndex.sort(fieldIndexComparer(timeField));

  // Create DataFrame with time and value fields
  const frame: DataFrame = {
    fields: [
      {
        ...timeField,
        values: timeField.values.map((_, i) => timeField.values[timeIndex[i]]),
      },
      {
        name: instanceLabels,
        type: FieldType.number,
        values: timeField.values.map((record) => 1),
        config: {},
      },
    ],
    length: timeField.values.length,
    name: instanceLabels,
  };

  return frame;
}
