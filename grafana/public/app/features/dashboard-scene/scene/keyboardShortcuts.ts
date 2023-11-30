import { locationService } from '@grafana/runtime';
import { sceneGraph, VizPanel } from '@grafana/scenes';
import { OptionsWithLegend } from '@grafana/schema';
import { KeybindingSet } from 'app/core/services/KeybindingSet';

import { ShareModal } from '../sharing/ShareModal';
import { dashboardSceneGraph } from '../utils/dashboardSceneGraph';
import { getDashboardUrl, getInspectUrl, getViewPanelUrl, tryGetExploreUrlForPanel } from '../utils/urlBuilders';
import { getPanelIdForVizPanel } from '../utils/utils';

import { DashboardScene } from './DashboardScene';

export function setupKeyboardShortcuts(scene: DashboardScene) {
  const keybindings = new KeybindingSet();

  // View panel
  keybindings.addBinding({
    key: 'v',
    onTrigger: withFocusedPanel(scene, (vizPanel: VizPanel) => {
      if (!scene.state.viewPanelScene) {
        locationService.push(getViewPanelUrl(vizPanel));
      }
    }),
  });

  // Panel edit
  keybindings.addBinding({
    key: 'e',
    onTrigger: withFocusedPanel(scene, async (vizPanel: VizPanel) => {
      const sceneRoot = vizPanel.getRoot();
      if (sceneRoot instanceof DashboardScene) {
        const panelId = getPanelIdForVizPanel(vizPanel);
        locationService.push(
          getDashboardUrl({
            uid: sceneRoot.state.uid,
            subPath: `/panel-edit/${panelId}`,
            currentQueryParams: location.search,
          })
        );
      }
    }),
  });

  // Panel share
  keybindings.addBinding({
    key: 'p s',
    onTrigger: withFocusedPanel(scene, async (vizPanel: VizPanel) => {
      scene.showModal(new ShareModal({ panelRef: vizPanel.getRef(), dashboardRef: scene.getRef() }));
    }),
  });

  // Panel inspect
  keybindings.addBinding({
    key: 'i',
    onTrigger: withFocusedPanel(scene, async (vizPanel: VizPanel) => {
      locationService.push(getInspectUrl(vizPanel));
    }),
  });

  // Got to Explore for panel
  keybindings.addBinding({
    key: 'p x',
    onTrigger: withFocusedPanel(scene, async (vizPanel: VizPanel) => {
      const url = await tryGetExploreUrlForPanel(vizPanel);
      if (url) {
        locationService.push(url);
      }
    }),
  });

  // Toggle legend
  keybindings.addBinding({
    key: 'p l',
    onTrigger: withFocusedPanel(scene, toggleVizPanelLegend),
  });

  // Refresh
  keybindings.addBinding({
    key: 'd r',
    onTrigger: () => sceneGraph.getTimeRange(scene).onRefresh(),
  });

  // Zoom out
  keybindings.addBinding({
    key: 't z',
    onTrigger: () => {
      handleZoomOut(scene);
    },
  });
  keybindings.addBinding({
    key: 'ctrl+z',
    onTrigger: () => {
      handleZoomOut(scene);
    },
  });

  keybindings.addBinding({
    key: 't left',
    onTrigger: () => {
      handleTimeRangeShift(scene, 'left');
    },
  });

  keybindings.addBinding({
    key: 't right',
    onTrigger: () => {
      handleTimeRangeShift(scene, 'right');
    },
  });

  // Dashboard settings
  keybindings.addBinding({
    key: 'd s',
    onTrigger: scene.onOpenSettings,
  });

  // toggle all panel legends (TODO)
  // delete panel (TODO when we work on editing)
  // toggle all exemplars (TODO)
  // collapse all rows (TODO)
  // expand all rows (TODO)

  return () => keybindings.removeAll;
}

export function withFocusedPanel(scene: DashboardScene, fn: (vizPanel: VizPanel) => void) {
  return () => {
    const elements = document.querySelectorAll(':hover');

    for (let i = elements.length - 1; i > 0; i--) {
      const element = elements[i];

      if (element instanceof HTMLElement && element.dataset?.vizPanelKey) {
        const panelKey = element.dataset?.vizPanelKey;
        const vizPanel = sceneGraph.findObject(scene, (o) => o.state.key === panelKey);

        if (vizPanel && vizPanel instanceof VizPanel) {
          fn(vizPanel);
          return;
        }
      }
    }
  };
}

export function toggleVizPanelLegend(vizPanel: VizPanel) {
  const options = vizPanel.state.options;
  if (hasLegendOptions(options) && typeof options.legend.showLegend === 'boolean') {
    vizPanel.onOptionsChange({
      legend: {
        showLegend: options.legend.showLegend ? false : true,
      },
    });
  }
}

function hasLegendOptions(optionsWithLegend: unknown): optionsWithLegend is OptionsWithLegend {
  return optionsWithLegend != null && typeof optionsWithLegend === 'object' && 'legend' in optionsWithLegend;
}

function handleZoomOut(scene: DashboardScene) {
  const timePicker = dashboardSceneGraph.getTimePicker(scene);
  timePicker?.onZoom();
}

function handleTimeRangeShift(scene: DashboardScene, direction: 'left' | 'right') {
  const timePicker = dashboardSceneGraph.getTimePicker(scene);

  if (!timePicker) {
    return;
  }

  if (direction === 'left') {
    timePicker.onMoveBackward();
  }
  if (direction === 'right') {
    timePicker.onMoveForward();
  }
}
