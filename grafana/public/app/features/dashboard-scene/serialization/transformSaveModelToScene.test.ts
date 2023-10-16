import { LoadingState } from '@grafana/data';
import { getPanelPlugin } from '@grafana/data/test/__mocks__/pluginMocks';
import { config } from '@grafana/runtime';
import {
  AdHocFilterSet,
  behaviors,
  CustomVariable,
  DataSourceVariable,
  QueryVariable,
  SceneDataLayerControls,
  SceneDataLayers,
  SceneDataTransformer,
  SceneGridItem,
  SceneGridLayout,
  SceneGridRow,
  VizPanel,
} from '@grafana/scenes';
import { DashboardCursorSync, defaultDashboard, Panel, RowPanel, VariableType } from '@grafana/schema';
import { DashboardModel, PanelModel } from 'app/features/dashboard/state';
import { createPanelSaveModel } from 'app/features/dashboard/state/__fixtures__/dashboardFixtures';
import { SHARED_DASHBOARD_QUERY } from 'app/plugins/datasource/dashboard';
import { DASHBOARD_DATASOURCE_PLUGIN_ID } from 'app/plugins/datasource/dashboard/types';

import { PanelRepeaterGridItem } from '../scene/PanelRepeaterGridItem';
import { PanelTimeRange } from '../scene/PanelTimeRange';
import { RowRepeaterBehavior } from '../scene/RowRepeaterBehavior';
import { ShareQueryDataProvider } from '../scene/ShareQueryDataProvider';
import { getQueryRunnerFor } from '../utils/utils';

import dashboard_to_load1 from './testfiles/dashboard_to_load1.json';
import repeatingRowsAndPanelsDashboardJson from './testfiles/repeating_rows_and_panels.json';
import {
  createDashboardSceneFromDashboardModel,
  buildGridItemForPanel,
  createSceneVariableFromVariableModel,
  transformSaveModelToScene,
} from './transformSaveModelToScene';

