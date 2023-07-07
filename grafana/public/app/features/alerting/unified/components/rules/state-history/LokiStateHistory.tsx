import { css } from '@emotion/css';
import { isEmpty, sortBy, take, uniq } from 'lodash';
import React, { useCallback, useMemo, useRef, useState } from 'react';
import { useForm } from 'react-hook-form';

import { DataFrame, dateTime, GrafanaTheme2, TimeRange } from '@grafana/data';
import { Stack } from '@grafana/experimental';
import { Alert, Button, Field, Icon, Input, Label, TagList, Tooltip, useStyles2 } from '@grafana/ui';

import { stateHistoryApi } from '../../../api/stateHistoryApi';
import { combineMatcherStrings } from '../../../utils/alertmanager';
import { HoverCard } from '../../HoverCard';

import { LogRecordViewerByTimestamp } from './LogRecordViewer';
import { LogTimelineViewer } from './LogTimelineViewer';
import { useRuleHistoryRecords } from './useRuleHistoryRecords';

interface Props {
  ruleUID: string;
}

const MAX_TIMELINE_SERIES = 12;

const LokiStateHistory = ({ ruleUID }: Props) => {
  const styles = useStyles2(getStyles);
  const [instancesFilter, setInstancesFilter] = useState('');
  const logsRef = useRef<Map<number, HTMLElement>>(new Map<number, HTMLElement>());

  const { getValues, setValue, register, handleSubmit } = useForm({ defaultValues: { query: '' } });

  const { useGetRuleHistoryQuery } = stateHistoryApi;

  // We prefer log count-based limit rather than time-based, but the API doesn't support it yet
  const queryTimeRange = useMemo(() => getDefaultTimeRange(), []);

  const {
    currentData: stateHistory,
    isLoading,
    isError,
    error,
  } = useGetRuleHistoryQuery({
    ruleUid: ruleUID,
    from: queryTimeRange.from.unix(),
    to: queryTimeRange.to.unix(),
    limit: 250,
  });

  const { dataFrames, historyRecords, commonLabels, totalRecordsCount } = useRuleHistoryRecords(
    stateHistory,
    instancesFilter
  );

  const { frameSubset, frameSubsetTimestamps, frameTimeRange } = useFrameSubset(dataFrames);

  const onLogRecordLabelClick = useCallback(
    (label: string) => {
      const matcherString = combineMatcherStrings(getValues('query'), label);
      setInstancesFilter(matcherString);
      setValue('query', matcherString);
    },
    [setInstancesFilter, setValue, getValues]
  );

  const onFilterCleared = useCallback(() => {
    setInstancesFilter('');
    setValue('query', '');
  }, [setInstancesFilter, setValue]);

  const refToHighlight = useRef<HTMLElement | undefined>(undefined);

  const onTimelinePointerMove = useCallback(
    (seriesIdx: number, pointIdx: number) => {
      // remove the highlight from the previous refToHighlight
      refToHighlight.current?.classList.remove(styles.highlightedLogRecord);

      const timestamp = frameSubsetTimestamps[pointIdx];
      const newTimestampRef = logsRef.current.get(timestamp);

      // now we have the new ref, add the styles
      newTimestampRef?.classList.add(styles.highlightedLogRecord);
      // keeping this here (commented) in case we decide we want to go back to this
      // newTimestampRef?.scrollIntoView({ behavior: 'smooth', block: 'center' });

      refToHighlight.current = newTimestampRef;
    },
    [frameSubsetTimestamps, styles.highlightedLogRecord]
  );

  if (isLoading) {
    return <div>Loading...</div>;
  }
  if (isError) {
    return (
      <Alert title="Error fetching the state history" severity="error">
        {error instanceof Error ? error.message : 'Unable to fetch alert state history'}
      </Alert>
    );
  }

  const hasMoreInstances = frameSubset.length < dataFrames.length;
  const emptyStateMessage =
    totalRecordsCount > 0
      ? `No matches were found for the given filters among the ${totalRecordsCount} instances`
      : 'No state transitions have occurred in the last 30 days';

  return (
    <div className={styles.fullSize}>
      <form onSubmit={handleSubmit((data) => setInstancesFilter(data.query))}>
        <SearchFieldInput
          {...register('query')}
          showClearFilterSuffix={!!instancesFilter}
          onClearFilterClick={onFilterCleared}
        />
        <input type="submit" hidden />
      </form>
      {!isEmpty(commonLabels) && (
        <div className={styles.commonLabels}>
          <Stack gap={1} alignItems="center">
            <strong>Common labels</strong>
            <Tooltip content="Common labels are the ones attached to all of the alert instances">
              <Icon name="info-circle" />
            </Tooltip>
          </Stack>
          <TagList tags={commonLabels.map((label) => label.join('='))} />
        </div>
      )}
      {isEmpty(frameSubset) ? (
        <>
          <div className={styles.emptyState}>
            {emptyStateMessage}
            {totalRecordsCount > 0 && (
              <Button variant="secondary" type="button" onClick={onFilterCleared}>
                Clear filters
              </Button>
            )}
          </div>
        </>
      ) : (
        <>
          <div className={styles.graphWrapper}>
            <LogTimelineViewer frames={frameSubset} timeRange={frameTimeRange} onPointerMove={onTimelinePointerMove} />
          </div>
          {hasMoreInstances && (
            <div className={styles.moreInstancesWarning}>
              <Stack direction="row" alignItems="center" gap={1}>
                <Icon name="exclamation-triangle" size="sm" />
                <small>{`Only showing ${frameSubset.length} out of ${dataFrames.length} instances. Click on the labels to narrow down the results`}</small>
              </Stack>
            </div>
          )}
          <LogRecordViewerByTimestamp
            records={historyRecords}
            commonLabels={commonLabels}
            onRecordsRendered={(recordRefs) => (logsRef.current = recordRefs)}
            onLabelClick={onLogRecordLabelClick}
          />
        </>
      )}
    </div>
  );
};

