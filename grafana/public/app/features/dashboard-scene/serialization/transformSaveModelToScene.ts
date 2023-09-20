import {
  ConstantVariableModel,
  CustomVariableModel,
  DataSourceVariableModel,
  QueryVariableModel,
  VariableModel,
} from '@grafana/data';
import {
  VizPanel,
  SceneTimePicker,
  SceneGridLayout,
  SceneGridRow,
  SceneTimeRange,
  SceneVariableSet,
  VariableValueSelectors,
  SceneVariable,
  CustomVariable,
  DataSourceVariable,
  QueryVariable,
  ConstantVariable,
  SceneRefreshPicker,
  SceneGridItem,
  SceneObject,
  SceneControlsSpacer,
  VizPanelMenu,
  behaviors,
  VizPanelState,
  SceneGridItemLike,
  SceneDataLayers,
  SceneDataLayerProvider,
  SceneDataLayerControls,
} from '@grafana/scenes';
import { getDashboardSrv } from 'app/features/dashboard/services/DashboardSrv';
import { DashboardModel, PanelModel } from 'app/features/dashboard/state';
import { DashboardDTO } from 'app/types';

import { DashboardAnnotationsDataLayer } from '../scene/DashboardAnnotationsDataLayer';
import { DashboardScene } from '../scene/DashboardScene';
import { LibraryVizPanel } from '../scene/LibraryVizPanel';
import { panelMenuBehavior } from '../scene/PanelMenuBehavior';
import { PanelRepeaterGridItem } from '../scene/PanelRepeaterGridItem';
import { PanelTimeRange } from '../scene/PanelTimeRange';
import { RowRepeaterBehavior } from '../scene/RowRepeaterBehavior';
import { createPanelDataProvider } from '../utils/createPanelDataProvider';
import { getVizPanelKeyForPanelId } from '../utils/utils';

export interface DashboardLoaderState {
  dashboard?: DashboardScene;
  isLoading?: boolean;
  loadError?: string;
}

export function transformSaveModelToScene(rsp: DashboardDTO): DashboardScene {
  // Just to have migrations run
  const oldModel = new DashboardModel(rsp.dashboard, rsp.meta, {
    autoMigrateOldPanels: true,
  });

  // Setting for built-in annotations query to run
  getDashboardSrv().setCurrent(oldModel);

  return createDashboardSceneFromDashboardModel(oldModel);
}

export function createSceneObjectsForPanels(oldPanels: PanelModel[]): SceneGridItemLike[] {
  // collects all panels and rows
  const panels: SceneGridItemLike[] = [];

  // indicates expanded row that's currently processed
  let currentRow: PanelModel | null = null;
  // collects panels in the currently processed, expanded row
  let currentRowPanels: SceneGridItemLike[] = [];

  for (const panel of oldPanels) {
    if (panel.type === 'row') {
      if (!currentRow) {
        if (Boolean(panel.collapsed)) {
          // collapsed rows contain their panels within the row model
          panels.push(createRowFromPanelModel(panel, []));
        } else {
          // indicate new row to be processed
          currentRow = panel;
        }
      } else {
        // when a row has been processed, and we hit a next one for processing
        if (currentRow.id !== panel.id) {
          // commit previous row panels
          panels.push(createRowFromPanelModel(currentRow, currentRowPanels));

          currentRow = panel;
          currentRowPanels = [];
        }
      }
    } else if (panel.libraryPanel?.uid && !('model' in panel.libraryPanel)) {
      const gridItem = new SceneGridItem({
        body: new LibraryVizPanel({
          title: panel.title,
          uid: panel.libraryPanel.uid,
        }),
        y: panel.gridPos.y,
        x: panel.gridPos.x,
        width: panel.gridPos.w,
        height: panel.gridPos.h,
      });
      panels.push(gridItem);
    } else {
      const panelObject = buildGridItemForPanel(panel);

      // when processing an expanded row, collect its panels
      if (currentRow) {
        currentRowPanels.push(panelObject);
      } else {
        panels.push(panelObject);
      }
    }
  }

  // commit a row if it's the last one
  if (currentRow) {
    panels.push(createRowFromPanelModel(currentRow, currentRowPanels));
  }

  return panels;
}

function createRowFromPanelModel(row: PanelModel, content: SceneGridItemLike[]): SceneGridItemLike {
  if (Boolean(row.collapsed)) {
    if (row.panels) {
      content = row.panels.map(buildGridItemForPanel);
    }
  }

  let behaviors: SceneObject[] | undefined;
  let children = content;

  if (row.repeat) {
    // For repeated rows the children are stored in the behavior
    children = [];
    behaviors = [
      new RowRepeaterBehavior({
        variableName: row.repeat,
        sources: content,
      }),
    ];
  }

  return new SceneGridRow({
    key: getVizPanelKeyForPanelId(row.id),
    title: row.title,
    y: row.gridPos.y,
    isCollapsed: row.collapsed,
    children: children,
    $behaviors: behaviors,
  });
}