describe('transformSaveModelToScene', () => {
  describe('when creating dashboard scene', () => {
    it('should initialize the DashboardScene with the model state', () => {
      const dash = {
        ...defaultDashboard,
        title: 'test',
        uid: 'test-uid',
        time: { from: 'now-10h', to: 'now' },
        weekStart: 'saturday',
        fiscalYearStartMonth: 2,
        timezone: 'America/New_York',
        templating: {
          list: [
            {
              hide: 2,
              name: 'constant',
              skipUrlSync: false,
              type: 'constant' as VariableType,
              query: 'test',
              id: 'constant',
              global: false,
              index: 3,
              state: LoadingState.Done,
              error: null,
              description: '',
              datasource: null,
            },
            {
              hide: 2,
              name: 'CoolFilters',
              type: 'adhoc' as VariableType,
              datasource: { uid: 'gdev-prometheus', type: 'prometheus' },
              id: 'adhoc',
              global: false,
              skipUrlSync: false,
              index: 3,
              state: LoadingState.Done,
              error: null,
              description: '',
            },
          ],
        },
      };
      const oldModel = new DashboardModel(dash);

      const scene = createDashboardSceneFromDashboardModel(oldModel);

      expect(scene.state.title).toBe('test');
      expect(scene.state.uid).toBe('test-uid');
      expect(scene.state?.$timeRange?.state.value.raw).toEqual(dash.time);
      expect(scene.state?.$timeRange?.state.fiscalYearStartMonth).toEqual(2);
      expect(scene.state?.$timeRange?.state.timeZone).toEqual('America/New_York');
      expect(scene.state?.$timeRange?.state.weekStart).toEqual('saturday');
      expect(scene.state?.$variables?.state.variables).toHaveLength(1);
      expect(scene.state.controls).toBeDefined();
      expect(scene.state.controls![1]).toBeInstanceOf(AdHocFilterSet);
      expect((scene.state.controls![1] as AdHocFilterSet).state.name).toBe('CoolFilters');
    });

    it('should apply cursor sync behavior', () => {
      const dash = {
        ...defaultDashboard,
        graphTooltip: DashboardCursorSync.Crosshair,
      };
      const oldModel = new DashboardModel(dash);

      const scene = createDashboardSceneFromDashboardModel(oldModel);

      expect(scene.state.$behaviors).toHaveLength(1);
      expect(scene.state.$behaviors![0]).toBeInstanceOf(behaviors.CursorSync);
      expect((scene.state.$behaviors![0] as behaviors.CursorSync).state.sync).toEqual(DashboardCursorSync.Crosshair);
    });
  });

  describe('when organizing panels as scene children', () => {
    it('should create panels within collapsed rows', () => {
      const panel = createPanelSaveModel({
        title: 'test',
        gridPos: { x: 1, y: 0, w: 12, h: 8 },
      }) as Panel;

      const row = createPanelSaveModel({
        title: 'test',
        type: 'row',
        gridPos: { x: 0, y: 0, w: 12, h: 1 },
        collapsed: true,
        panels: [panel],
      }) as unknown as RowPanel;

      const dashboard = {
        ...defaultDashboard,
        panels: [row],
      };

      const oldModel = new DashboardModel(dashboard);

      const scene = createDashboardSceneFromDashboardModel(oldModel);
      const body = scene.state.body as SceneGridLayout;

      expect(body.state.children).toHaveLength(1);
      const rowScene = body.state.children[0] as SceneGridRow;
      expect(rowScene).toBeInstanceOf(SceneGridRow);
      expect(rowScene.state.title).toEqual(row.title);
      expect(rowScene.state.y).toEqual(row.gridPos!.y);
      expect(rowScene.state.isCollapsed).toEqual(row.collapsed);
      expect(rowScene.state.children).toHaveLength(1);
      expect(rowScene.state.children[0]).toBeInstanceOf(SceneGridItem);
    });

    it('should create panels within expanded row', () => {
      const panelOutOfRow = createPanelSaveModel({
        title: 'Out of a row',
        gridPos: {
          h: 8,
          w: 12,
          x: 0,
          y: 0,
        },
      });
      const rowWithPanel = createPanelSaveModel({
        title: 'Row with panel',
        type: 'row',
        id: 10,
        collapsed: false,
        gridPos: {
          h: 1,
          w: 24,
          x: 0,
          y: 8,
        },
        // This panels array is not used if the row is not collapsed
        panels: [],
      });
      const panelInRow = createPanelSaveModel({
        gridPos: {
          h: 8,
          w: 12,
          x: 0,
          y: 9,
        },
        title: 'In row 1',
      });
      const emptyRow = createPanelSaveModel({
        collapsed: false,
        gridPos: {
          h: 1,
          w: 24,
          x: 0,
          y: 17,
        },
        // This panels array is not used if the row is not collapsed
        panels: [],
        title: 'Empty row',
        type: 'row',
      });
      const dashboard = {
        ...defaultDashboard,
        panels: [panelOutOfRow, rowWithPanel, panelInRow, emptyRow],
      };

      const oldModel = new DashboardModel(dashboard);

      const scene = createDashboardSceneFromDashboardModel(oldModel);
      const body = scene.state.body as SceneGridLayout;

      expect(body.state.children).toHaveLength(3);
      expect(body).toBeInstanceOf(SceneGridLayout);
      // Panel out of row
      expect(body.state.children[0]).toBeInstanceOf(SceneGridItem);
      const panelOutOfRowVizPanel = body.state.children[0] as SceneGridItem;
      expect((panelOutOfRowVizPanel.state.body as VizPanel)?.state.title).toBe(panelOutOfRow.title);
      // Row with panel
      expect(body.state.children[1]).toBeInstanceOf(SceneGridRow);
      const rowWithPanelsScene = body.state.children[1] as SceneGridRow;
      expect(rowWithPanelsScene.state.title).toBe(rowWithPanel.title);
      expect(rowWithPanelsScene.state.key).toBe('panel-10');
      expect(rowWithPanelsScene.state.children).toHaveLength(1);
      // Panel within row
      expect(rowWithPanelsScene.state.children[0]).toBeInstanceOf(SceneGridItem);
      const panelInRowVizPanel = rowWithPanelsScene.state.children[0] as SceneGridItem;
      expect((panelInRowVizPanel.state.body as VizPanel).state.title).toBe(panelInRow.title);
      // Empty row
      expect(body.state.children[2]).toBeInstanceOf(SceneGridRow);
      const emptyRowScene = body.state.children[2] as SceneGridRow;
      expect(emptyRowScene.state.title).toBe(emptyRow.title);
      expect(emptyRowScene.state.children).toHaveLength(0);
    });
  });

  describe('when creating viz panel objects', () => {
    it('should initalize the VizPanel scene object state', () => {
      const panel = {
        title: 'test',
        type: 'test-plugin',
        gridPos: { x: 0, y: 0, w: 12, h: 8 },
        maxDataPoints: 100,
        options: {
          fieldOptions: {
            defaults: {
              unit: 'none',
              decimals: 2,
            },
            overrides: [],
          },
        },
        fieldConfig: {
          defaults: {
            unit: 'none',
          },
          overrides: [],
        },
        pluginVersion: '1.0.0',
        transformations: [
          {
            id: 'reduce',
            options: {
              reducers: [
                {
                  id: 'mean',
                },
              ],
            },
          },
        ],
        targets: [
          {
            refId: 'A',
            queryType: 'randomWalk',
          },
        ],
      };

      const { gridItem, vizPanel } = buildGridItemForTest(panel);

      expect(gridItem.state.x).toEqual(0);
      expect(gridItem.state.y).toEqual(0);
      expect(gridItem.state.width).toEqual(12);
      expect(gridItem.state.height).toEqual(8);

      expect(vizPanel.state.title).toBe('test');
      expect(vizPanel.state.pluginId).toBe('test-plugin');
      expect(vizPanel.state.options).toEqual(panel.options);
      expect(vizPanel.state.fieldConfig).toEqual(panel.fieldConfig);
      expect(vizPanel.state.pluginVersion).toBe('1.0.0');

      const queryRunner = getQueryRunnerFor(vizPanel)!;
      expect(queryRunner.state.queries).toEqual(panel.targets);
      expect(queryRunner.state.maxDataPoints).toEqual(100);
      expect(queryRunner.state.maxDataPointsFromWidth).toEqual(true);

      expect((vizPanel.state.$data as SceneDataTransformer)?.state.transformations).toEqual(panel.transformations);
    });

    it('should initalize the VizPanel without title and transparent true', () => {
      const panel = {
        title: '',
        type: 'test-plugin',
        gridPos: { x: 0, y: 0, w: 12, h: 8 },
        transparent: true,
      };

      const { vizPanel } = buildGridItemForTest(panel);

      expect(vizPanel.state.displayMode).toEqual('transparent');
      expect(vizPanel.state.hoverHeader).toEqual(true);
    });

    it('should set PanelTimeRange when timeFrom or timeShift is present', () => {
      const panel = {
        type: 'test-plugin',
        timeFrom: '2h',
        timeShift: '1d',
      };

      const { vizPanel } = buildGridItemForTest(panel);
      const timeRange = vizPanel.state.$timeRange as PanelTimeRange;

      expect(timeRange).toBeInstanceOf(PanelTimeRange);
      expect(timeRange.state.timeFrom).toBe('2h');
      expect(timeRange.state.timeShift).toBe('1d');
    });

    it('should handle a dashboard query data source', () => {
      const panel = {
        title: '',
        type: 'test-plugin',
        datasource: { uid: SHARED_DASHBOARD_QUERY, type: DASHBOARD_DATASOURCE_PLUGIN_ID },
        gridPos: { x: 0, y: 0, w: 12, h: 8 },
        transparent: true,
        targets: [{ refId: 'A', panelId: 10 }],
      };

      const { vizPanel } = buildGridItemForTest(panel);
      expect(vizPanel.state.$data).toBeInstanceOf(ShareQueryDataProvider);
    });

    it('should not set SceneQueryRunner for plugins with skipDataQuery', () => {
      const panel = {
        title: '',
        type: 'text-plugin-34',
        gridPos: { x: 0, y: 0, w: 12, h: 8 },
        transparent: true,
        targets: [{ refId: 'A' }],
      };

      config.panels['text-plugin-34'] = getPanelPlugin({
        skipDataQuery: true,
      }).meta;

      const { vizPanel } = buildGridItemForTest(panel);

      expect(vizPanel.state.$data).toBeUndefined();
    });

    it('When repeat is set should build PanelRepeaterGridItem', () => {
      const panel = {
        title: '',
        type: 'text-plugin-34',
        gridPos: { x: 0, y: 0, w: 8, h: 8 },
        repeat: 'server',
        repeatDirection: 'v',
        maxPerRow: 8,
      };

      const gridItem = buildGridItemForPanel(new PanelModel(panel));
      const repeater = gridItem as PanelRepeaterGridItem;

      expect(repeater.state.maxPerRow).toBe(8);
      expect(repeater.state.variableName).toBe('server');
      expect(repeater.state.width).toBe(8);
      expect(repeater.state.height).toBe(8);
      expect(repeater.state.repeatDirection).toBe('v');
      expect(repeater.state.maxPerRow).toBe(8);
    });
  });

  describe('when creating variables objects', () => {
    it('should migrate custom variable', () => {
      const variable = {
        current: {
          selected: false,
          text: 'a',
          value: 'a',
        },
        hide: 0,
        includeAll: false,
        multi: false,
        name: 'query0',
        options: [
          {
            selected: true,
            text: 'a',
            value: 'a',
          },
          {
            selected: false,
            text: 'b',
            value: 'b',
          },
          {
            selected: false,
            text: 'c',
            value: 'c',
          },
          {
            selected: false,
            text: 'd',
            value: 'd',
          },
        ],
        query: 'a,b,c,d',
        skipUrlSync: false,
        type: 'custom' as VariableType,
        rootStateKey: 'N4XLmH5Vz',
        id: 'query0',
        global: false,
        index: 0,
        state: 'Done',
        error: null,
        description: null,
        allValue: null,
      };

      const migrated = createSceneVariableFromVariableModel(variable);
      const { key, ...rest } = migrated.state;

      expect(migrated).toBeInstanceOf(CustomVariable);
      expect(rest).toEqual({
        allValue: undefined,
        defaultToAll: false,
        description: null,
        includeAll: false,
        isMulti: false,
        label: undefined,
        name: 'query0',
        options: [],
        query: 'a,b,c,d',
        skipUrlSync: false,
        text: 'a',
        type: 'custom',
        value: 'a',
        hide: 0,
      });
    });

    it('should migrate query variable', () => {
      const variable = {
        allValue: null,
        current: {
          text: 'America',
          value: 'America',
          selected: false,
        },
        datasource: {
          uid: 'P15396BDD62B2BE29',
          type: 'influxdb',
        },
        definition: '',
        hide: 0,
        includeAll: false,
        label: 'Datacenter',
        multi: false,
        name: 'datacenter',
        options: [
          {
            text: 'America',
            value: 'America',
            selected: true,
          },
          {
            text: 'Africa',
            value: 'Africa',
            selected: false,
          },
          {
            text: 'Asia',
            value: 'Asia',
            selected: false,
          },
          {
            text: 'Europe',
            value: 'Europe',
            selected: false,
          },
        ],
        query: 'SHOW TAG VALUES  WITH KEY = "datacenter" ',
        refresh: 1,
        regex: '',
        skipUrlSync: false,
        sort: 0,
        tagValuesQuery: null,
        tagsQuery: null,
        type: 'query' as VariableType,
        useTags: false,
        rootStateKey: '000000002',
        id: 'datacenter',
        global: false,
        index: 0,
        state: 'Done',
        error: null,
        description: null,
      };

      const migrated = createSceneVariableFromVariableModel(variable);
      const { key, ...rest } = migrated.state;

      expect(migrated).toBeInstanceOf(QueryVariable);
      expect(rest).toEqual({
        allValue: undefined,
        datasource: {
          type: 'influxdb',
          uid: 'P15396BDD62B2BE29',
        },
        defaultToAll: false,
        description: null,
        includeAll: false,
        isMulti: false,
        label: 'Datacenter',
        name: 'datacenter',
        options: [],
        query: 'SHOW TAG VALUES  WITH KEY = "datacenter" ',
        refresh: 1,
        regex: '',
        skipUrlSync: false,
        sort: 0,
        text: 'America',
        type: 'query',
        value: 'America',
        hide: 0,
      });
    });

    it('should migrate datasource variable', () => {
      const variable = {
        id: 'query1',
        rootStateKey: 'N4XLmH5Vz',
        name: 'query1',
        type: 'datasource' as VariableType,
        global: false,
        index: 1,
        hide: 0,
        skipUrlSync: false,
        state: 'Done',
        error: null,
        description: null,
        current: {
          value: ['gdev-prometheus', 'gdev-slow-prometheus'],
          text: ['gdev-prometheus', 'gdev-slow-prometheus'],
          selected: true,
        },
        regex: '/^gdev/',
        options: [
          {
            text: 'All',
            value: '$__all',
            selected: false,
          },
          {
            text: 'gdev-prometheus',
            value: 'gdev-prometheus',
            selected: true,
          },
          {
            text: 'gdev-slow-prometheus',
            value: 'gdev-slow-prometheus',
            selected: false,
          },
        ],
        query: 'prometheus',
        multi: true,
        includeAll: true,
        refresh: 1,
        allValue: 'Custom all',
      };

      const migrated = createSceneVariableFromVariableModel(variable);
      const { key, ...rest } = migrated.state;

      expect(migrated).toBeInstanceOf(DataSourceVariable);
      expect(rest).toEqual({
        allValue: 'Custom all',
        defaultToAll: true,
        includeAll: true,
        label: undefined,
        name: 'query1',
        options: [],
        pluginId: 'prometheus',
        regex: '/^gdev/',
        skipUrlSync: false,
        text: ['gdev-prometheus', 'gdev-slow-prometheus'],
        type: 'datasource',
        value: ['gdev-prometheus', 'gdev-slow-prometheus'],
        isMulti: true,
        description: null,
        hide: 0,
      });
    });

    it('should migrate constant variable', () => {
      const variable = {
        hide: 2,
        label: 'constant',
        name: 'constant',
        skipUrlSync: false,
        type: 'constant' as VariableType,
        rootStateKey: 'N4XLmH5Vz',
        current: {
          selected: true,
          text: 'test',
          value: 'test',
        },
        options: [
          {
            selected: true,
            text: 'test',
            value: 'test',
          },
        ],
        query: 'test',
        id: 'constant',
        global: false,
        index: 3,
        state: 'Done',
        error: null,
        description: null,
      };

      const migrated = createSceneVariableFromVariableModel(variable);
      const { key, ...rest } = migrated.state;

      expect(rest).toEqual({
        description: null,
        hide: 2,
        label: 'constant',
        name: 'constant',
        skipUrlSync: true,
        type: 'constant',
        value: 'test',
      });
    });

    it('should migrate interval variable', () => {
      const variable = {
        name: 'intervalVar',
        label: 'Interval Label',
        type: 'interval' as VariableType,
        rootStateKey: 'N4XLmH5Vz',
        auto: false,
        refresh: 2,
        auto_count: 30,
        auto_min: '10s',
        current: {
          selected: true,
          text: '1m',
          value: '1m',
        },
        options: [
          {
            selected: true,
            text: '1m',
            value: '1m',
          },
        ],
        query: '1m, 5m, 15m, 30m, 1h, 6h, 12h, 1d, 7d, 14d, 30d',
        id: 'intervalVar',
        global: false,
        index: 4,
        hide: 0,
        skipUrlSync: false,
        state: 'Done',
        error: null,
        description: null,
      };
      const migrated = createSceneVariableFromVariableModel(variable);
      const { key, ...rest } = migrated.state;
      expect(rest).toEqual({
        label: 'Interval Label',
        autoEnabled: false,
        autoMinInterval: '10s',
        autoStepCount: 30,
        description: null,
        refresh: 2,
        intervals: ['1m', '5m', '15m', '30m', '1h', '6h', '12h', '1d', '7d', '14d', '30d'],
        hide: 0,
        name: 'intervalVar',
        skipUrlSync: false,
        type: 'interval',
        value: '1m',
      });
    });
    it.each(['textbox', 'system'])('should throw for unsupported (yet) variables', (type) => {
      const variable = {
        name: 'query0',
        type: type as VariableType,
      };

      expect(() => createSceneVariableFromVariableModel(variable)).toThrow();
    });
  });

  describe('Repeating rows', () => {
    it('Should build correct scene model', () => {
      const scene = transformSaveModelToScene({ dashboard: repeatingRowsAndPanelsDashboardJson as any, meta: {} });
      const body = scene.state.body as SceneGridLayout;
      const row2 = body.state.children[1] as SceneGridRow;

      expect(row2.state.$behaviors?.[0]).toBeInstanceOf(RowRepeaterBehavior);

      const repeatBehavior = row2.state.$behaviors?.[0] as RowRepeaterBehavior;
      expect(repeatBehavior.state.variableName).toBe('server');

      const lastRow = body.state.children[body.state.children.length - 1] as SceneGridRow;
      expect(lastRow.state.isCollapsed).toBe(true);
    });
  });

  describe('Annotation queries', () => {
    it('Should build correct scene model', () => {
      const scene = transformSaveModelToScene({ dashboard: dashboard_to_load1 as any, meta: {} });

      expect(scene.state.$data).toBeInstanceOf(SceneDataLayers);
      expect(scene.state.controls![2]).toBeInstanceOf(SceneDataLayerControls);

      const dataLayers = scene.state.$data as SceneDataLayers;
      expect(dataLayers.state.layers).toHaveLength(4);
      expect(dataLayers.state.layers[0].state.name).toBe('Annotations & Alerts');
      expect(dataLayers.state.layers[0].state.isEnabled).toBe(true);
      expect(dataLayers.state.layers[0].state.isHidden).toBe(false);

      expect(dataLayers.state.layers[1].state.name).toBe('Enabled');
      expect(dataLayers.state.layers[1].state.isEnabled).toBe(true);
      expect(dataLayers.state.layers[1].state.isHidden).toBe(false);

      expect(dataLayers.state.layers[2].state.name).toBe('Disabled');
      expect(dataLayers.state.layers[2].state.isEnabled).toBe(false);
      expect(dataLayers.state.layers[2].state.isHidden).toBe(false);

      expect(dataLayers.state.layers[3].state.name).toBe('Hidden');
      expect(dataLayers.state.layers[3].state.isEnabled).toBe(true);
      expect(dataLayers.state.layers[3].state.isHidden).toBe(true);
    });
  });
});

function buildGridItemForTest(saveModel: Partial<Panel>): { gridItem: SceneGridItem; vizPanel: VizPanel } {
  const gridItem = buildGridItemForPanel(new PanelModel(saveModel));
  if (gridItem instanceof SceneGridItem) {
    return { gridItem, vizPanel: gridItem.state.body as VizPanel };
  }

  throw new Error('buildGridItemForPanel to return SceneGridItem');
}