function useFrameSubset(frames: DataFrame[]) {
  return useMemo(() => {
    const frameSubset = take(frames, MAX_TIMELINE_SERIES);
    const frameSubsetTimestamps = sortBy(uniq(frameSubset.flatMap((frame) => frame.fields[0].values)));

    const minTs = Math.min(...frameSubsetTimestamps);
    const maxTs = Math.max(...frameSubsetTimestamps);

    const rangeStart = dateTime(minTs);
    const rangeStop = dateTime(maxTs);

    const frameTimeRange: TimeRange = {
      from: rangeStart,
      to: rangeStop,
      raw: {
        from: rangeStart,
        to: rangeStop,
      },
    };

    return { frameSubset, frameSubsetTimestamps, frameTimeRange };
  }, [frames]);
}

interface SearchFieldInputProps extends Omit<React.ComponentProps<typeof Input>, 'prefix' | 'suffix' | 'placeholder'> {
  showClearFilterSuffix: boolean;
  onClearFilterClick: () => void;
}

const SearchFieldInput = React.forwardRef<HTMLInputElement, SearchFieldInputProps>(
  ({ showClearFilterSuffix, onClearFilterClick, ...rest }: SearchFieldInputProps, ref) => {
    return (
      <Field
        label={
          <Label htmlFor="instancesSearchInput">
            <Stack gap={0.5}>
              <span>Filter instances</span>
              <HoverCard
                content={
                  <>
                    Use label matcher expression (like <code>{'{foo=bar}'}</code>) or click on an instance label to
                    filter instances
                  </>
                }
              >
                <Icon name="info-circle" size="sm" />
              </HoverCard>
            </Stack>
          </Label>
        }
      >
        <Input
          id="instancesSearchInput"
          prefix={<Icon name="search" />}
          suffix={
            showClearFilterSuffix && (
              <Button fill="text" icon="times" size="sm" onClick={onClearFilterClick}>
                Clear
              </Button>
            )
          }
          placeholder="Filter instances"
          ref={ref}
          {...rest}
        />
      </Field>
    );
  }
);
SearchFieldInput.displayName = 'SearchFieldInput';

function getDefaultTimeRange(): TimeRange {
  const fromDateTime = dateTime().subtract(30, 'days');
  const toDateTime = dateTime();
  return {
    from: fromDateTime,
    to: toDateTime,
    raw: { from: fromDateTime, to: toDateTime },
  };
}

export const getStyles = (theme: GrafanaTheme2) => ({
  fullSize: css`
    min-width: 100%;
    height: 100%;

    display: flex;
    flex-direction: column;
  `,
  graphWrapper: css`
    padding: ${theme.spacing()} 0;
  `,
  emptyState: css`
    color: ${theme.colors.text.secondary};

    display: flex;
    flex-direction: column;
    gap: ${theme.spacing(2)};
    align-items: center;
    margin: auto auto;
  `,
  moreInstancesWarning: css`
    color: ${theme.colors.warning.text};
    padding: ${theme.spacing()};
  `,
  commonLabels: css`
    display: grid;
    grid-template-columns: max-content auto;
  `,
  // we need !important here to override the list item default styles
  highlightedLogRecord: css`
    background: ${theme.colors.primary.transparent} !important;
    outline: 1px solid ${theme.colors.primary.shade} !important;
  `,
});

export default LokiStateHistory;
