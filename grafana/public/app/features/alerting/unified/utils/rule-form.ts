import {
  DataQuery,
  DataSourceInstanceSettings,
  DataSourceRef,
  getDefaultRelativeTimeRange,
  IntervalValues,
  rangeUtil,
  RelativeTimeRange,
  ScopedVars,
  TimeRange,
} from '@grafana/data';
import { getDataSourceSrv } from '@grafana/runtime';
import { ExpressionDatasourceRef } from '@grafana/runtime/src/utils/DataSourceWithBackend';
import { DataSourceJsonData } from '@grafana/schema';
import { getNextRefIdChar } from 'app/core/utils/query';
import { DashboardModel, PanelModel } from 'app/features/dashboard/state';
import { ExpressionDatasourceUID, ExpressionQuery, ExpressionQueryType } from 'app/features/expressions/types';
import { LokiQuery } from 'app/plugins/datasource/loki/types';
import { PromQuery } from 'app/plugins/datasource/prometheus/types';
import { RuleWithLocation } from 'app/types/unified-alerting';
import {
  AlertDataQuery,
  AlertQuery,
  Annotations,
  GrafanaAlertStateDecision,
  Labels,
  PostableRuleGrafanaRuleDTO,
  RulerAlertingRuleDTO,
  RulerRecordingRuleDTO,
  RulerRuleDTO,
} from 'app/types/unified-alerting-dto';

import { EvalFunction } from '../../state/alertDef';
import { RuleFormType, RuleFormValues } from '../types/rule-form';

import { getRulesAccess } from './access-control';
import { Annotation, defaultAnnotations } from './constants';
import { getDefaultOrFirstCompatibleDataSource, isGrafanaRulesSource } from './datasource';
import { arrayToRecord, recordToArray } from './misc';
import { isAlertingRulerRule, isGrafanaRulerRule, isRecordingRulerRule } from './rules';
import { parseInterval } from './time';

export type PromOrLokiQuery = PromQuery | LokiQuery;

export const MINUTE = '1m';

export const getDefaultFormValues = (): RuleFormValues => {
  const { canCreateGrafanaRules, canCreateCloudRules } = getRulesAccess();

  return Object.freeze({
    name: '',
    uid: '',
    labels: [{ key: '', value: '' }],
    annotations: defaultAnnotations,
    dataSourceName: null,
    type: canCreateGrafanaRules ? RuleFormType.grafana : canCreateCloudRules ? RuleFormType.cloudAlerting : undefined, // viewers can't create prom alerts
    group: '',

    // grafana
    folder: null,
    queries: [],
    recordingRulesQueries: [],
    condition: '',
    noDataState: GrafanaAlertStateDecision.NoData,
    execErrState: GrafanaAlertStateDecision.Error,
    evaluateFor: '5m',
    evaluateEvery: MINUTE,

    // cortex / loki
    namespace: '',
    expression: '',
    forTime: 1,
    forTimeUnit: 'm',
  });
};

export function formValuesToRulerRuleDTO(values: RuleFormValues): RulerRuleDTO {
  const { name, expression, forTime, forTimeUnit, type } = values;
  if (type === RuleFormType.cloudAlerting) {
    return {
      alert: name,
      for: `${forTime}${forTimeUnit}`,
      annotations: arrayToRecord(values.annotations || []),
      labels: arrayToRecord(values.labels || []),
      expr: expression,
    };
  } else if (type === RuleFormType.cloudRecording) {
    return {
      record: name,
      labels: arrayToRecord(values.labels || []),
      expr: expression,
    };
  }
  throw new Error(`unexpected rule type: ${type}`);
}

export function listifyLabelsOrAnnotations(
  item: Labels | Annotations | undefined,
  addEmpty: boolean
): Array<{ key: string; value: string }> {
  const list = [...recordToArray(item || {})];
  if (addEmpty) {
    list.push({ key: '', value: '' });
  }
  return list;
}

