import React from 'react';

import {
  EmbeddedScene,
  NestedScene,
  QueryVariable,
  SceneFlexItem,
  SceneFlexLayout,
  SceneReactObject,
  SceneRefreshPicker,
  SceneTimePicker,
  SceneTimeRange,
  SceneVariableSet,
  VariableValueSelectors,
} from '@grafana/scenes';

import { getGrafanaInstancesByStateScene } from '../insights/grafana/AlertsByStateScene';
import { getGrafanaEvalSuccessVsFailuresScene } from '../insights/grafana/EvalSuccessVsFailuresScene';
import { getFiringGrafanaAlertsScene } from '../insights/grafana/Firing';
import { getInstanceStatByStatusScene } from '../insights/grafana/InstanceStatusScene';
import { getGrafanaMissedIterationsScene } from '../insights/grafana/MissedIterationsScene';
import { getMostFiredInstancesScene } from '../insights/grafana/MostFiredInstancesTable';
import { getPausedGrafanaAlertsScene } from '../insights/grafana/Paused';
import { getGrafanaRulesByEvaluationScene } from '../insights/grafana/RulesByEvaluation';
import { getGrafanaRulesByEvaluationPercentageScene } from '../insights/grafana/RulesByEvaluationPercentage';
import { getGrafanaAlertmanagerNotificationsScene } from '../insights/grafana/alertmanager/NotificationsScene';
import { getGrafanaAlertmanagerSilencesScene } from '../insights/grafana/alertmanager/SilencesByStateScene';
import { getAlertsByStateScene } from '../insights/mimir/AlertsByState';
import { getInvalidConfigScene } from '../insights/mimir/InvalidConfig';
import { getNotificationsScene } from '../insights/mimir/Notifications';
import { getSilencesScene } from '../insights/mimir/Silences';
import { getRuleGroupEvaluationDurationIntervalRatioScene } from '../insights/mimir/perGroup/RuleGroupEvaluationDurationIntervalRatioScene';
import { getRuleGroupEvaluationDurationScene } from '../insights/mimir/perGroup/RuleGroupEvaluationDurationScene';
import { getRuleGroupEvaluationsScene } from '../insights/mimir/perGroup/RuleGroupEvaluationsScene';
import { getRuleGroupIntervalScene } from '../insights/mimir/perGroup/RuleGroupIntervalScene';
import { getRulesPerGroupScene } from '../insights/mimir/perGroup/RulesPerGroupScene';
import { getEvalSuccessVsFailuresScene } from '../insights/mimir/rules/EvalSuccessVsFailuresScene';
import { getFiringCloudAlertsScene } from '../insights/mimir/rules/Firing';
import { getInstancesByStateScene } from '../insights/mimir/rules/InstancesByState';
import { getInstancesPercentageByStateScene } from '../insights/mimir/rules/InstancesPercentageByState';
import { getMissedIterationsScene } from '../insights/mimir/rules/MissedIterationsScene';
import { getMostFiredRulesScene } from '../insights/mimir/rules/MostFiredRules';
import { getPendingCloudAlertsScene } from '../insights/mimir/rules/Pending';

const ashDs = {
  type: 'loki',
  uid: 'grafanacloud-alert-state-history',
};

const cloudUsageDs = {
  type: 'prometheus',
  uid: 'grafanacloud-usage',
};

const grafanaCloudPromDs = {
  type: 'prometheus',
  uid: 'grafanacloud-prom',
};

const SERIES_COLORS = {
  alerting: 'red',
  firing: 'red',
  active: 'red',
  missed: 'red',
  failed: 'red',
  pending: 'yellow',
  nodata: 'blue',
  'active evaluation': 'blue',
  normal: 'green',
  success: 'green',
  error: 'orange',
};

export function overrideToFixedColor(key: keyof typeof SERIES_COLORS) {
  return {
    mode: 'fixed',
    fixedColor: SERIES_COLORS[key],
  };
}

