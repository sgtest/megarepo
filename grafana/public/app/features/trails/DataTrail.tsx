import { css } from '@emotion/css';
import React from 'react';

import { AdHocVariableFilter, GrafanaTheme2, VariableHide, urlUtil } from '@grafana/data';
import { locationService } from '@grafana/runtime';
import {
  AdHocFiltersVariable,
  DataSourceVariable,
  getUrlSyncManager,
  SceneComponentProps,
  SceneControlsSpacer,
  sceneGraph,
  SceneObject,
  SceneObjectBase,
  SceneObjectState,
  SceneObjectUrlSyncConfig,
  SceneObjectUrlValues,
  SceneRefreshPicker,
  SceneTimePicker,
  SceneTimeRange,
  sceneUtils,
  SceneVariable,
  SceneVariableSet,
  VariableDependencyConfig,
  VariableValueSelectors,
} from '@grafana/scenes';
import { useStyles2 } from '@grafana/ui';

import { DataTrailSettings } from './DataTrailSettings';
import { DataTrailHistory } from './DataTrailsHistory';
import { MetricScene } from './MetricScene';
import { MetricSelectScene } from './MetricSelectScene';
import { MetricsHeader } from './MetricsHeader';
import { getTrailStore } from './TrailStore/TrailStore';
import { MetricDatasourceHelper } from './helpers/MetricDatasourceHelper';
import { reportChangeInLabelFilters } from './interactions';
import { MetricSelectedEvent, trailDS, VAR_DATASOURCE, VAR_FILTERS } from './shared';

export interface DataTrailState extends SceneObjectState {
  topScene?: SceneObject;
  embedded?: boolean;
  controls: SceneObject[];
  history: DataTrailHistory;
  settings: DataTrailSettings;
  createdAt: number;

  // just for for the starting data source
  initialDS?: string;
  initialFilters?: AdHocVariableFilter[];

  // Synced with url
  metric?: string;
}

export class DataTrail extends SceneObjectBase<DataTrailState> {
  protected _urlSync = new SceneObjectUrlSyncConfig(this, { keys: ['metric'] });

  public constructor(state: Partial<DataTrailState>) {
    super({
      $timeRange: state.$timeRange ?? new SceneTimeRange({}),
      $variables: state.$variables ?? getVariableSet(state.initialDS, state.metric, state.initialFilters),
      controls: state.controls ?? [
        new VariableValueSelectors({ layout: 'vertical' }),
        new SceneControlsSpacer(),
        new SceneTimePicker({}),
        new SceneRefreshPicker({}),
      ],
      history: state.history ?? new DataTrailHistory({}),
      settings: state.settings ?? new DataTrailSettings({}),
      createdAt: state.createdAt ?? new Date().getTime(),
      ...state,
    });

    this.addActivationHandler(this._onActivate.bind(this));
  }

  public _onActivate() {
    if (!this.state.topScene) {
      this.setState({ topScene: getTopSceneFor(this.state.metric) });
    }

    // Some scene elements publish this
    this.subscribeToEvent(MetricSelectedEvent, this._handleMetricSelectedEvent.bind(this));

    const filtersVariable = sceneGraph.lookupVariable(VAR_FILTERS, this);
    if (filtersVariable instanceof AdHocFiltersVariable) {
      this._subs.add(
        filtersVariable?.subscribeToState((newState, prevState) => {
          if (!this._addingFilterWithoutReportingInteraction) {
            reportChangeInLabelFilters(newState.filters, prevState.filters);
          }
        })
      );
    }

    this.enableUrlSync();

    return () => {
      this.disableUrlSync();

      if (!this.state.embedded) {
        getTrailStore().setRecentTrail(this);
      }
    };
  }

  private enableUrlSync() {
    if (!this.state.embedded) {
      getUrlSyncManager().initSync(this);
    }
  }

  private disableUrlSync() {
    if (!this.state.embedded) {
      getUrlSyncManager().cleanUp(this);
    }
  }

  protected _variableDependency = new VariableDependencyConfig(this, {
    variableNames: [VAR_DATASOURCE],
    onReferencedVariableValueChanged: (variable: SceneVariable) => {
      const { name } = variable.state;
      if (name === VAR_DATASOURCE) {
        this.datasourceHelper.reset();
      }
    },
  });

  /**
   * Assuming that the change in filter was already reported with a cause other than `'adhoc_filter'`,
   * this will modify the adhoc filter variable and prevent the automatic reporting which would
   * normally occur through the call to `reportChangeInLabelFilters`.
   */
  public addFilterWithoutReportingInteraction(filter: AdHocVariableFilter) {
    const variable = sceneGraph.lookupVariable('filters', this);
    if (!(variable instanceof AdHocFiltersVariable)) {
      return;
    }

    this._addingFilterWithoutReportingInteraction = true;

    variable.setState({ filters: [...variable.state.filters, filter] });

    this._addingFilterWithoutReportingInteraction = false;
  }

