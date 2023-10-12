import { RelativeTimeRange } from '@grafana/data';
import { Matcher } from 'app/plugins/datasource/alertmanager/types';
import { RuleIdentifier, RuleNamespace, RulerDataSourceConfig } from 'app/types/unified-alerting';
import {
  AlertQuery,
  Annotations,
  GrafanaAlertStateDecision,
  Labels,
  PostableRuleGrafanaRuleDTO,
  PromRulesResponse,
  RulerAlertingRuleDTO,
  RulerRecordingRuleDTO,
  RulerRuleGroupDTO,
  RulerRulesConfigDTO,
} from 'app/types/unified-alerting-dto';

import { ExportFormats } from '../components/export/providers';
import { Folder } from '../components/rule-editor/RuleFolderPicker';
import { getDatasourceAPIUid, GRAFANA_RULES_SOURCE_NAME } from '../utils/datasource';
import { arrayKeyValuesToObject } from '../utils/labels';
import { isCloudRuleIdentifier, isPrometheusRuleIdentifier } from '../utils/rules';

import { alertingApi } from './alertingApi';
import {
  FetchPromRulesFilter,
  groupRulesByFileName,
  paramsWithMatcherAndState,
  prepareRulesFilterQueryParams,
} from './prometheus';
import { FetchRulerRulesFilter, rulerUrlBuilder } from './ruler';

export type ResponseLabels = {
  labels: AlertInstances[];
};

export type PreviewResponse = ResponseLabels[];

export interface Datasource {
  type: string;
  uid: string;
}

export const PREVIEW_URL = '/api/v1/rule/test/grafana';
export const PROM_RULES_URL = 'api/prometheus/grafana/api/v1/rules';

export interface Data {
  refId: string;
  relativeTimeRange: RelativeTimeRange;
  queryType: string;
  datasourceUid: string;
  model: AlertQuery;
}

export interface GrafanaAlert {
  data?: Data;
  condition: string;
  no_data_state: GrafanaAlertStateDecision;
  title: string;
}

export interface Rule {
  grafana_alert: GrafanaAlert;
  for: string;
  labels: Labels;
  annotations: Annotations;
}

export type AlertInstances = Record<string, string>;

interface ExportRulesParams {
  format: ExportFormats;
  folderUid?: string;
  group?: string;
  ruleUid?: string;
}

export interface ModifyExportPayload {
  rules: Array<RulerAlertingRuleDTO | RulerRecordingRuleDTO | PostableRuleGrafanaRuleDTO>;
  name: string;
  interval?: string | undefined;
  source_tenants?: string[] | undefined;
}