//make sure default annotations are always shown in order even if empty
export function normalizeDefaultAnnotations(annotations: Array<{ key: string; value: string }>) {
  const orderedAnnotations = [...annotations];
  const defaultAnnotationKeys = defaultAnnotations.map((annotation) => annotation.key);

  defaultAnnotationKeys.forEach((defaultAnnotationKey, index) => {
    const fieldIndex = orderedAnnotations.findIndex((field) => field.key === defaultAnnotationKey);

    if (fieldIndex === -1) {
      //add the default annotation if abstent
      const emptyValue = { key: defaultAnnotationKey, value: '' };
      orderedAnnotations.splice(index, 0, emptyValue);
    } else if (fieldIndex !== index) {
      //move it to the correct position if present
      orderedAnnotations.splice(index, 0, orderedAnnotations.splice(fieldIndex, 1)[0]);
    }
  });
  return orderedAnnotations;
}

export function formValuesToRulerGrafanaRuleDTO(values: RuleFormValues): PostableRuleGrafanaRuleDTO {
  const { name, condition, noDataState, execErrState, evaluateFor, queries, isPaused } = values;
  if (condition) {
    return {
      grafana_alert: {
        title: name,
        condition,
        no_data_state: noDataState,
        exec_err_state: execErrState,
        data: queries.map(fixBothInstantAndRangeQuery),
        is_paused: Boolean(isPaused),
      },
      for: evaluateFor,
      annotations: arrayToRecord(values.annotations || []),
      labels: arrayToRecord(values.labels || []),
    };
  }
  throw new Error('Cannot create rule without specifying alert condition');
}

export function rulerRuleToFormValues(ruleWithLocation: RuleWithLocation): RuleFormValues {
  const { ruleSourceName, namespace, group, rule } = ruleWithLocation;

  const defaultFormValues = getDefaultFormValues();
  if (isGrafanaRulesSource(ruleSourceName)) {
    if (isGrafanaRulerRule(rule)) {
      const ga = rule.grafana_alert;

      return {
        ...defaultFormValues,
        name: ga.title,
        type: RuleFormType.grafana,
        group: group.name,
        evaluateEvery: group.interval || defaultFormValues.evaluateEvery,
        evaluateFor: rule.for || '0',
        noDataState: ga.no_data_state,
        execErrState: ga.exec_err_state,
        queries: ga.data,
        condition: ga.condition,
        annotations: normalizeDefaultAnnotations(listifyLabelsOrAnnotations(rule.annotations, false)),
        labels: listifyLabelsOrAnnotations(rule.labels, true),
        folder: { title: namespace, uid: ga.namespace_uid },
        isPaused: ga.is_paused,
      };
    } else {
      throw new Error('Unexpected type of rule for grafana rules source');
    }
  } else {
    if (isAlertingRulerRule(rule)) {
      const datasourceUid = getDataSourceSrv().getInstanceSettings(ruleSourceName)?.uid ?? '';

      const defaultQuery = {
        refId: 'A',
        datasourceUid,
        queryType: '',
        relativeTimeRange: getDefaultRelativeTimeRange(),
        expr: rule.expr,
        model: {
          refId: 'A',
          hide: false,
          expr: rule.expr,
        },
      };

      const alertingRuleValues = alertingRulerRuleToRuleForm(rule);

      return {
        ...defaultFormValues,
        ...alertingRuleValues,
        queries: [defaultQuery],
        annotations: normalizeDefaultAnnotations(listifyLabelsOrAnnotations(rule.annotations, false)),
        type: RuleFormType.cloudAlerting,
        dataSourceName: ruleSourceName,
        namespace,
        group: group.name,
      };
    } else if (isRecordingRulerRule(rule)) {
      const recordingRuleValues = recordingRulerRuleToRuleForm(rule);

      return {
        ...defaultFormValues,
        ...recordingRuleValues,
        type: RuleFormType.cloudRecording,
        dataSourceName: ruleSourceName,
        namespace,
        group: group.name,
      };
    } else {
      throw new Error('Unexpected type of rule for cloud rules source');
    }
  }
}

