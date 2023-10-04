import React from 'react';

import {
  EmbeddedScene,
  NestedScene,
  QueryVariable,
  SceneFlexItem,
  SceneFlexLayout,
  SceneReactObject,
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
const LAST_WEEK_TIME_RANGE = new SceneTimeRange({ from: 'now-2w', to: 'now-1w' });

export function SectionSubheader({ children }: React.PropsWithChildren) {
  return <div>{children}</div>;
}

export function getInsightsScenes() {
  return new EmbeddedScene({
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
                getMostFiredInstancesScene(THIS_WEEK_TIME_RANGE, ashDs, 'Top 10 firing instances this week'),
                getFiringGrafanaAlertsScene(THIS_WEEK_TIME_RANGE, cloudUsageDs, 'Firing rules'),
                getPausedGrafanaAlertsScene(THIS_WEEK_TIME_RANGE, cloudUsageDs, 'Paused rules'),
              ],
            }),
            new SceneFlexLayout({
              children: [
                getGrafanaInstancesByStateScene(THIS_WEEK_TIME_RANGE, cloudUsageDs, 'Alert instances by state'),
                new SceneFlexLayout({
                  height: '400px',
                  direction: 'column',
                  children: [
                    new SceneFlexLayout({
                      height: '400px',
                      children: [
                        getInstanceStatByStatusScene(
                          THIS_WEEK_TIME_RANGE,
                          cloudUsageDs,
                          'Alerting instances',
                          'alerting'
                        ),
                        getInstanceStatByStatusScene(
                          THIS_WEEK_TIME_RANGE,
                          cloudUsageDs,
                          'Pending instances',
                          'pending'
                        ),
                      ],
                    }),
                    new SceneFlexLayout({
                      children: [
                        getInstanceStatByStatusScene(THIS_WEEK_TIME_RANGE, cloudUsageDs, 'No data instances', 'nodata'),
                        getInstanceStatByStatusScene(THIS_WEEK_TIME_RANGE, cloudUsageDs, 'Error instances', 'error'),
                      ],
                    }),
                  ],
                }),
              ],
            }),
            new SceneFlexLayout({
              children: [
                getGrafanaRulesByEvaluationScene(THIS_WEEK_TIME_RANGE, cloudUsageDs, 'Alert rule evaluation'),
                getGrafanaRulesByEvaluationPercentageScene(
                  THIS_WEEK_TIME_RANGE,
                  cloudUsageDs,
                  '% of alert rule evaluation'
                ),
              ],
            }),
            new SceneFlexLayout({
              children: [
                getGrafanaEvalSuccessVsFailuresScene(
                  THIS_WEEK_TIME_RANGE,
                  cloudUsageDs,
                  'Evaluation success vs failures'
                ),
                getGrafanaMissedIterationsScene(
                  THIS_WEEK_TIME_RANGE,
                  cloudUsageDs,
                  'Iterations missed per evaluation group'
                ),
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
            getGrafanaAlertmanagerNotificationsScene(THIS_WEEK_TIME_RANGE, cloudUsageDs, 'Notifications'),
            getGrafanaAlertmanagerSilencesScene(LAST_WEEK_TIME_RANGE, cloudUsageDs, 'Silences'),
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
            getAlertsByStateScene(THIS_WEEK_TIME_RANGE, cloudUsageDs, 'Alerts by state'),
            getNotificationsScene(THIS_WEEK_TIME_RANGE, cloudUsageDs, 'Notifications'),
          ],
        }),
        new SceneFlexLayout({
          children: [
            getSilencesScene(LAST_WEEK_TIME_RANGE, cloudUsageDs, 'Silences'),
            getInvalidConfigScene(THIS_WEEK_TIME_RANGE, cloudUsageDs, 'Invalid configuration'),
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
            getMostFiredRulesScene(THIS_WEEK_TIME_RANGE, grafanaCloudPromDs, 'Top 10 firing rules this week'),
            getFiringCloudAlertsScene(THIS_WEEK_TIME_RANGE, grafanaCloudPromDs, 'Firing instances'),
            getPendingCloudAlertsScene(THIS_WEEK_TIME_RANGE, grafanaCloudPromDs, 'Pending instances'),
          ],
        }),
        new SceneFlexLayout({
          children: [
            getInstancesByStateScene(THIS_WEEK_TIME_RANGE, grafanaCloudPromDs, 'Count of alert instances by state'),
            getInstancesPercentageByStateScene(
              THIS_WEEK_TIME_RANGE,
              grafanaCloudPromDs,
              '% of alert instances by State'
            ),
          ],
        }),
        new SceneFlexLayout({
          children: [
            getEvalSuccessVsFailuresScene(THIS_WEEK_TIME_RANGE, cloudUsageDs, 'Evaluation success vs failures'),
            getMissedIterationsScene(THIS_WEEK_TIME_RANGE, cloudUsageDs, 'Iterations missed'),
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
            getRuleGroupEvaluationsScene(THIS_WEEK_TIME_RANGE, cloudUsageDs, 'Rule group evaluation'),
            getRuleGroupIntervalScene(THIS_WEEK_TIME_RANGE, cloudUsageDs, 'Rule group interval'),
          ],
        }),
        new SceneFlexLayout({
          children: [
            getRuleGroupEvaluationDurationScene(THIS_WEEK_TIME_RANGE, cloudUsageDs, 'Rule group evaluation duration'),
            getRulesPerGroupScene(THIS_WEEK_TIME_RANGE, cloudUsageDs, 'Rules per group'),
            getRuleGroupEvaluationDurationIntervalRatioScene(
              THIS_WEEK_TIME_RANGE,
              cloudUsageDs,
              'Evaluation duration / interval ratio'
            ),
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
