import { Subscription } from 'rxjs';

import { AnnotationQuery, DashboardCursorSync, dateTimeFormat, DateTimeInput, EventBusSrv } from '@grafana/data';
import { TimeRangeUpdatedEvent } from '@grafana/runtime';
import {
  behaviors,
  SceneDataTransformer,
  sceneGraph,
  SceneGridItem,
  SceneGridLayout,
  SceneGridRow,
  VizPanel,
} from '@grafana/scenes';

import { DashboardScene } from '../scene/DashboardScene';

import { dashboardSceneGraph } from './dashboardSceneGraph';
import { findVizPanelByKey, getPanelIdForVizPanel, getVizPanelKeyForPanelId } from './utils';

/**
 * Will move this to make it the main way we remain somewhat compatible with getDashboardSrv().getCurrent
 */
export class DashboardModelCompatibilityWrapper {
  public events = new EventBusSrv();
  private _subs = new Subscription();

  public constructor(private _scene: DashboardScene) {
    const timeRange = sceneGraph.getTimeRange(_scene);

    this._subs.add(
      timeRange.subscribeToState((state, prev) => {
        if (state.value !== prev.value) {
          this.events.publish(new TimeRangeUpdatedEvent(state.value));
        }
      })
    );
  }

  public get id(): number | null {
    return this._scene.state.id ?? null;
  }

  public get uid() {
    return this._scene.state.uid ?? null;
  }

  public get title() {
    return this._scene.state.title;
  }

  public get description() {
    return this._scene.state.description;
  }

  public get editable() {
    return this._scene.state.editable;
  }

  public get graphTooltip() {
    return this._getSyncMode();
  }

  public get timepicker() {
    return {
      refresh_intervals: dashboardSceneGraph.getRefreshPicker(this._scene)?.state.intervals,
    };
  }

  public get timezone() {
    return this.getTimezone();
  }

  public get weekStart() {
    return sceneGraph.getTimeRange(this._scene).state.weekStart;
  }

  public get tags() {
    return this._scene.state.tags;
  }

  public get meta() {
    return this._scene.state.meta;
  }

  public get time() {
    const time = sceneGraph.getTimeRange(this._scene);
    return {
      from: time.state.from,
      to: time.state.to,
    };
  }

  /**
   * Used from from timeseries migration handler to migrate time regions to dashboard annotations
   */
  public get annotations(): { list: AnnotationQuery[] } {
    console.error('Scenes DashboardModelCompatibilityWrapper.annotations not implemented (yet)');
    return { list: [] };
  }

  public getTimezone() {
    const time = sceneGraph.getTimeRange(this._scene);
    return time.getTimeZone();
  }

  public sharedTooltipModeEnabled() {
    return this._getSyncMode() > 0;
  }

  public sharedCrosshairModeOnly() {
    return this._getSyncMode() === 1;
  }

  private _getSyncMode() {
    if (this._scene.state.$behaviors) {
      for (const behavior of this._scene.state.$behaviors) {
        if (behavior instanceof behaviors.CursorSync) {
          return behavior.state.sync;
        }
      }
    }

    return DashboardCursorSync.Off;
  }

  public otherPanelInFullscreen(panel: unknown) {
    return false;
  }

  public formatDate(date: DateTimeInput, format?: string) {
    return dateTimeFormat(date, {
      format,
      timeZone: this.getTimezone(),
    });
  }

  public getPanelById(id: number): PanelCompatibilityWrapper | null {
    const vizPanel = findVizPanelByKey(this._scene, getVizPanelKeyForPanelId(id));
    if (vizPanel) {
      return new PanelCompatibilityWrapper(vizPanel);
    }

    return null;
  }

  /**
   * Mainly implemented to support Getting started panel's dissmis button.
   */
  public removePanel(panel: PanelCompatibilityWrapper) {
    const vizPanel = findVizPanelByKey(this._scene, getVizPanelKeyForPanelId(panel.id));
    if (!vizPanel) {
      console.error('Trying to remove a panel that was not found in scene', panel);
      return;
    }

    const gridItem = vizPanel.parent;
    if (!(gridItem instanceof SceneGridItem)) {
      console.error('Trying to remove a panel that is not wrapped in SceneGridItem');
      return;
    }

    const layout = sceneGraph.getLayout(vizPanel);
    if (!(layout instanceof SceneGridLayout)) {
      console.error('Trying to remove a panel in a layout that is not SceneGridLayout ');
      return;
    }

    // if grid item is directly in the layout just remove it
    if (layout === gridItem.parent) {
      layout.setState({
        children: layout.state.children.filter((child) => child !== gridItem),
      });
    }

    // Removing from a row is a bit more complicated
    if (gridItem.parent instanceof SceneGridRow) {
      // Clone the row and remove the grid item
      const newRow = layout.clone({
        children: layout.state.children.filter((child) => child !== gridItem),
      });

      // Now update the grid layout and replace the row with the updated one
      if (layout.parent instanceof SceneGridLayout) {
        layout.parent.setState({
          children: layout.parent.state.children.map((child) => (child === layout ? newRow : child)),
        });
      }
    }
  }

  public canEditAnnotations(dashboardUID?: string) {
    // TOOD
    return false;
  }

  public panelInitialized() {}

  public destroy() {
    this.events.removeAllListeners();
    this._subs.unsubscribe();
  }
}

class PanelCompatibilityWrapper {
  constructor(private _vizPanel: VizPanel) {}

  public get id() {
    const id = getPanelIdForVizPanel(this._vizPanel);

    if (isNaN(id)) {
      console.error('VizPanel key could not be translated to a legacy numeric panel id', this._vizPanel);
      return 0;
    }

    return id;
  }

  public get type() {
    return this._vizPanel.state.pluginId;
  }

  public get title() {
    return this._vizPanel.state.title;
  }

  public get transformations() {
    if (this._vizPanel.state.$data instanceof SceneDataTransformer) {
      return this._vizPanel.state.$data.state.transformations;
    }

    return [];
  }

  public refresh() {
    console.error('Scenes PanelCompatibilityWrapper.refresh no implemented (yet)');
  }

  public render() {
    console.error('Scenes PanelCompatibilityWrapper.render no implemented (yet)');
  }

  public getQueryRunner() {
    console.error('Scenes PanelCompatibilityWrapper.getQueryRunner no implemented (yet)');
  }
}
