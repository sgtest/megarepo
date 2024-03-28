import { css } from '@emotion/css';
import React, { useEffect, useState } from 'react';

import { GrafanaTheme2 } from '@grafana/data';
import { selectors } from '@grafana/e2e-selectors';
import { config, locationService } from '@grafana/runtime';
import { Button, ButtonGroup, Dropdown, Icon, Menu, ToolbarButton, ToolbarButtonRow, useStyles2 } from '@grafana/ui';
import { AppChromeUpdate } from 'app/core/components/AppChrome/AppChromeUpdate';
import { NavToolbarSeparator } from 'app/core/components/AppChrome/NavToolbar/NavToolbarSeparator';
import { contextSrv } from 'app/core/core';
import { Trans, t } from 'app/core/internationalization';
import { getDashboardSrv } from 'app/features/dashboard/services/DashboardSrv';
import { playlistSrv } from 'app/features/playlist/PlaylistSrv';

import { PanelEditor } from '../panel-edit/PanelEditor';
import { ShareModal } from '../sharing/ShareModal';
import { DashboardInteractions } from '../utils/interactions';
import { DynamicDashNavButtonModel, dynamicDashNavActions } from '../utils/registerDynamicDashNavAction';

import { DashboardScene } from './DashboardScene';
import { GoToSnapshotOriginButton } from './GoToSnapshotOriginButton';
import { LibraryVizPanel } from './LibraryVizPanel';

interface Props {
  dashboard: DashboardScene;
}

export const NavToolbarActions = React.memo<Props>(({ dashboard }) => {
  const actions = <ToolbarActions dashboard={dashboard} />;
  return <AppChromeUpdate actions={actions} />;
});

NavToolbarActions.displayName = 'NavToolbarActions';

/**
 * This part is split into a separate component to help test this
 */