export function alertingRulerRuleToRuleForm(
  rule: RulerAlertingRuleDTO
): Pick<RuleFormValues, 'name' | 'forTime' | 'forTimeUnit' | 'expression' | 'annotations' | 'labels'> {
  const defaultFormValues = getDefaultFormValues();

  const [forTime, forTimeUnit] = rule.for
    ? parseInterval(rule.for)
    : [defaultFormValues.forTime, defaultFormValues.forTimeUnit];

  return {
    name: rule.alert,
    expression: rule.expr,
    forTime,
    forTimeUnit,
    annotations: listifyLabelsOrAnnotations(rule.annotations, false),
    labels: listifyLabelsOrAnnotations(rule.labels, true),
  };
}

export function recordingRulerRuleToRuleForm(
  rule: RulerRecordingRuleDTO
): Pick<RuleFormValues, 'name' | 'expression' | 'labels'> {
  return {
    name: rule.record,
    expression: rule.expr,
    labels: listifyLabelsOrAnnotations(rule.labels, true),
  };
}

export const getDefaultQueries = (): AlertQuery[] => {
  const dataSource = getDefaultOrFirstCompatibleDataSource();

  if (!dataSource) {
    return [...getDefaultExpressions('A', 'B')];
  }
  const relativeTimeRange = getDefaultRelativeTimeRange();

  return [
    {
      refId: 'A',
      datasourceUid: dataSource.uid,
      queryType: '',
      relativeTimeRange,
      model: {
        refId: 'A',
      },
    },
    ...getDefaultExpressions('B', 'C'),
  ];
};

export const getDefaultRecordingRulesQueries = (
  rulesSourcesWithRuler: Array<DataSourceInstanceSettings<DataSourceJsonData>>
): AlertQuery[] => {
  const relativeTimeRange = getDefaultRelativeTimeRange();

  return [
    {
      refId: 'A',
      datasourceUid: rulesSourcesWithRuler[0]?.uid || '',
      queryType: '',
      relativeTimeRange,
      model: {
        refId: 'A',
      },
    },
  ];
};

const getDefaultExpressions = (...refIds: [string, string]): AlertQuery[] => {
  const refOne = refIds[0];
  const refTwo = refIds[1];

  const reduceExpression: ExpressionQuery = {
    refId: refIds[0],
    type: ExpressionQueryType.reduce,
    datasource: {
      uid: ExpressionDatasourceUID,
      type: ExpressionDatasourceRef.type,
    },
    conditions: [
      {
        type: 'query',
        evaluator: {
          params: [],
          type: EvalFunction.IsAbove,
        },
        operator: {
          type: 'and',
        },
        query: {
          params: [refOne],
        },
        reducer: {
          params: [],
          type: 'last',
        },
      },
    ],
    reducer: 'last',
    expression: 'A',
  };

  const thresholdExpression: ExpressionQuery = {
    refId: refTwo,
    type: ExpressionQueryType.threshold,
    datasource: {
      uid: ExpressionDatasourceUID,
      type: ExpressionDatasourceRef.type,
    },
    conditions: [
      {
        type: 'query',
        evaluator: {
          params: [0],
          type: EvalFunction.IsAbove,
        },
        operator: {
          type: 'and',
        },
        query: {
          params: [refTwo],
        },
        reducer: {
          params: [],
          type: 'last',
        },
      },
    ],
    expression: refOne,
  };

  return [
    {
      refId: refOne,
      datasourceUid: ExpressionDatasourceUID,
      queryType: '',
      model: reduceExpression,
    },
    {
      refId: refTwo,
      datasourceUid: ExpressionDatasourceUID,
      queryType: '',
      model: thresholdExpression,
    },
  ];
};

