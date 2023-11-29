import { SceneObjectUrlSyncHandler, SceneObjectUrlValues } from '@grafana/scenes';

import { DashboardLinksEditView, DashboardLinksEditViewState } from './DashboardLinksEditView';

export class EditListViewSceneUrlSync implements SceneObjectUrlSyncHandler {
  constructor(private _scene: DashboardLinksEditView) {}

  getKeys(): string[] {
    return ['editIndex'];
  }

  getUrlState(): SceneObjectUrlValues {
    const state = this._scene.state;
    return {
      editIndex: state.editIndex !== undefined ? String(state.editIndex) : undefined,
    };
  }

  updateFromUrl(values: SceneObjectUrlValues): void {
    let update: Partial<DashboardLinksEditViewState> = {};
    if (typeof values.editIndex === 'string') {
      update = { editIndex: Number(values.editIndex) };
    } else {
      update = { editIndex: undefined };
    }

    if (Object.keys(update).length > 0) {
      this._scene.setState(update);
    }
  }
}