export function createDashboardSceneFromDashboardModel(oldModel: DashboardModel) {
  let variables: SceneVariableSet | undefined = undefined;
  let layers: SceneDataLayerProvider[] = [];

  if (oldModel.templating?.list?.length) {
    const variableObjects = oldModel.templating.list
      .map((v) => {
        try {
          return createSceneVariableFromVariableModel(v);
        } catch (err) {
          console.error(err);
          return null;
        }
      })
      // TODO: Remove filter
      // Added temporarily to allow skipping non-compatible variables
      .filter((v): v is SceneVariable => Boolean(v));

    variables = new SceneVariableSet({
      variables: variableObjects,
    });
  }

  if (oldModel.annotations?.list?.length) {
    layers = oldModel.annotations?.list.map((a) => {
      // Each annotation query is an individual data layer
      return new DashboardAnnotationsDataLayer({
        query: a,
        name: a.name,
        isEnabled: Boolean(a.enable),
        isHidden: Boolean(a.hide),
      });
    });
  }

  const controls: SceneObject[] = [
    new SceneDataLayerControls(),
    new VariableValueSelectors({}),
    new SceneControlsSpacer(),
    new SceneTimePicker({}),
    new SceneRefreshPicker({
      refresh: oldModel.refresh,
      intervals: oldModel.timepicker.refresh_intervals,
    }),
  ];

  return new DashboardScene({
    title: oldModel.title,
    uid: oldModel.uid,
    meta: oldModel.meta,
    body: new SceneGridLayout({
      isLazy: true,
      children: createSceneObjectsForPanels(oldModel.panels),
    }),
    $timeRange: new SceneTimeRange(oldModel.time),
    $variables: variables,
    $behaviors: [
      new behaviors.CursorSync({
        sync: oldModel.graphTooltip,
      }),
    ],
    $data:
      layers.length > 0
        ? new SceneDataLayers({
            layers,
          })
        : undefined,
    controls: controls,
  });
}

export function createSceneVariableFromVariableModel(variable: VariableModel): SceneVariable {
  const commonProperties = {
    name: variable.name,
    label: variable.label,
  };
  if (isCustomVariable(variable)) {
    return new CustomVariable({
      ...commonProperties,
      value: variable.current.value,
      text: variable.current.text,
      description: variable.description,
      query: variable.query,
      isMulti: variable.multi,
      allValue: variable.allValue || undefined,
      includeAll: variable.includeAll,
      defaultToAll: Boolean(variable.includeAll),
      skipUrlSync: variable.skipUrlSync,
      hide: variable.hide,
    });
  } else if (isQueryVariable(variable)) {
    return new QueryVariable({
      ...commonProperties,
      value: variable.current.value,
      text: variable.current.text,
      description: variable.description,
      query: variable.query,
      datasource: variable.datasource,
      sort: variable.sort,
      refresh: variable.refresh,
      regex: variable.regex,
      allValue: variable.allValue || undefined,
      includeAll: variable.includeAll,
      defaultToAll: Boolean(variable.includeAll),
      isMulti: variable.multi,
      skipUrlSync: variable.skipUrlSync,
      hide: variable.hide,
    });
  } else if (isDataSourceVariable(variable)) {
    return new DataSourceVariable({
      ...commonProperties,
      value: variable.current.value,
      text: variable.current.text,
      description: variable.description,
      regex: variable.regex,
      pluginId: variable.query,
      allValue: variable.allValue || undefined,
      includeAll: variable.includeAll,
      defaultToAll: Boolean(variable.includeAll),
      skipUrlSync: variable.skipUrlSync,
      isMulti: variable.multi,
      hide: variable.hide,
    });
  } else if (isConstantVariable(variable)) {
    return new ConstantVariable({
      ...commonProperties,
      description: variable.description,
      value: variable.query,
      skipUrlSync: variable.skipUrlSync,
      hide: variable.hide,
    });
  } else {
    throw new Error(`Scenes: Unsupported variable type ${variable.type}`);
  }
}

export function buildGridItemForPanel(panel: PanelModel): SceneGridItemLike {
  const vizPanelState: VizPanelState = {
    key: getVizPanelKeyForPanelId(panel.id),
    title: panel.title,
    pluginId: panel.type,
    options: panel.options ?? {},
    fieldConfig: panel.fieldConfig,
    pluginVersion: panel.pluginVersion,
    displayMode: panel.transparent ? 'transparent' : undefined,
    // To be replaced with it's own option persited option instead derived
    hoverHeader: !panel.title && !panel.timeFrom && !panel.timeShift,
    $data: createPanelDataProvider(panel),
    menu: new VizPanelMenu({
      $behaviors: [panelMenuBehavior],
    }),
  };

  if (panel.timeFrom || panel.timeShift) {
    vizPanelState.$timeRange = new PanelTimeRange({
      timeFrom: panel.timeFrom,
      timeShift: panel.timeShift,
      hideTimeOverride: panel.hideTimeOverride,
    });
  }

  if (panel.repeat) {
    const repeatDirection = panel.repeatDirection ?? 'h';

    return new PanelRepeaterGridItem({
      key: `grid-item-${panel.id}`,
      x: panel.gridPos.x,
      y: panel.gridPos.y,
      width: repeatDirection === 'h' ? 24 : panel.gridPos.w,
      height: panel.gridPos.h,
      itemHeight: panel.gridPos.h,
      source: new VizPanel(vizPanelState),
      variableName: panel.repeat,
      repeatedPanels: [],
      repeatDirection: panel.repeatDirection,
      maxPerRow: panel.maxPerRow,
    });
  }

  return new SceneGridItem({
    key: `grid-item-${panel.id}`,
    x: panel.gridPos.x,
    y: panel.gridPos.y,
    width: panel.gridPos.w,
    height: panel.gridPos.h,
    body: new VizPanel(vizPanelState),
  });
}

const isCustomVariable = (v: VariableModel): v is CustomVariableModel => v.type === 'custom';
const isQueryVariable = (v: VariableModel): v is QueryVariableModel => v.type === 'query';
const isDataSourceVariable = (v: VariableModel): v is DataSourceVariableModel => v.type === 'datasource';
const isConstantVariable = (v: VariableModel): v is ConstantVariableModel => v.type === 'constant';