export const PANEL_STYLES = { minHeight: 300 };

const THIS_WEEK_TIME_RANGE = new SceneTimeRange({ from: 'now-1w', to: 'now' });

export function SectionSubheader({ children }: React.PropsWithChildren) {
  return <div>{children}</div>;
}

export function getInsightsScenes() {
  return new EmbeddedScene({
    $timeRange: THIS_WEEK_TIME_RANGE,
    controls: [new SceneTimePicker({}), new SceneRefreshPicker({})],
    body: new SceneFlexLayout({
      direction: 'column',
      children: [
        new SceneFlexItem({
          ySizing: 'content',
          body: getGrafanaManagedScenes(),
        }),
        new SceneFlexItem({
          ySizing: 'content',
          body: getGrafanaAlertmanagerScenes(),
        }),
        new SceneFlexItem({
          ySizing: 'content',
          body: getCloudScenes(),
        }),
        new SceneFlexItem({
          ySizing: 'content',
          body: getMimirManagedRulesScenes(),
        }),
        new SceneFlexItem({
          ySizing: 'content',
          body: getMimirManagedRulesPerGroupScenes(),
        }),
      ],
    }),
  });
}

function getGrafanaManagedScenes() {
  return new NestedScene({
    title: 'Grafana-managed rules',
    canCollapse: true,
    isCollapsed: false,
    body: new SceneFlexLayout({
      direction: 'column',
      children: [
        new SceneFlexItem({
          body: new SceneReactObject({
            component: SectionSubheader,
            props: { children: <div>Grafana-managed rules</div> },
          }),
        }),
        new SceneFlexLayout({
          direction: 'column',
          children: [
            new SceneFlexLayout({
              children: [
                getMostFiredInstancesScene(ashDs, 'Top 10 firing instances this week'),
                getFiringGrafanaAlertsScene(cloudUsageDs, 'Firing rules'),
                getPausedGrafanaAlertsScene(cloudUsageDs, 'Paused rules'),
              ],
            }),
            new SceneFlexLayout({
              children: [
                getGrafanaInstancesByStateScene(cloudUsageDs, 'Alert instances by state'),
                new SceneFlexLayout({
                  height: '400px',
                  direction: 'column',
                  children: [
                    new SceneFlexLayout({
                      height: '400px',
                      children: [
                        getInstanceStatByStatusScene(cloudUsageDs, 'Alerting instances', 'alerting'),
                        getInstanceStatByStatusScene(cloudUsageDs, 'Pending instances', 'pending'),
                      ],
                    }),
                    new SceneFlexLayout({
                      children: [
                        getInstanceStatByStatusScene(cloudUsageDs, 'No data instances', 'nodata'),
                        getInstanceStatByStatusScene(cloudUsageDs, 'Error instances', 'error'),
                      ],
                    }),
                  ],
                }),
              ],
            }),
            new SceneFlexLayout({
              children: [
                getGrafanaRulesByEvaluationScene(cloudUsageDs, 'Alert rule evaluation'),
                getGrafanaRulesByEvaluationPercentageScene(cloudUsageDs, '% of alert rule evaluation'),
              ],
            }),
            new SceneFlexLayout({
              children: [
                getGrafanaEvalSuccessVsFailuresScene(cloudUsageDs, 'Evaluation success vs failures'),
                getGrafanaMissedIterationsScene(cloudUsageDs, 'Iterations missed per evaluation group'),
              ],
            }),
          ],
        }),
      ],
    }),
  });
}

function getGrafanaAlertmanagerScenes() {
  return new NestedScene({
    title: 'Grafana Alertmanager',
    canCollapse: true,
    isCollapsed: false,
    body: new SceneFlexLayout({
      direction: 'column',
      children: [
        new SceneFlexItem({
          body: new SceneReactObject({
            component: SectionSubheader,
            props: { children: <div>Grafana Alertmanager</div> },
          }),
        }),
        new SceneFlexLayout({
          children: [
            getGrafanaAlertmanagerNotificationsScene(cloudUsageDs, 'Notifications'),
            getGrafanaAlertmanagerSilencesScene(cloudUsageDs, 'Silences'),
          ],
        }),
      ],
    }),
  });
}

