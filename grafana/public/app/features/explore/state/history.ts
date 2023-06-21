import { AnyAction, createAction } from '@reduxjs/toolkit';

import { HistoryItem } from '@grafana/data';
import { DataQuery } from '@grafana/schema';
import {
  addToRichHistory,
  deleteAllFromRichHistory,
  deleteQueryInRichHistory,
  getRichHistory,
  getRichHistorySettings,
  updateCommentInRichHistory,
  updateRichHistorySettings,
  updateStarredInRichHistory,
} from 'app/core/utils/richHistory';
import { ExploreItemState, ExploreState, RichHistoryQuery, ThunkResult } from 'app/types';

import { supportedFeatures } from '../../../core/history/richHistoryStorageProvider';
import { RichHistorySearchFilters, RichHistorySettings } from '../../../core/utils/richHistoryTypes';

import {
  richHistoryLimitExceededAction,
  richHistorySearchFiltersUpdatedAction,
  richHistorySettingsUpdatedAction,
  richHistoryStorageFullAction,
  richHistoryUpdatedAction,
} from './main';
import { selectPanesEntries } from './selectors';

//
// Actions and Payloads
//

export interface HistoryUpdatedPayload {
  exploreId: string;
  history: HistoryItem[];
}
export const historyUpdatedAction = createAction<HistoryUpdatedPayload>('explore/historyUpdated');

//
// Action creators
//

type SyncHistoryUpdatesOptions = {
  updatedQuery?: RichHistoryQuery;
  deletedId?: string;
};

/**
 * Updates current state in both Explore panes after changing or deleting a query history item
 */
const updateRichHistoryState = ({ updatedQuery, deletedId }: SyncHistoryUpdatesOptions): ThunkResult<void> => {
  return async (dispatch, getState) => {
    forEachExplorePane(getState().explore, (item, exploreId) => {
      const newRichHistory = item.richHistory
        // update
        .map((query) => (query.id === updatedQuery?.id ? updatedQuery : query))
        // or remove
        .filter((query) => query.id !== deletedId);
      const deletedItems = item.richHistory.length - newRichHistory.length;
      dispatch(
        richHistoryUpdatedAction({
          richHistoryResults: { richHistory: newRichHistory, total: item.richHistoryTotal! - deletedItems },
          exploreId,
        })
      );
    });
  };
};

const forEachExplorePane = (state: ExploreState, callback: (item: ExploreItemState, exploreId: string) => void) => {
  Object.entries(state.panes).forEach(([exploreId, item]) => {
    item && callback(item, exploreId);
  });
};

export const addHistoryItem = (
  datasourceUid: string,
  datasourceName: string,
  queries: DataQuery[]
): ThunkResult<void> => {
  return async (dispatch, getState) => {
    const { richHistoryStorageFull, limitExceeded } = await addToRichHistory(
      datasourceUid,
      datasourceName,
      queries,
      false,
      '',
      !getState().explore.richHistoryStorageFull,
      !getState().explore.richHistoryLimitExceededWarningShown
    );
    if (richHistoryStorageFull) {
      dispatch(richHistoryStorageFullAction());
    }
    if (limitExceeded) {
      dispatch(richHistoryLimitExceededAction());
    }
  };
};

export const starHistoryItem = (id: string, starred: boolean): ThunkResult<void> => {
  return async (dispatch, getState) => {
    const updatedQuery = await updateStarredInRichHistory(id, starred);
    dispatch(updateRichHistoryState({ updatedQuery }));
  };
};

export const commentHistoryItem = (id: string, comment?: string): ThunkResult<void> => {
  return async (dispatch) => {
    const updatedQuery = await updateCommentInRichHistory(id, comment);
    dispatch(updateRichHistoryState({ updatedQuery }));
  };
};

export const deleteHistoryItem = (id: string): ThunkResult<void> => {
  return async (dispatch) => {
    const deletedId = await deleteQueryInRichHistory(id);
    dispatch(updateRichHistoryState({ deletedId }));
  };
};

export const deleteRichHistory = (): ThunkResult<void> => {
  return async (dispatch, getState) => {
    await deleteAllFromRichHistory();
    selectPanesEntries(getState()).forEach(([exploreId]) => {
      dispatch(richHistoryUpdatedAction({ richHistoryResults: { richHistory: [], total: 0 }, exploreId }));
      dispatch(richHistoryUpdatedAction({ richHistoryResults: { richHistory: [], total: 0 }, exploreId }));
    });
  };
};

export const loadRichHistory = (exploreId: string): ThunkResult<void> => {
  return async (dispatch, getState) => {
    const filters = getState().explore.panes[exploreId]!.richHistorySearchFilters;
    if (filters) {
      const richHistoryResults = await getRichHistory(filters);
      dispatch(richHistoryUpdatedAction({ richHistoryResults, exploreId }));
    }
  };
};

export const loadMoreRichHistory = (exploreId: string): ThunkResult<void> => {
  return async (dispatch, getState) => {
    const currentFilters = getState().explore.panes[exploreId]?.richHistorySearchFilters;
    const currentRichHistory = getState().explore.panes[exploreId]?.richHistory;
    if (currentFilters && currentRichHistory) {
      const nextFilters = { ...currentFilters, page: (currentFilters?.page || 1) + 1 };
      const moreRichHistory = await getRichHistory(nextFilters);
      const richHistory = [...currentRichHistory, ...moreRichHistory.richHistory];
      dispatch(richHistorySearchFiltersUpdatedAction({ filters: nextFilters, exploreId }));
      dispatch(
        richHistoryUpdatedAction({ richHistoryResults: { richHistory, total: moreRichHistory.total }, exploreId })
      );
    }
  };
};

export const clearRichHistoryResults = (exploreId: string): ThunkResult<void> => {
  return async (dispatch) => {
    dispatch(richHistorySearchFiltersUpdatedAction({ filters: undefined, exploreId }));
    dispatch(richHistoryUpdatedAction({ richHistoryResults: { richHistory: [], total: 0 }, exploreId }));
  };
};

/**
 * Initialize query history pane. To load history it requires settings to be loaded first
 * (but only once per session). Filters are initialised by the tab (starred or home).
 */
export const initRichHistory = (): ThunkResult<void> => {
  return async (dispatch, getState) => {
    let settings = getState().explore.richHistorySettings;
    if (!settings) {
      settings = await getRichHistorySettings();
      dispatch(richHistorySettingsUpdatedAction(settings));
    }
  };
};

export const updateHistorySettings = (settings: RichHistorySettings): ThunkResult<void> => {
  return async (dispatch) => {
    dispatch(richHistorySettingsUpdatedAction(settings));
    await updateRichHistorySettings(settings);
  };
};

/**
 * Assumed this can be called only when settings and filters are initialised
 */
export const updateHistorySearchFilters = (exploreId: string, filters: RichHistorySearchFilters): ThunkResult<void> => {
  return async (dispatch, getState) => {
    await dispatch(richHistorySearchFiltersUpdatedAction({ exploreId, filters: { ...filters } }));
    const currentSettings = getState().explore.richHistorySettings!;
    if (supportedFeatures().lastUsedDataSourcesAvailable) {
      await dispatch(
        updateHistorySettings({
          ...currentSettings,
          lastUsedDatasourceFilters: filters.datasourceFilters,
        })
      );
    }
  };
};

export const historyReducer = (state: ExploreItemState, action: AnyAction): ExploreItemState => {
  if (historyUpdatedAction.match(action)) {
    return {
      ...state,
      history: action.payload.history,
    };
  }
  return state;
};