export const alertRuleApi = alertingApi.injectEndpoints({
  endpoints: (build) => ({
    preview: build.mutation<
      PreviewResponse,
      {
        alertQueries: AlertQuery[];
        condition: string;
        folder: Folder;
        customLabels: Array<{
          key: string;
          value: string;
        }>;
        alertName?: string;
        alertUid?: string;
      }
    >({
      query: ({ alertQueries, condition, customLabels, folder, alertName, alertUid }) => ({
        url: PREVIEW_URL,
        data: {
          rule: {
            grafana_alert: {
              data: alertQueries,
              condition: condition,
              no_data_state: 'Alerting',
              title: alertName,
              uid: alertUid ?? 'N/A',
            },
            for: '0s',
            labels: arrayKeyValuesToObject(customLabels),
            annotations: {},
          },
          folderUid: folder.uid,
          folderTitle: folder.title,
        },
        method: 'POST',
      }),
    }),

    prometheusRulesByNamespace: build.query<
      RuleNamespace[],
      {
        limitAlerts?: number;
        identifier?: RuleIdentifier;
        filter?: FetchPromRulesFilter;
        state?: string[];
        matcher?: Matcher[];
      }
    >({
      query: ({ limitAlerts, identifier, filter, state, matcher }) => {
        const searchParams = new URLSearchParams();

        // if we're fetching for Grafana managed rules, we should add a limit to the number of alert instances
        // we do this because the response is large otherwise and we don't show all of them in the UI anyway.
        if (limitAlerts) {
          searchParams.set('limit_alerts', String(limitAlerts));
        }

        if (identifier && (isPrometheusRuleIdentifier(identifier) || isCloudRuleIdentifier(identifier))) {
          searchParams.set('file', identifier.namespace);
          searchParams.set('rule_group', identifier.groupName);
        }

        const params = prepareRulesFilterQueryParams(searchParams, filter);

        return { url: PROM_RULES_URL, params: paramsWithMatcherAndState(params, state, matcher) };
      },
      transformResponse: (response: PromRulesResponse): RuleNamespace[] => {
        return groupRulesByFileName(response.data.groups, GRAFANA_RULES_SOURCE_NAME);
      },
    }),

    prometheusRuleNamespaces: build.query<
      RuleNamespace[],
      { ruleSourceName: string; namespace?: string; groupName?: string; ruleName?: string }
    >({
      query: ({ ruleSourceName, namespace, groupName, ruleName }) => {
        const queryParams: Record<string, string | undefined> = {};
        // if (isPrometheusRuleIdentifier(ruleIdentifier) || isCloudRuleIdentifier(ruleIdentifier)) {
        queryParams['file'] = namespace;
        queryParams['rule_group'] = groupName;
        queryParams['rule_name'] = ruleName;
        // }

        return {
          url: `api/prometheus/${getDatasourceAPIUid(ruleSourceName)}/api/v1/rules`,
          params: queryParams,
        };
      },
      transformResponse: (response: PromRulesResponse, _, args): RuleNamespace[] => {
        return groupRulesByFileName(response.data.groups, args.ruleSourceName);
      },
    }),

    rulerRules: build.query<
      RulerRulesConfigDTO,
      { rulerConfig: RulerDataSourceConfig; filter?: FetchRulerRulesFilter }
    >({
      query: ({ rulerConfig, filter }) => {
        const { path, params } = rulerUrlBuilder(rulerConfig).rules(filter);
        return { url: path, params };
      },
    }),

    // TODO This should be probably a separate ruler API file
    rulerRuleGroup: build.query<
      RulerRuleGroupDTO,
      { rulerConfig: RulerDataSourceConfig; namespace: string; group: string }
    >({
      query: ({ rulerConfig, namespace, group }) => {
        const { path, params } = rulerUrlBuilder(rulerConfig).namespaceGroup(namespace, group);
        return { url: path, params };
      },
    }),

    exportRules: build.query<string, ExportRulesParams>({
      query: ({ format, folderUid, group, ruleUid }) => ({
        url: `/api/ruler/grafana/api/v1/export/rules`,
        params: { format: format, folderUid: folderUid, group: group, ruleUid: ruleUid },
        responseType: 'text',
      }),
    }),
    exportReceiver: build.query<string, { receiverName: string; decrypt: boolean; format: ExportFormats }>({
      query: ({ receiverName, decrypt, format }) => ({
        url: `/api/v1/provisioning/contact-points/export/`,
        params: { format: format, decrypt: decrypt, name: receiverName },
        responseType: 'text',
      }),
    }),
    exportReceivers: build.query<string, { decrypt: boolean; format: ExportFormats }>({
      query: ({ decrypt, format }) => ({
        url: `/api/v1/provisioning/contact-points/export/`,
        params: { format: format, decrypt: decrypt },
        responseType: 'text',
      }),
    }),
    exportPolicies: build.query<string, { format: ExportFormats }>({
      query: ({ format }) => ({
        url: `/api/v1/provisioning/policies/export/`,
        params: { format: format },
        responseType: 'text',
      }),
    }),
    exportModifiedRuleGroup: build.mutation<
      string,
      { payload: ModifyExportPayload; format: ExportFormats; nameSpace: string }
    >({
      query: ({ payload, format, nameSpace }) => ({
        url: `/api/ruler/grafana/api/v1/rules/${nameSpace}/export/`,
        params: { format: format },
        responseType: 'text',
        data: payload,
        method: 'POST',
      }),
    }),
  }),
});