const dataQueriesToGrafanaQueries = async (
  queries: DataQuery[],
  relativeTimeRange: RelativeTimeRange,
  scopedVars: ScopedVars | {},
  panelDataSourceRef?: DataSourceRef,
  maxDataPoints?: number,
  minInterval?: string
): Promise<AlertQuery[]> => {
  const result: AlertQuery[] = [];

  for (const target of queries) {
    const datasource = await getDataSourceSrv().get(target.datasource?.uid ? target.datasource : panelDataSourceRef);
    const dsRef = { uid: datasource.uid, type: datasource.type };

    const range = rangeUtil.relativeToTimeRange(relativeTimeRange);
    const { interval, intervalMs } = getIntervals(range, minInterval ?? datasource.interval, maxDataPoints);
    const queryVariables = {
      __interval: { text: interval, value: interval },
      __interval_ms: { text: intervalMs, value: intervalMs },
      ...scopedVars,
    };

    const interpolatedTarget = datasource.interpolateVariablesInQueries
      ? await datasource.interpolateVariablesInQueries([target], queryVariables)[0]
      : target;

    // expressions
    if (dsRef.uid === ExpressionDatasourceUID) {
      const newQuery: AlertQuery = {
        refId: interpolatedTarget.refId,
        queryType: '',
        relativeTimeRange,
        datasourceUid: ExpressionDatasourceUID,
        model: interpolatedTarget,
      };
      result.push(newQuery);
      // queries
    } else {
      const datasourceSettings = getDataSourceSrv().getInstanceSettings(dsRef);
      if (datasourceSettings && datasourceSettings.meta.alerting) {
        const newQuery: AlertQuery = {
          refId: interpolatedTarget.refId,
          queryType: interpolatedTarget.queryType ?? '',
          relativeTimeRange,
          datasourceUid: datasourceSettings.uid,
          model: {
            ...interpolatedTarget,
            maxDataPoints,
            intervalMs,
          },
        };
        result.push(newQuery);
      }
    }
  }
  return result;
};

export const panelToRuleFormValues = async (
  panel: PanelModel,
  dashboard: DashboardModel
): Promise<Partial<RuleFormValues> | undefined> => {
  const { targets } = panel;
  if (!panel.id || !dashboard.uid) {
    return undefined;
  }

  const relativeTimeRange = rangeUtil.timeRangeToRelative(rangeUtil.convertRawToRange(dashboard.time));
  const queries = await dataQueriesToGrafanaQueries(
    targets,
    relativeTimeRange,
    panel.scopedVars || {},
    panel.datasource ?? undefined,
    panel.maxDataPoints ?? undefined,
    panel.interval ?? undefined
  );
  // if no alerting capable queries are found, can't create a rule
  if (!queries.length || !queries.find((query) => query.datasourceUid !== ExpressionDatasourceUID)) {
    return undefined;
  }

  if (!queries.find((query) => query.datasourceUid === ExpressionDatasourceUID)) {
    const [reduceExpression, _thresholdExpression] = getDefaultExpressions(getNextRefIdChar(queries), '-');
    queries.push(reduceExpression);

    const [_reduceExpression, thresholdExpression] = getDefaultExpressions(
      reduceExpression.refId,
      getNextRefIdChar(queries)
    );
    queries.push(thresholdExpression);
  }

  const { folderTitle, folderUid } = dashboard.meta;

  const formValues = {
    type: RuleFormType.grafana,
    folder:
      folderUid && folderTitle
        ? {
            uid: folderUid,
            title: folderTitle,
          }
        : undefined,
    queries,
    name: panel.title,
    condition: queries[queries.length - 1].refId,
    annotations: [
      {
        key: Annotation.dashboardUID,
        value: dashboard.uid,
      },
      {
        key: Annotation.panelID,
        value: String(panel.id),
      },
    ],
  };
  return formValues;
};

export function getIntervals(range: TimeRange, lowLimit?: string, resolution?: number): IntervalValues {
  if (!resolution) {
    if (lowLimit && rangeUtil.intervalToMs(lowLimit) > 1000) {
      return {
        interval: lowLimit,
        intervalMs: rangeUtil.intervalToMs(lowLimit),
      };
    }
    return { interval: '1s', intervalMs: 1000 };
  }

  return rangeUtil.calculateInterval(range, resolution, lowLimit);
}

export function fixBothInstantAndRangeQuery(query: AlertQuery) {
  const model = query.model;

  if (!isPromQuery(model)) {
    return query;
  }

  const isBothInstantAndRange = model.instant && model.range;
  if (isBothInstantAndRange) {
    return { ...query, model: { ...model, range: true, instant: false } };
  }

  return query;
}

function isPromQuery(model: AlertDataQuery): model is PromQuery {
  return 'expr' in model && 'instant' in model && 'range' in model;
}

export function isPromOrLokiQuery(model: AlertDataQuery): model is PromOrLokiQuery {
  return 'expr' in model;
}
