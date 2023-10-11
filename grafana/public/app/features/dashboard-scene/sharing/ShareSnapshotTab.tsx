import React from 'react';
import useAsyncFn from 'react-use/lib/useAsyncFn';

import { SelectableValue } from '@grafana/data';
import { getBackendSrv } from '@grafana/runtime';
import { SceneComponentProps, sceneGraph, SceneObjectBase, SceneObjectRef, VizPanel } from '@grafana/scenes';
import { Button, ClipboardButton, Field, Input, Modal, RadioButtonGroup } from '@grafana/ui';
import { t, Trans } from 'app/core/internationalization';
import { trackDashboardSharingActionPerType } from 'app/features/dashboard/components/ShareModal/analytics';
import { shareDashboardType } from 'app/features/dashboard/components/ShareModal/utils';

import { DashboardScene } from '../scene/DashboardScene';
import { transformSceneToSaveModel, trimDashboardForSnapshot } from '../serialization/transformSceneToSaveModel';

import { SceneShareTabState } from './types';

const SNAPSHOTS_API_ENDPOINT = '/api/snapshots';
const DEFAULT_EXPIRE_OPTION: SelectableValue<number> = {
  label: t('share-modal.snapshot.expire-never', `Never`),
  value: 0,
};

const EXPIRE_OPTIONS = [
  DEFAULT_EXPIRE_OPTION,
  {
    label: t('share-modal.snapshot.expire-hour', `1 Hour`),
    value: 60 * 60,
  },
  {
    label: t('share-modal.snapshot.expire-day', `1 Day`),
    value: 60 * 60 * 24,
  },
  {
    label: t('share-modal.snapshot.expire-week', `7 Days`),
    value: 60 * 60 * 24 * 7,
  },
];

type SnapshotSharingOptions = {
  externalEnabled: boolean;
  externalSnapshotName: string;
  externalSnapshotURL: string;
  snapshotEnabled: boolean;
};
export interface ShareSnapshotTabState extends SceneShareTabState {
  panelRef?: SceneObjectRef<VizPanel>;
  dashboardRef: SceneObjectRef<DashboardScene>;
  snapshotName?: string;
  selectedExpireOption?: SelectableValue<number>;

  snapshotSharingOptions?: SnapshotSharingOptions;
}

export class ShareSnapshotTab extends SceneObjectBase<ShareSnapshotTabState> {
  static Component = ShareSnapshoTabRenderer;

  public constructor(state: ShareSnapshotTabState) {
    super({
      ...state,
      snapshotName: state.dashboardRef.resolve().state.title,
      selectedExpireOption: DEFAULT_EXPIRE_OPTION,
    });

    this.addActivationHandler(() => {
      this._onActivate();
    });
  }

  private _onActivate() {
    getBackendSrv()
      .get('/api/snapshot/shared-options')
      .then((shareOptions: SnapshotSharingOptions) => {
        if (this.isActive) {
          this.setState({
            snapshotSharingOptions: shareOptions,
          });
        }
      });
  }

  public getTabLabel() {
    return t('share-modal.tab-title.snapshot', 'Snapshot');
  }

  public onSnasphotNameChange = (snapshotName: string) => {
    this.setState({ snapshotName: snapshotName.trim() });
  };

  public onExpireChange = (option: number) => {
    this.setState({
      selectedExpireOption: EXPIRE_OPTIONS.find((o) => o.value === option),
    });
  };

  private prepareSnapshot() {
    const timeRange = sceneGraph.getTimeRange(this);
    const { dashboardRef, panelRef } = this.state;
    const saveModel = transformSceneToSaveModel(dashboardRef.resolve(), true);

    return trimDashboardForSnapshot(
      this.state.snapshotName || '',
      timeRange.state.value,
      saveModel,
      panelRef?.resolve()
    );
  }

  public onSnapshotCreate = async (external = false) => {
    const { selectedExpireOption } = this.state;
    const snapshot = this.prepareSnapshot();

    // TODO
    // snapshot.snapshot = {
    //   originalUrl: window.location.href,
    // };

    const cmdData = {
      dashboard: snapshot,
      name: snapshot.title,
      expires: selectedExpireOption?.value,
      external,
    };

    try {
      const results: { deleteUrl: string; url: string } = await getBackendSrv().post(SNAPSHOTS_API_ENDPOINT, cmdData);
      return results;
    } finally {
      trackDashboardSharingActionPerType(external ? 'publish_snapshot' : 'local_snapshot', shareDashboardType.snapshot);
    }
  };
}

