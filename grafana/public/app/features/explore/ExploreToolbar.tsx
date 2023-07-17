import { css, cx } from '@emotion/css';
import { pick } from 'lodash';
import React, { RefObject, useMemo } from 'react';
import { shallowEqual } from 'react-redux';

import { DataSourceInstanceSettings, RawTimeRange } from '@grafana/data';
import { reportInteraction } from '@grafana/runtime';
import { defaultIntervals, PageToolbar, RefreshPicker, SetInterval, ToolbarButton, ButtonGroup } from '@grafana/ui';
import { AppChromeUpdate } from 'app/core/components/AppChrome/AppChromeUpdate';
import { createAndCopyShortLink } from 'app/core/utils/shortLinks';
import { DataSourcePicker } from 'app/features/datasources/components/picker/DataSourcePicker';
import { StoreState, useDispatch, useSelector } from 'app/types/store';

import { DashNavButton } from '../dashboard/components/DashNav/DashNavButton';
import { getTimeSrv } from '../dashboard/services/TimeSrv';
import { updateFiscalYearStartMonthForSession, updateTimeZoneForSession } from '../profile/state/reducers';
import { getFiscalYearStartMonth, getTimeZone } from '../profile/state/selectors';

import { ExploreTimeControls } from './ExploreTimeControls';
import { LiveTailButton } from './LiveTailButton';
import { ToolbarExtensionPoint } from './extensions/ToolbarExtensionPoint';
import { changeDatasource } from './state/datasource';
import { splitClose, splitOpen, maximizePaneAction, evenPaneResizeAction } from './state/main';
import { cancelQueries, runQueries, selectIsWaitingForData } from './state/query';
import { isSplit, selectPanesEntries } from './state/selectors';
import { syncTimes, changeRefreshInterval } from './state/time';
import { LiveTailControls } from './useLiveTailControls';

const rotateIcon = css({
  '> div > svg': {
    transform: 'rotate(180deg)',
  },
});

interface Props {
  exploreId: string;
  onChangeTime: (range: RawTimeRange, changedByScanner?: boolean) => void;
  topOfViewRef: RefObject<HTMLDivElement>;
}