  private _addingFilterWithoutReportingInteraction = false;
  private datasourceHelper = new MetricDatasourceHelper(this);

  public getMetricMetadata(metric?: string) {
    return this.datasourceHelper.getMetricMetadata(metric);
  }

  public getCurrentMetricMetadata() {
    return this.getMetricMetadata(this.state.metric);
  }

  public restoreFromHistoryStep(state: DataTrailState) {
    this.disableUrlSync();

    this.setState(
      sceneUtils.cloneSceneObjectState(state, {
        history: this.state.history,
        metric: !state.metric ? undefined : state.metric,
      })
    );

    const urlState = getUrlSyncManager().getUrlState(this);
    const fullUrl = urlUtil.renderUrl(locationService.getLocation().pathname, urlState);
    locationService.replace(fullUrl);

    this.enableUrlSync();
  }

  private _handleMetricSelectedEvent(evt: MetricSelectedEvent) {
    this.setState(this.getSceneUpdatesForNewMetricValue(evt.payload));

    // Add metric to adhoc filters baseFilter
    const filterVar = sceneGraph.lookupVariable(VAR_FILTERS, this);
    if (filterVar instanceof AdHocFiltersVariable) {
      filterVar.setState({
        baseFilters: getBaseFiltersForMetric(evt.payload),
      });
    }
  }

  private getSceneUpdatesForNewMetricValue(metric: string | undefined) {
    const stateUpdate: Partial<DataTrailState> = {};
    stateUpdate.metric = metric;
    stateUpdate.topScene = getTopSceneFor(metric);
    return stateUpdate;
  }

  getUrlState() {
    return { metric: this.state.metric };
  }

  updateFromUrl(values: SceneObjectUrlValues) {
    const stateUpdate: Partial<DataTrailState> = {};

    if (typeof values.metric === 'string') {
      if (this.state.metric !== values.metric) {
        Object.assign(stateUpdate, this.getSceneUpdatesForNewMetricValue(values.metric));
      }
    } else if (values.metric === null) {
      stateUpdate.metric = undefined;
      stateUpdate.topScene = new MetricSelectScene({});
    }

    this.setState(stateUpdate);
  }

  static Component = ({ model }: SceneComponentProps<DataTrail>) => {
    const { controls, topScene, history, settings } = model.useState();
    const styles = useStyles2(getStyles);
    const showHeaderForFirstTimeUsers = getTrailStore().recent.length < 2;

    return (
      <div className={styles.container}>
        {showHeaderForFirstTimeUsers && <MetricsHeader />}
        <history.Component model={history} />
        {controls && (
          <div className={styles.controls}>
            {controls.map((control) => (
              <control.Component key={control.state.key} model={control} />
            ))}
            <settings.Component model={settings} />
          </div>
        )}
        <div className={styles.body}>{topScene && <topScene.Component model={topScene} />}</div>
      </div>
    );
  };
}

export function getTopSceneFor(metric?: string) {
  if (metric) {
    return new MetricScene({ metric: metric });
  } else {
    return new MetricSelectScene({});
  }
}

function getVariableSet(initialDS?: string, metric?: string, initialFilters?: AdHocVariableFilter[]) {
  return new SceneVariableSet({
    variables: [
      new DataSourceVariable({
        name: VAR_DATASOURCE,
        label: 'Data source',
        description: 'Only prometheus data sources are supported',
        value: initialDS,
        pluginId: 'prometheus',
      }),
      new AdHocFiltersVariable({
        name: VAR_FILTERS,
        addFilterButtonText: 'Add label',
        datasource: trailDS,
        hide: VariableHide.hideLabel,
        layout: 'vertical',
        filters: initialFilters ?? [],
        baseFilters: getBaseFiltersForMetric(metric),
      }),
    ],
  });
}

function getStyles(theme: GrafanaTheme2) {
  return {
    container: css({
      flexGrow: 1,
      display: 'flex',
      gap: theme.spacing(1),
      minHeight: '100%',
      flexDirection: 'column',
    }),
    body: css({
      flexGrow: 1,
      display: 'flex',
      flexDirection: 'column',
    }),
    controls: css({
      display: 'flex',
      gap: theme.spacing(1),
      padding: theme.spacing(1, 0),
      alignItems: 'flex-end',
      flexWrap: 'wrap',
      position: 'sticky',
      background: theme.isDark ? theme.colors.background.canvas : theme.colors.background.primary,
      zIndex: theme.zIndex.navbarFixed,
      top: 0,
    }),
  };
}

function getBaseFiltersForMetric(metric?: string): AdHocVariableFilter[] {
  if (metric) {
    return [{ key: '__name__', operator: '=', value: metric }];
  }
  return [];
}