function getCloudScenes() {
  return new NestedScene({
    title: 'Mimir alertmanager',
    canCollapse: true,
    isCollapsed: false,
    body: new SceneFlexLayout({
      direction: 'column',
      children: [
        new SceneFlexItem({
          body: new SceneReactObject({
            component: SectionSubheader,
            props: { children: <div>Mimir Alertmanager</div> },
          }),
        }),
        new SceneFlexLayout({
          children: [
            getAlertsByStateScene(cloudUsageDs, 'Alerts by state'),
            getNotificationsScene(cloudUsageDs, 'Notifications'),
          ],
        }),
        new SceneFlexLayout({
          children: [
            getSilencesScene(cloudUsageDs, 'Silences'),
            getInvalidConfigScene(cloudUsageDs, 'Invalid configuration'),
          ],
        }),
      ],
    }),
  });
}

function getMimirManagedRulesScenes() {
  return new NestedScene({
    title: 'Mimir-managed rules',
    canCollapse: true,
    isCollapsed: false,
    body: new SceneFlexLayout({
      direction: 'column',
      children: [
        new SceneFlexItem({
          body: new SceneReactObject({
            component: SectionSubheader,
            props: { children: <div>Mimir-managed rules</div> },
          }),
        }),
        new SceneFlexLayout({
          children: [
            getMostFiredRulesScene(grafanaCloudPromDs, 'Top 10 firing rules this week'),
            getFiringCloudAlertsScene(grafanaCloudPromDs, 'Firing instances'),
            getPendingCloudAlertsScene(grafanaCloudPromDs, 'Pending instances'),
          ],
        }),
        new SceneFlexLayout({
          children: [
            getInstancesByStateScene(grafanaCloudPromDs, 'Count of alert instances by state'),
            getInstancesPercentageByStateScene(grafanaCloudPromDs, '% of alert instances by State'),
          ],
        }),
        new SceneFlexLayout({
          children: [
            getEvalSuccessVsFailuresScene(cloudUsageDs, 'Evaluation success vs failures'),
            getMissedIterationsScene(cloudUsageDs, 'Iterations missed'),
          ],
        }),
      ],
    }),
  });
}

function getMimirManagedRulesPerGroupScenes() {
  const ruleGroupHandler = new QueryVariable({
    label: 'Rule Group',
    name: 'rule_group',
    datasource: cloudUsageDs,
    query: 'label_values(grafanacloud_instance_rule_group_rules,rule_group)',
  });

  return new NestedScene({
    title: 'Mimir-managed Rules - Per Rule Group',
    canCollapse: true,
    isCollapsed: false,
    body: new SceneFlexLayout({
      direction: 'column',
      children: [
        new SceneFlexItem({
          body: new SceneReactObject({
            component: SectionSubheader,
            props: { children: <div>Mimir-managed Rules - Per Rule Group</div> },
          }),
        }),
        new SceneFlexLayout({
          children: [
            getRuleGroupEvaluationsScene(cloudUsageDs, 'Rule group evaluation'),
            getRuleGroupIntervalScene(cloudUsageDs, 'Rule group interval'),
          ],
        }),
        new SceneFlexLayout({
          children: [
            getRuleGroupEvaluationDurationScene(cloudUsageDs, 'Rule group evaluation duration'),
            getRulesPerGroupScene(cloudUsageDs, 'Rules per group'),
            getRuleGroupEvaluationDurationIntervalRatioScene(cloudUsageDs, 'Evaluation duration / interval ratio'),
          ],
        }),
      ],
    }),
    $variables: new SceneVariableSet({
      variables: [ruleGroupHandler],
    }),
    controls: [new VariableValueSelectors({})],
  });
}