export function ExploreToolbar({ exploreId, topOfViewRef, onChangeTime }: Props) {
  const dispatch = useDispatch();

  const splitted = useSelector(isSplit);
  const timeZone = useSelector((state: StoreState) => getTimeZone(state.user));
  const fiscalYearStartMonth = useSelector((state: StoreState) => getFiscalYearStartMonth(state.user));
  const { refreshInterval, datasourceInstance, range, isLive, isPaused, syncedTimes } = useSelector(
    (state: StoreState) => ({
      ...pick(state.explore.panes[exploreId]!, 'refreshInterval', 'datasourceInstance', 'range', 'isLive', 'isPaused'),
      syncedTimes: state.explore.syncedTimes,
    }),
    shallowEqual
  );
  const loading = useSelector(selectIsWaitingForData(exploreId));
  const isLargerPane = useSelector((state: StoreState) => state.explore.largerExploreId === exploreId);
  const showSmallTimePicker = useSelector((state) => splitted || state.explore.panes[exploreId]!.containerWidth < 1210);
  const showSmallDataSourcePicker = useSelector(
    (state) => state.explore.panes[exploreId]!.containerWidth < (splitted ? 700 : 800)
  );

  const panes = useSelector(selectPanesEntries);

  const shouldRotateSplitIcon = useMemo(
    () => (exploreId === panes[0][0] && isLargerPane) || (exploreId === panes[1]?.[0] && !isLargerPane),
    [isLargerPane, exploreId, panes]
  );

  const onCopyShortLink = () => {
    createAndCopyShortLink(global.location.href);
    reportInteraction('grafana_explore_shortened_link_clicked');
  };

  const onChangeDatasource = async (dsSettings: DataSourceInstanceSettings) => {
    dispatch(changeDatasource(exploreId, dsSettings.uid, { importQueries: true }));
  };

  const onRunQuery = (loading = false) => {
    if (loading) {
      return dispatch(cancelQueries(exploreId));
    } else {
      return dispatch(runQueries({ exploreId }));
    }
  };

  const onChangeTimeZone = (timezone: string) => dispatch(updateTimeZoneForSession(timezone));

  const onOpenSplitView = () => {
    dispatch(splitOpen());
    reportInteraction('grafana_explore_split_view_opened', { origin: 'menu' });
  };

  const onCloseSplitView = () => {
    dispatch(splitClose(exploreId));
    reportInteraction('grafana_explore_split_view_closed');
  };

  const onClickResize = () => {
    if (isLargerPane) {
      dispatch(evenPaneResizeAction());
    } else {
      dispatch(maximizePaneAction({ exploreId }));
    }
  };

  const onChangeTimeSync = () => {
    dispatch(syncTimes(exploreId));
  };

  const onChangeFiscalYearStartMonth = (fiscalyearStartMonth: number) =>
    dispatch(updateFiscalYearStartMonthForSession(fiscalyearStartMonth));

  const onChangeRefreshInterval = (refreshInterval: string) => {
    dispatch(changeRefreshInterval({ exploreId, refreshInterval }));
  };

  return (
    <div ref={topOfViewRef}>
      {refreshInterval && <SetInterval func={onRunQuery} interval={refreshInterval} loading={loading} />}
      <div ref={topOfViewRef}>
        <AppChromeUpdate
          actions={[
            <DashNavButton
              key="share"
              tooltip="Copy shortened link"
              icon="share-alt"
              onClick={onCopyShortLink}
              aria-label="Copy shortened link"
            />,
            <div style={{ flex: 1 }} key="spacer" />,
          ]}
        />
      </div>
      <PageToolbar
        aria-label="Explore toolbar"
        leftItems={[
          <DataSourcePicker
            key={`${exploreId}-ds-picker`}
            mixed
            onChange={onChangeDatasource}
            current={datasourceInstance?.getRef()}
            hideTextValue={showSmallDataSourcePicker}
            width={showSmallDataSourcePicker ? 8 : undefined}
          />,
        ]}
        forceShowLeftItems
      >
        {[
          !splitted ? (
            <ToolbarButton
              variant="canvas"
              key="split"
              tooltip="Split the pane"
              onClick={onOpenSplitView}
              icon="columns"
              disabled={isLive}
            >
              Split
            </ToolbarButton>
          ) : (
            <ButtonGroup key="split-controls">
              <ToolbarButton
                variant="canvas"
                tooltip={`${isLargerPane ? 'Narrow' : 'Widen'} pane`}
                onClick={onClickResize}
                icon={isLargerPane ? 'gf-movepane-left' : 'gf-movepane-right'}
                iconOnly={true}
                className={cx(shouldRotateSplitIcon && rotateIcon)}
              />
              <ToolbarButton tooltip="Close split pane" onClick={onCloseSplitView} icon="times" variant="canvas">
                Close
              </ToolbarButton>
            </ButtonGroup>
          ),
          <ToolbarExtensionPoint
            splitted={splitted}
            key="toolbar-extension-point"
            exploreId={exploreId}
            timeZone={timeZone}
          />,
          !isLive && (
            <ExploreTimeControls
              key="timeControls"
              exploreId={exploreId}
              range={range}
              timeZone={timeZone}
              fiscalYearStartMonth={fiscalYearStartMonth}
              onChangeTime={onChangeTime}
              splitted={splitted}
              syncedTimes={syncedTimes}
              onChangeTimeSync={onChangeTimeSync}
              hideText={showSmallTimePicker}
              onChangeTimeZone={onChangeTimeZone}
              onChangeFiscalYearStartMonth={onChangeFiscalYearStartMonth}
            />
          ),
          <RefreshPicker
            key="refreshPicker"
            onIntervalChanged={onChangeRefreshInterval}
            value={refreshInterval}
            isLoading={loading}
            text={showSmallTimePicker ? undefined : loading ? 'Cancel' : 'Run query'}
            tooltip={showSmallTimePicker ? (loading ? 'Cancel' : 'Run query') : undefined}
            intervals={getTimeSrv().getValidIntervals(defaultIntervals)}
            isLive={isLive}
            onRefresh={() => onRunQuery(loading)}
            noIntervalPicker={isLive}
            primary={true}
            width={(showSmallTimePicker ? 35 : 108) + 'px'}
          />,
          datasourceInstance?.meta.streaming && (
            <LiveTailControls key="liveControls" exploreId={exploreId}>
              {(c) => {
                const controls = {
                  ...c,
                  start: () => {
                    reportInteraction('grafana_explore_logs_live_tailing_clicked', {
                      datasourceType: datasourceInstance?.type,
                    });
                    c.start();
                  },
                };
                return (
                  <LiveTailButton
                    splitted={splitted}
                    isLive={isLive}
                    isPaused={isPaused}
                    start={controls.start}
                    pause={controls.pause}
                    resume={controls.resume}
                    stop={controls.stop}
                  />
                );
              }}
            </LiveTailControls>
          ),
        ].filter(Boolean)}
      </PageToolbar>
    </div>
  );
}
