import { CoreApp } from '@grafana/data';
import {
  sceneGraph,
  SceneGridItem,
  SceneGridLayout,
  SceneRefreshPicker,
  SceneTimeRange,
  SceneQueryRunner,
  SceneVariableSet,
  TestVariable,
  VizPanel,
} from '@grafana/scenes';
import appEvents from 'app/core/app_events';
import { getDashboardSrv } from 'app/features/dashboard/services/DashboardSrv';
import { VariablesChanged } from 'app/features/variables/types';

import { dashboardSceneGraph } from '../utils/dashboardSceneGraph';
import { djb2Hash } from '../utils/djb2Hash';

import { DashboardControls } from './DashboardControls';
import { DashboardLinksControls } from './DashboardLinksControls';
import { DashboardScene, DashboardSceneState } from './DashboardScene';

describe('DashboardScene', () => {
  describe('DashboardSrv.getCurrent compatibility', () => {
    it('Should set to compatibility wrapper', () => {
      const scene = buildTestScene();
      scene.activate();

      expect(getDashboardSrv().getCurrent()?.uid).toBe('dash-1');
    });
  });

  describe('Editing and discarding', () => {
    describe('Given scene in edit mode', () => {
      let scene: DashboardScene;

      beforeEach(() => {
        scene = buildTestScene();
        scene.onEnterEditMode();
      });

      it('Should set isEditing to true', () => {
        expect(scene.state.isEditing).toBe(true);
      });

      it('A change to griditem pos should set isDirty true', () => {
        const gridItem = sceneGraph.findObject(scene, (p) => p.state.key === 'griditem-1') as SceneGridItem;
        gridItem.setState({ x: 10, y: 0, width: 10, height: 10 });

        expect(scene.state.isDirty).toBe(true);

        scene.onDiscard();
        const gridItem2 = sceneGraph.findObject(scene, (p) => p.state.key === 'griditem-1') as SceneGridItem;
        expect(gridItem2.state.x).toBe(0);
      });

      it.each`
        prop              | value
        ${'title'}        | ${'new title'}
        ${'description'}  | ${'new description'}
        ${'tags'}         | ${['tag1', 'tag2']}
        ${'editable'}     | ${false}
        ${'graphTooltip'} | ${1}
      `(
        'A change to $prop should set isDirty true',
        ({ prop, value }: { prop: keyof DashboardSceneState; value: any }) => {
          scene.setState({ [prop]: value });

          expect(scene.state.isDirty).toBe(true);

          // TODO: Discard doesn't restore the previous state
          // scene.onDiscard();
          // expect(scene.state[prop]).toBe(prevState);
        }
      );

      // TODO: Make the dashboard to restore the defaults on discard
      it.skip('A change to refresh picker interval settings should set isDirty true', () => {
        const refreshPicker = dashboardSceneGraph.getRefreshPicker(scene)!;
        const prevState = refreshPicker.state.intervals;
        refreshPicker.setState({ intervals: ['10s'] });

        expect(scene.state.isDirty).toBe(true);

        scene.onDiscard();
        expect(refreshPicker.state.intervals).toEqual(prevState);
      });

      // TODO: Make the dashboard to restore the defaults on discard
      it.skip('A change to time zone should set isDirty true', () => {
        const timeRange = scene.state.$timeRange!;
        const prevState = timeRange.state.timeZone;
        timeRange.setState({ timeZone: 'UTC' });

        expect(scene.state.isDirty).toBe(true);

        scene.onDiscard();
        expect(timeRange.state.timeZone).toBe(prevState);
      });
    });
  });

  describe('Enriching data requests', () => {
    let scene: DashboardScene;

    beforeEach(() => {
      scene = buildTestScene();
      scene.onEnterEditMode();
    });

    it('Should add app, uid, and panelId', () => {
      const queryRunner = sceneGraph.findObject(scene, (o) => o.state.key === 'data-query-runner')!;
      expect(scene.enrichDataRequest(queryRunner)).toEqual({
        app: CoreApp.Dashboard,
        dashboardUID: 'dash-1',
        panelId: 1,
      });
    });

    it('Should hash the key of the cloned panels and set it as panelId', () => {
      const queryRunner = sceneGraph.findObject(scene, (o) => o.state.key === 'data-query-runner2')!;
      const expectedPanelId = djb2Hash('panel-2-clone-1');
      expect(scene.enrichDataRequest(queryRunner).panelId).toEqual(expectedPanelId);
    });
  });

  describe('When variables change', () => {
    it('A change to griditem pos should set isDirty true', () => {
      const varA = new TestVariable({ name: 'A', query: 'A.*', value: 'A.AA', text: '', options: [], delayMs: 0 });
      const scene = buildTestScene({
        $variables: new SceneVariableSet({ variables: [varA] }),
      });

      scene.activate();

      const eventHandler = jest.fn();
      appEvents.subscribe(VariablesChanged, eventHandler);

      varA.changeValueTo('A.AB');

      expect(eventHandler).toHaveBeenCalledTimes(1);
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
