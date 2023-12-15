import {
  SceneGridItem,
  SceneGridLayout,
  SceneQueryRunner,
  SceneRefreshPicker,
  SceneTimePicker,
  SceneTimeRange,
  VizPanel,
} from '@grafana/scenes';

import { DashboardControls } from '../scene/DashboardControls';
import { DashboardLinksControls } from '../scene/DashboardLinksControls';
import { DashboardScene, DashboardSceneState } from '../scene/DashboardScene';

import { dashboardSceneGraph } from './dashboardSceneGraph';

describe('dashboardSceneGraph', () => {
  describe('getTimePicker', () => {
    it('should return null if no time picker', () => {
      const scene = buildTestScene({
        controls: [
          new DashboardControls({
            variableControls: [],
            linkControls: new DashboardLinksControls({}),
            timeControls: [],
          }),
        ],
      });

      const timePicker = dashboardSceneGraph.getTimePicker(scene);
      expect(timePicker).toBeNull();
    });

    it('should return time picker', () => {
      const scene = buildTestScene();
      const timePicker = dashboardSceneGraph.getTimePicker(scene);
      expect(timePicker).not.toBeNull();
    });
  });

  describe('getRefreshPicker', () => {
    it('should return null if no refresh picker', () => {
      const scene = buildTestScene({
        controls: [
          new DashboardControls({
            variableControls: [],
            linkControls: new DashboardLinksControls({}),
            timeControls: [],
          }),
        ],
      });

      const refreshPicker = dashboardSceneGraph.getRefreshPicker(scene);
      expect(refreshPicker).toBeNull();
    });

    it('should return refresh picker', () => {
      const scene = buildTestScene();
      const refreshPicker = dashboardSceneGraph.getRefreshPicker(scene);
      expect(refreshPicker).not.toBeNull();
    });
  });

  describe('getDashboardControls', () => {
    it('should return null if no dashboard controls', () => {
      const scene = buildTestScene({ controls: [] });

      const dashboardControls = dashboardSceneGraph.getDashboardControls(scene);
      expect(dashboardControls).toBeNull();
    });

    it('should return dashboard controls', () => {
      const scene = buildTestScene();
      const dashboardControls = dashboardSceneGraph.getDashboardControls(scene);
      expect(dashboardControls).not.toBeNull();
    });
  });
});

function buildTestScene(overrides?: Partial<DashboardSceneState>) {
  const scene = new DashboardScene({
    title: 'hello',
    uid: 'dash-1',
    $timeRange: new SceneTimeRange({}),
    controls: [
      new DashboardControls({
        variableControls: [],
        linkControls: new DashboardLinksControls({}),
        timeControls: [
          new SceneTimePicker({}),
          new SceneRefreshPicker({
            intervals: ['1s'],
          }),
        ],
      }),
    ],
    body: new SceneGridLayout({
      children: [
        new SceneGridItem({
          key: 'griditem-1',
          x: 0,
          body: new VizPanel({
            title: 'Panel A',
            key: 'panel-1',
            pluginId: 'table',
            $data: new SceneQueryRunner({ key: 'data-query-runner', queries: [{ refId: 'A' }] }),
          }),
        }),
        new SceneGridItem({
          body: new VizPanel({
            title: 'Panel B',
            key: 'panel-2',
            pluginId: 'table',
          }),
        }),
        new SceneGridItem({
          body: new VizPanel({
            title: 'Panel B',
            key: 'panel-2-clone-1',
            pluginId: 'table',
            $data: new SceneQueryRunner({ key: 'data-query-runner2', queries: [{ refId: 'A' }] }),
          }),
        }),
      ],
    }),
    ...overrides,
  });

  return scene;
}
