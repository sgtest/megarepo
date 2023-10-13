import { Unsubscribable } from 'rxjs';

import { AppEvents } from '@grafana/data';
import { locationService } from '@grafana/runtime';
import { SceneObjectUrlSyncHandler, SceneObjectUrlValues } from '@grafana/scenes';
import appEvents from 'app/core/app_events';

import { PanelInspectDrawer } from '../inspect/PanelInspectDrawer';
import { findVizPanelByKey } from '../utils/utils';

import { DashboardScene, DashboardSceneState } from './DashboardScene';
import { DashboardRepeatsProcessedEvent } from './types';

export class DashboardSceneUrlSync implements SceneObjectUrlSyncHandler {
  private _eventSub?: Unsubscribable;

  constructor(private _scene: DashboardScene) {}

  getKeys(): string[] {
    return ['inspect', 'viewPanel'];
  }

  getUrlState(): SceneObjectUrlValues {
    const state = this._scene.state;
    return { inspect: state.inspectPanelKey, viewPanel: state.viewPanelKey };
  }

  updateFromUrl(values: SceneObjectUrlValues): void {
    const { inspectPanelKey: inspectPanelId, viewPanelKey: viewPanelId } = this._scene.state;
    const update: Partial<DashboardSceneState> = {};

    // Handle inspect object state
    if (typeof values.inspect === 'string') {
      const panel = findVizPanelByKey(this._scene, values.inspect);
      if (!panel) {
        appEvents.emit(AppEvents.alertError, ['Panel not found']);
        locationService.partial({ inspect: null });
        return;
      }

      update.inspectPanelKey = values.inspect;
      update.overlay = new PanelInspectDrawer({ panelRef: panel.getRef() });
    } else if (inspectPanelId) {
      update.inspectPanelKey = undefined;
      update.overlay = undefined;
    }

    // Handle view panel state
    if (typeof values.viewPanel === 'string') {
      const panel = findVizPanelByKey(this._scene, values.viewPanel);
      if (!panel) {
        // // If we are trying to view a repeat clone that can't be found it might be that the repeats have not been processed yet
        if (values.viewPanel.indexOf('clone')) {
          this._handleViewRepeatClone(values.viewPanel);
          return;
        }

        appEvents.emit(AppEvents.alertError, ['Panel not found']);
        locationService.partial({ viewPanel: null });
        return;
      }

      update.viewPanelKey = values.viewPanel;
    } else if (viewPanelId) {
      update.viewPanelKey = undefined;
    }

    if (Object.keys(update).length > 0) {
      this._scene.setState(update);
    }
  }

  private _handleViewRepeatClone(viewPanel: string) {
    if (!this._eventSub) {
      this._eventSub = this._scene.subscribeToEvent(DashboardRepeatsProcessedEvent, () => {
        const panel = findVizPanelByKey(this._scene, viewPanel);
        if (panel) {
          this._eventSub?.unsubscribe();
          this._scene.setState({ viewPanelKey: viewPanel });
        }
      });
    }
  }
}