export function ToolbarActions({ dashboard }: Props) {
  const {
    isEditing,
    viewPanelScene,
    isDirty,
    uid,
    meta,
    editview,
    editPanel,
    hasCopiedPanel: copiedPanel,
  } = dashboard.useState();
  const { isPlaying } = playlistSrv.useState();

  const canSaveAs = contextSrv.hasEditPermissionInFolders;
  const toolbarActions: ToolbarAction[] = [];
  const buttonWithExtraMargin = useStyles2(getStyles);
  const isEditingPanel = Boolean(editPanel);
  const isViewingPanel = Boolean(viewPanelScene);
  const isEditingLibraryPanel = useEditingLibraryPanel(editPanel);
  const hasCopiedPanel = Boolean(copiedPanel);
  // Means we are not in settings view, fullscreen panel or edit panel
  const isShowingDashboard = !editview && !isViewingPanel && !isEditingPanel;
  const isEditingAndShowingDashboard = isEditing && isShowingDashboard;

  if (!isEditingPanel) {
    // This adds the precence indicators in enterprise
    addDynamicActions(toolbarActions, dynamicDashNavActions.left, 'left-actions');
  }

  toolbarActions.push({
    group: 'icon-actions',
    condition: isEditingAndShowingDashboard,
    render: () => (
      <ToolbarButton
        key="add-visualization"
        tooltip={'Add visualization'}
        icon="graph-bar"
        onClick={() => {
          const id = dashboard.onCreateNewPanel();
          DashboardInteractions.toolbarAddButtonClicked({ item: 'add_visualization' });
          locationService.partial({ editPanel: id });
        }}
      />
    ),
  });

  toolbarActions.push({
    group: 'icon-actions',
    condition: isEditingAndShowingDashboard,
    render: () => (
      <ToolbarButton
        key="add-library-panel"
        tooltip={'Add library panel'}
        icon="library-panel"
        onClick={() => {
          dashboard.onCreateLibPanelWidget();
          DashboardInteractions.toolbarAddButtonClicked({ item: 'add_library_panel' });
        }}
      />
    ),
  });

  toolbarActions.push({
    group: 'icon-actions',
    condition: isEditingAndShowingDashboard,
    render: () => (
      <ToolbarButton
        key="add-row"
        tooltip={'Add row'}
        icon="wrap-text"
        onClick={() => {
          dashboard.onCreateNewRow();
          DashboardInteractions.toolbarAddButtonClicked({ item: 'add_row' });
        }}
      />
    ),
  });

  toolbarActions.push({
    group: 'icon-actions',
    condition: isEditingAndShowingDashboard,
    render: () => (
      <ToolbarButton
        key="paste-panel"
        disabled={!hasCopiedPanel}
        tooltip={'Paste panel'}
        icon="copy"
        onClick={() => {
          dashboard.pastePanel();
          DashboardInteractions.toolbarAddButtonClicked({ item: 'paste_panel' });
        }}
      />
    ),
  });

  toolbarActions.push({
    group: 'icon-actions',
    condition: uid && Boolean(meta.canStar) && isShowingDashboard,
    render: () => {
      let desc = meta.isStarred
        ? t('dashboard.toolbar.unmark-favorite', 'Unmark as favorite')
        : t('dashboard.toolbar.mark-favorite', 'Mark as favorite');
      return (
        <ToolbarButton
          tooltip={desc}
          icon={
            <Icon name={meta.isStarred ? 'favorite' : 'star'} size="lg" type={meta.isStarred ? 'mono' : 'default'} />
          }
          key="star-dashboard-button"
          onClick={() => {
            DashboardInteractions.toolbarFavoritesClick();
            dashboard.onStarDashboard();
          }}
        />
      );
    },
  });

  const isDevEnv = config.buildInfo.env === 'development';

  toolbarActions.push({
    group: 'icon-actions',
    condition: isDevEnv && uid && isShowingDashboard,
    render: () => (
      <ToolbarButton
        key="view-in-old-dashboard-button"
        tooltip={'Switch to old dashboard page'}
        icon="apps"
        onClick={() => {
          locationService.partial({ scenes: false });
        }}
      />
    ),
  });

  toolbarActions.push({
    group: 'icon-actions',
    condition: meta.isSnapshot && !isEditing,
    render: () => (
      <GoToSnapshotOriginButton originalURL={dashboard.getInitialSaveModel()?.snapshot?.originalUrl ?? ''} />
    ),
  });

  toolbarActions.push({
    group: 'playlist-actions',
    condition: isPlaying && isShowingDashboard && !isEditing,
    render: () => (
      <ToolbarButton
        key="play-list-prev"
        data-testid={selectors.pages.Dashboard.DashNav.playlistControls.prev}
        tooltip={t('dashboard.toolbar.playlist-previous', 'Go to previous dashboard')}
        icon="backward"
        onClick={() => playlistSrv.prev()}
      />
    ),
  });

  toolbarActions.push({
    group: 'playlist-actions',
    condition: isPlaying && isShowingDashboard && !isEditing,
    render: () => (
      <ToolbarButton
        key="play-list-stop"
        onClick={() => playlistSrv.stop()}
        data-testid={selectors.pages.Dashboard.DashNav.playlistControls.stop}
      >
        <Trans i18nKey="dashboard.toolbar.playlist-stop">Stop playlist</Trans>
      </ToolbarButton>
    ),
  });

  toolbarActions.push({
    group: 'playlist-actions',
    condition: isPlaying && isShowingDashboard && !isEditing,
    render: () => (
      <ToolbarButton
        key="play-list-next"
        data-testid={selectors.pages.Dashboard.DashNav.playlistControls.next}
        tooltip={t('dashboard.toolbar.playlist-next', 'Go to next dashboard')}
        icon="forward"
        onClick={() => playlistSrv.next()}
        narrow
      />
    ),
  });

  if (!isEditingPanel) {
    // This adds the alert rules button and the dashboard insights button
    addDynamicActions(toolbarActions, dynamicDashNavActions.right, 'icon-actions');
  }

  toolbarActions.push({
    group: 'back-button',
    condition: (isViewingPanel || isEditingPanel) && !isEditingLibraryPanel,
    render: () => (
      <Button
        onClick={() => {
          locationService.partial({ viewPanel: null, editPanel: null });
        }}
        tooltip=""
        key="back"
        variant="secondary"
        size="sm"
        icon="arrow-left"
      >
        Back to dashboard
      </Button>
    ),
  });

  toolbarActions.push({
    group: 'back-button',
    condition: Boolean(editview),
    render: () => (
      <Button
        onClick={() => {
          locationService.partial({ editview: null });
        }}
        tooltip=""
        key="back"
        fill="text"
        variant="secondary"
        size="sm"
        icon="arrow-left"
      >
        Back to dashboard
      </Button>
    ),
  });

  toolbarActions.push({
    group: 'main-buttons',
    condition: uid && !isEditing && !meta.isSnapshot && !isPlaying,
    render: () => (
      <Button
        key="share-dashboard-button"
        tooltip={t('dashboard.toolbar.share', 'Share dashboard')}
        size="sm"
        className={buttonWithExtraMargin}
        fill="outline"
        onClick={() => {
          DashboardInteractions.toolbarShareClick();
          dashboard.showModal(new ShareModal({ dashboardRef: dashboard.getRef() }));
        }}
      >
        Share
      </Button>
    ),
  });

  toolbarActions.push({
    group: 'main-buttons',
    condition: !isEditing && dashboard.canEditDashboard() && !isViewingPanel && !isPlaying,
    render: () => (
      <Button
        onClick={() => {
          dashboard.onEnterEditMode();
        }}
        tooltip="Enter edit mode"
        key="edit"
        className={buttonWithExtraMargin}
        variant="primary"
        size="sm"
      >
        Edit
      </Button>
    ),
  });

  toolbarActions.push({
    group: 'settings',
    condition: isEditing && dashboard.canEditDashboard() && isShowingDashboard,
    render: () => (
      <Button
        onClick={() => {
          dashboard.onOpenSettings();
        }}
        tooltip="Dashboard settings"
        fill="text"
        size="sm"
        key="settings"
        variant="secondary"
      >
        Settings
      </Button>
    ),
  });

  toolbarActions.push({
    group: 'main-buttons',
    condition: isEditing && !meta.isNew && isShowingDashboard,
    render: () => (
      <Button
        onClick={() => dashboard.exitEditMode({ skipConfirm: false })}
        tooltip="Exits edit mode and discards unsaved changes"
        size="sm"
        key="discard"
        fill="text"
        variant="primary"
      >
        Exit edit
      </Button>
    ),
  });

  toolbarActions.push({
    group: 'main-buttons',
    condition: isEditingPanel && !isEditingLibraryPanel && !editview && !meta.isNew && !isViewingPanel,
    render: () => (
      <Button
        onClick={editPanel?.onDiscard}
        tooltip="Discard panel changes"
        size="sm"
        key="discard"
        fill="outline"
        variant="destructive"
      >
        Discard panel changes
      </Button>
    ),
  });

  toolbarActions.push({
    group: 'main-buttons',
    condition: isEditingPanel && isEditingLibraryPanel && !editview && !isViewingPanel,
    render: () => (
      <Button
        onClick={editPanel?.onDiscard}
        tooltip="Discard library panel changes"
        size="sm"
        key="discardLibraryPanel"
        fill="outline"
        variant="destructive"
      >
        Discard library panel changes
      </Button>
    ),
  });

  toolbarActions.push({
    group: 'main-buttons',
    condition: isEditingPanel && isEditingLibraryPanel && !editview && !isViewingPanel,
    render: () => (
      <Button
        onClick={editPanel?.onUnlinkLibraryPanel}
        tooltip="Unlink library panel"
        size="sm"
        key="unlinkLibraryPanel"
        fill="outline"
        variant="secondary"
      >
        Unlink library panel
      </Button>
    ),
  });

  toolbarActions.push({
    group: 'main-buttons',
    condition: isEditingPanel && isEditingLibraryPanel && !editview && !isViewingPanel,
    render: () => (
      <Button
        onClick={editPanel?.onSaveLibraryPanel}
        tooltip="Save library panel"
        size="sm"
        key="saveLibraryPanel"
        fill="outline"
        variant="primary"
      >
        Save library panel
      </Button>
    ),
  });

  toolbarActions.push({
    group: 'main-buttons',
    condition: isEditing && !isEditingLibraryPanel && (meta.canSave || canSaveAs),
    render: () => {
      // if we  only can save
      if (meta.isNew) {
        return (
          <Button
            onClick={() => {
              DashboardInteractions.toolbarSaveClick();
              dashboard.openSaveDrawer({});
            }}
            className={buttonWithExtraMargin}
            tooltip="Save changes"
            key="save"
            size="sm"
            variant={'primary'}
          >
            Save dashboard
          </Button>
        );
      }

      // If we only can save as copy
      if (canSaveAs && !meta.canSave && !meta.canMakeEditable) {
        return (
          <Button
            onClick={() => {
              DashboardInteractions.toolbarSaveClick();
              dashboard.openSaveDrawer({ saveAsCopy: true });
            }}
            className={buttonWithExtraMargin}
            tooltip="Save as copy"
            key="save"
            size="sm"
            variant={isDirty ? 'primary' : 'secondary'}
          >
            Save as copy
          </Button>
        );
      }

      // If we can do both save and save as copy we show a button group with dropdown menu
      const menu = (
        <Menu>
          <Menu.Item
            label="Save"
            icon="save"
            onClick={() => {
              DashboardInteractions.toolbarSaveClick();
              dashboard.openSaveDrawer({});
            }}
          />
          <Menu.Item
            label="Save as copy"
            icon="copy"
            onClick={() => {
              DashboardInteractions.toolbarSaveAsClick();
              dashboard.openSaveDrawer({ saveAsCopy: true });
            }}
          />
        </Menu>
      );

      return (
        <ButtonGroup className={buttonWithExtraMargin} key="save">
          <Button
            onClick={() => {
              DashboardInteractions.toolbarSaveClick();
              dashboard.openSaveDrawer({});
            }}
            tooltip="Save changes"
            size="sm"
            variant={isDirty ? 'primary' : 'secondary'}
          >
            Save dashboard
          </Button>
          <Dropdown overlay={menu}>
            <Button icon="angle-down" variant={isDirty ? 'primary' : 'secondary'} size="sm" />
          </Dropdown>
        </ButtonGroup>
      );
    },
  });

  const actionElements: React.ReactNode[] = [];
  let lastGroup = '';

  for (const action of toolbarActions) {
    if (!action.condition) {
      continue;
    }

    if (lastGroup && lastGroup !== action.group) {
      lastGroup && actionElements.push(<NavToolbarSeparator key={`${action.group}-separator`} />);
    }

    actionElements.push(action.render());
    lastGroup = action.group;
  }

  return <ToolbarButtonRow alignment="right">{actionElements}</ToolbarButtonRow>;
}