function ShareSnapshoTabRenderer({ model }: SceneComponentProps<ShareSnapshotTab>) {
  const { snapshotName, selectedExpireOption, modalRef, snapshotSharingOptions } = model.useState();

  const [snapshotResult, createSnapshot] = useAsyncFn(async (external = false) => {
    return model.onSnapshotCreate(external);
  });

  const [deleteSnapshotResult, deleteSnapshot] = useAsyncFn(async (url: string) => {
    return await getBackendSrv().get(url);
  });

  // If snapshot has been deleted - show message and allow to close modal
  if (deleteSnapshotResult.value) {
    return (
      <Trans i18nKey="share-modal.snapshot.deleted-message">
        The snapshot has been deleted. If you have already accessed it once, then it might take up to an hour before
        before it is removed from browser caches or CDN caches.
      </Trans>
    );
  }

  return (
    <>
      {/* Before snapshot has been created show configuration  */}
      {!Boolean(snapshotResult.value) && (
        <>
          <div>
            <p className="share-modal-info-text">
              <Trans i18nKey="share-modal.snapshot.info-text-1">
                A snapshot is an instant way to share an interactive dashboard publicly. When created, we strip
                sensitive data like queries (metric, template, and annotation) and panel links, leaving only the visible
                metric data and series names embedded in your dashboard.
              </Trans>
            </p>
            <p className="share-modal-info-text">
              <Trans i18nKey="share-modal.snapshot.info-text-2">
                Keep in mind, your snapshot <em>can be viewed by anyone</em> that has the link and can access the URL.
                Share wisely.
              </Trans>
            </p>
          </div>

          <Field label={t('share-modal.snapshot.name', `Snapshot name`)}>
            <Input
              id="snapshot-name-input"
              width={30}
              defaultValue={snapshotName}
              onBlur={(e) => model.onSnasphotNameChange(e.target.value)}
            />
          </Field>

          <Field label={t('share-modal.snapshot.expire', `Expire`)}>
            <RadioButtonGroup<number>
              id="expire-select-input"
              options={EXPIRE_OPTIONS}
              value={selectedExpireOption?.value}
              onChange={model.onExpireChange}
            />
          </Field>

          <Modal.ButtonRow>
            <Button
              variant="secondary"
              onClick={() => {
                modalRef?.resolve().onDismiss();
              }}
              fill="outline"
            >
              <Trans i18nKey="share-modal.snapshot.cancel-button">Cancel</Trans>
            </Button>

            {snapshotSharingOptions?.externalEnabled && (
              <Button variant="secondary" disabled={snapshotResult.loading} onClick={() => createSnapshot(true)}>
                {snapshotSharingOptions?.externalSnapshotName}
              </Button>
            )}
            <Button variant="primary" disabled={snapshotResult.loading} onClick={() => createSnapshot()}>
              <Trans i18nKey="share-modal.snapshot.local-button">Local Snapshot</Trans>
            </Button>
          </Modal.ButtonRow>
        </>
      )}

      {/* When snapshot has been created - show link and allow copy/deletion */}
      {snapshotResult.value && (
        <>
          <Field label={t('share-modal.snapshot.url-label', 'Snapshot URL')}>
            <Input
              id="snapshot-url-input"
              value={snapshotResult.value.url}
              readOnly
              addonAfter={
                <ClipboardButton icon="copy" variant="primary" getText={() => snapshotResult.value!.url}>
                  <Trans i18nKey="share-modal.snapshot.copy-link-button">Copy</Trans>
                </ClipboardButton>
              }
            />
          </Field>

          <div className="pull-right" style={{ padding: '5px' }}>
            <Trans i18nKey="share-modal.snapshot.mistake-message">Did you make a mistake? </Trans>&nbsp;
            <Button
              fill="outline"
              size="md"
              variant="destructive"
              onClick={() => {
                deleteSnapshot(snapshotResult.value!.deleteUrl);
              }}
            >
              <Trans i18nKey="share-modal.snapshot.delete-button">Delete snapshot.</Trans>
            </Button>
          </div>
        </>
      )}
    </>
  );
}
