import { StateManagerBase } from 'app/core/services/StateManagerBase';
import { dashboardLoaderSrv } from 'app/features/dashboard/services/DashboardLoaderSrv';
import { getDashboardSrv } from 'app/features/dashboard/services/DashboardSrv';

import { buildPanelEditScene, PanelEditor } from '../panel-edit/PanelEditor';
import { DashboardScene } from '../scene/DashboardScene';
import { transformSaveModelToScene } from '../serialization/transformSaveModelToScene';
import { getVizPanelKeyForPanelId, findVizPanelByKey } from '../utils/utils';

export interface DashboardScenePageState {
  dashboard?: DashboardScene;
  panelEditor?: PanelEditor;
  isLoading?: boolean;
  loadError?: string;
}

export class DashboardScenePageStateManager extends StateManagerBase<DashboardScenePageState> {
  private cache: Record<string, DashboardScene> = {};

  public async loadDashboard(uid: string) {
    try {
      const dashboard = await this.loadScene(uid);
      dashboard.startUrlSync();

      this.setState({ dashboard: dashboard, isLoading: false });
    } catch (err) {
      this.setState({ isLoading: false, loadError: String(err) });
    }
  }

  public async loadPanelEdit(uid: string, panelId: string) {
    try {
      const dashboard = await this.loadScene(uid);
      const panel = findVizPanelByKey(dashboard, getVizPanelKeyForPanelId(parseInt(panelId, 10)));

      if (!panel) {
        this.setState({ isLoading: false, loadError: 'Panel not found' });
        return;
      }

      const panelEditor = buildPanelEditScene(dashboard, panel);
      panelEditor.startUrlSync();

      this.setState({ isLoading: false, panelEditor });
    } catch (err) {
      this.setState({ isLoading: false, loadError: String(err) });
    }
  }

  private async loadScene(uid: string): Promise<DashboardScene> {
    const fromCache = this.cache[uid];
    if (fromCache) {
      return fromCache;
    }

    this.setState({ isLoading: true });

    const rsp = await dashboardLoaderSrv.loadDashboard('db', '', uid);

    if (rsp.dashboard) {
      const scene = transformSaveModelToScene(rsp);
      this.cache[uid] = scene;
      return scene;
    }

    throw new Error('Dashboard not found');
  }

  public clearState() {
    getDashboardSrv().setCurrent(undefined);
    this.setState({ dashboard: undefined, loadError: undefined, isLoading: false, panelEditor: undefined });
  }
}

let stateManager: DashboardScenePageStateManager | null = null;

export function getDashboardScenePageStateManager(): DashboardScenePageStateManager {
  if (!stateManager) {
    stateManager = new DashboardScenePageStateManager({});
  }

  return stateManager;
}