function addDynamicActions(
  toolbarActions: ToolbarAction[],
  registeredActions: DynamicDashNavButtonModel[],
  group: string
) {
  if (registeredActions.length > 0) {
    for (const action of registeredActions) {
      const props = { dashboard: getDashboardSrv().getCurrent()! };
      if (action.show(props)) {
        const Component = action.component;
        toolbarActions.push({
          group: group,
          condition: true,
          render: () => <Component {...props} key={toolbarActions.length} />,
        });
      }
    }
  }
}

function useEditingLibraryPanel(panelEditor?: PanelEditor) {
  const [isEditingLibraryPanel, setEditingLibraryPanel] = useState<Boolean>(false);

  useEffect(() => {
    if (panelEditor) {
      const unsub = panelEditor.state.vizManager.subscribeToState((vizManagerState) =>
        setEditingLibraryPanel(vizManagerState.sourcePanel.resolve().parent instanceof LibraryVizPanel)
      );
      return () => {
        unsub.unsubscribe();
      };
    }
    setEditingLibraryPanel(false);
    return;
  }, [panelEditor]);

  return isEditingLibraryPanel;
}

interface ToolbarAction {
  group: string;
  condition?: boolean | string;
  render: () => React.ReactNode;
}

function getStyles(theme: GrafanaTheme2) {
  return css({ margin: theme.spacing(0, 0.5) });
}
