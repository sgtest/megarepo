import { DataFrame } from '@grafana/data';
import { DataQuery, DataSourceRef } from '@grafana/schema';
import { backendSrv } from 'app/core/services/backend_srv';
import {
  getNewDashboardModelData,
  setDashboardToFetchFromLocalStorage,
} from 'app/features/dashboard/state/initDashboard';
import { DashboardDTO, ExplorePanelData } from 'app/types';

export enum AddToDashboardError {
  FETCH_DASHBOARD = 'fetch-dashboard',
  SET_DASHBOARD_LS = 'set-dashboard-ls-error',
}

interface AddPanelToDashboardOptions {
  queries: DataQuery[];
  queryResponse: ExplorePanelData;
  datasource?: DataSourceRef;
  dashboardUid?: string;
}

function createDashboard(): DashboardDTO {
  const dto = getNewDashboardModelData();

  // getNewDashboardModelData adds by default the "add-panel" panel. We don't want that.
  dto.dashboard.panels = [];

  return dto;
}

export async function setDashboardInLocalStorage(options: AddPanelToDashboardOptions) {
  const panelType = getPanelType(options.queries, options.queryResponse);
  const panel = {
    targets: options.queries,
    type: panelType,
    title: 'New Panel',
    gridPos: { x: 0, y: 0, w: 12, h: 8 },
    datasource: options.datasource,
  };

  let dto: DashboardDTO;

  if (options.dashboardUid) {
    try {
      dto = await backendSrv.getDashboardByUid(options.dashboardUid);
    } catch (e) {
      throw AddToDashboardError.FETCH_DASHBOARD;
    }
  } else {
    dto = createDashboard();
  }

  dto.dashboard.panels = [panel, ...(dto.dashboard.panels ?? [])];

  try {
    setDashboardToFetchFromLocalStorage(dto);
  } catch {
    throw AddToDashboardError.SET_DASHBOARD_LS;
  }
}

const isVisible = (query: DataQuery) => !query.hide;
const hasRefId = (refId: DataFrame['refId']) => (frame: DataFrame) => frame.refId === refId;

function getPanelType(queries: DataQuery[], queryResponse: ExplorePanelData) {
  for (const { refId } of queries.filter(isVisible)) {
    const hasQueryRefId = hasRefId(refId);
    if (queryResponse.flameGraphFrames.some(hasQueryRefId)) {
      return 'flamegraph';
    }
    if (queryResponse.graphFrames.some(hasQueryRefId)) {
      return 'timeseries';
    }
    if (queryResponse.logsFrames.some(hasQueryRefId)) {
      return 'logs';
    }
    if (queryResponse.nodeGraphFrames.some(hasQueryRefId)) {
      return 'nodeGraph';
    }
    if (queryResponse.traceFrames.some(hasQueryRefId)) {
      return 'traces';
    }
    if (queryResponse.customFrames.some(hasQueryRefId)) {
      // we will always have a custom frame and meta, it should never default to 'table' (but all paths must return a string)
      return queryResponse.customFrames.find(hasQueryRefId)?.meta?.preferredVisualisationPluginId ?? 'table';
    }
  }

  // falling back to table
  return 'table';
}
