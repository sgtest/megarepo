import { css } from '@emotion/css';
import { cloneDeep } from 'lodash';
import React, { ChangeEvent, useState } from 'react';

import {
  CoreApp,
  DataSourceApi,
  DataSourceInstanceSettings,
  GrafanaTheme2,
  LoadingState,
  PanelData,
  RelativeTimeRange,
  ThresholdsConfig,
} from '@grafana/data';
import { Stack } from '@grafana/experimental';
import { DataQuery } from '@grafana/schema';
import { GraphTresholdsStyleMode, Icon, InlineField, Input, Tooltip, useStyles2 } from '@grafana/ui';
import { QueryEditorRow } from 'app/features/query/components/QueryEditorRow';
import { AlertQuery } from 'app/types/unified-alerting-dto';

import { msToSingleUnitDuration } from '../../utils/time';
import { ExpressionStatusIndicator } from '../expressions/ExpressionStatusIndicator';

import { QueryOptions } from './QueryOptions';
import { VizWrapper } from './VizWrapper';

export const DEFAULT_MAX_DATA_POINTS = 43200;
export const DEFAULT_MIN_INTERVAL = '1s';

export interface AlertQueryOptions {
  maxDataPoints?: number | undefined;
  minInterval?: string | undefined;
}

interface Props {
  data: PanelData;
  error?: Error;
  query: AlertQuery;
  queries: AlertQuery[];
  dsSettings: DataSourceInstanceSettings;
  onChangeDataSource: (settings: DataSourceInstanceSettings, index: number) => void;
  onChangeQuery: (query: DataQuery, index: number) => void;
  onChangeTimeRange?: (timeRange: RelativeTimeRange, index: number) => void;
  onRemoveQuery: (query: DataQuery) => void;
  onDuplicateQuery: (query: AlertQuery) => void;
  onRunQueries: () => void;
  index: number;
  thresholds: ThresholdsConfig;
  thresholdsType?: GraphTresholdsStyleMode;
  onChangeThreshold?: (thresholds: ThresholdsConfig, index: number) => void;
  condition: string | null;
  onSetCondition: (refId: string) => void;
  onChangeQueryOptions: (options: AlertQueryOptions, index: number) => void;
}

export const QueryWrapper = ({
  data,
  error,
  dsSettings,
  index,
  onChangeDataSource,
  onChangeQuery,
  onChangeTimeRange,
  onRunQueries,
  onRemoveQuery,
  onDuplicateQuery,
  query,
  queries,
  thresholds,
  thresholdsType,
  onChangeThreshold,
  condition,
  onSetCondition,
  onChangeQueryOptions,
}: Props) => {
  const styles = useStyles2(getStyles);
  const [dsInstance, setDsInstance] = useState<DataSourceApi>();
  const defaults = dsInstance?.getDefaultQuery ? dsInstance.getDefaultQuery(CoreApp.UnifiedAlerting) : {};

  const queryWithDefaults = {
    ...defaults,
    ...cloneDeep(query.model),
  };

  function SelectingDataSourceTooltip() {
    const styles = useStyles2(getStyles);
    return (
      <div className={styles.dsTooltip}>
        <Tooltip
          content={
            <>
              Not finding the data source you want? Some data sources are not supported for alerting. Click on the icon
              for more information.
            </>
          }
        >
          <Icon
            name="info-circle"
            onClick={() =>
              window.open(
                ' https://grafana.com/docs/grafana/latest/alerting/fundamentals/data-source-alerting/',
                '_blank'
              )
            }
          />
        </Tooltip>
      </div>
    );
  }

  // TODO add a warning label here too when the data looks like time series data and is used as an alert condition
  function HeaderExtras({ query, error, index }: { query: AlertQuery; error?: Error; index: number }) {
    const queryOptions: AlertQueryOptions = {
      maxDataPoints: query.model.maxDataPoints,
      minInterval: query.model.intervalMs ? msToSingleUnitDuration(query.model.intervalMs) : undefined,
    };
    const alertQueryOptions: AlertQueryOptions = {
      maxDataPoints: queryOptions.maxDataPoints,
      minInterval: queryOptions.minInterval,
    };

    const isAlertCondition = condition === query.refId;

    return (
      <Stack direction="row" alignItems="center" gap={1}>
        <SelectingDataSourceTooltip />
        <QueryOptions
          onChangeTimeRange={onChangeTimeRange}
          query={query}
          queryOptions={alertQueryOptions}
          onChangeQueryOptions={onChangeQueryOptions}
          index={index}
        />
        <ExpressionStatusIndicator
          error={error}
          onSetCondition={() => onSetCondition(query.refId)}
          isCondition={isAlertCondition}
        />
      </Stack>
    );
  }

  const showVizualisation = data.state !== LoadingState.NotStarted;

  return (
    <Stack direction="column" gap={0.5}>
      <div className={styles.wrapper}>
        <QueryEditorRow<DataQuery>
          alerting
          collapsable={false}
          dataSource={dsSettings}
          onDataSourceLoaded={setDsInstance}
          onChangeDataSource={(settings) => onChangeDataSource(settings, index)}
          id={query.refId}
          index={index}
          key={query.refId}
          data={data}
          query={queryWithDefaults}
          onChange={(query) => onChangeQuery(query, index)}
          onRemoveQuery={onRemoveQuery}
          onAddQuery={() => onDuplicateQuery(cloneDeep(query))}
          onRunQuery={onRunQueries}
          queries={queries}
          renderHeaderExtras={() => <HeaderExtras query={query} index={index} error={error} />}
          app={CoreApp.UnifiedAlerting}
          hideDisableQuery={true}
        />
      </div>
      {showVizualisation && (
        <VizWrapper
          data={data}
          thresholds={thresholds}
          thresholdsType={thresholdsType}
          onThresholdsChange={onChangeThreshold ? (thresholds) => onChangeThreshold(thresholds, index) : undefined}
        />
      )}
    </Stack>
  );
};

export const EmptyQueryWrapper = ({ children }: React.PropsWithChildren<{}>) => {
  const styles = useStyles2(getStyles);
  return <div className={styles.wrapper}>{children}</div>;
};

export function MaxDataPointsOption({
  options,
  onChange,
}: {
  options: AlertQueryOptions;
  onChange: (options: AlertQueryOptions) => void;
}) {
  const value = options.maxDataPoints ?? '';

  const onMaxDataPointsBlur = (event: ChangeEvent<HTMLInputElement>) => {
    const maxDataPointsNumber = parseInt(event.target.value, 10);

    const maxDataPoints = isNaN(maxDataPointsNumber) || maxDataPointsNumber === 0 ? undefined : maxDataPointsNumber;

    if (maxDataPoints !== options.maxDataPoints) {
      onChange({
        ...options,
        maxDataPoints,
      });
    }
  };

  return (
    <InlineField
      labelWidth={24}
      label="Max data points"
      tooltip="The maximum data points per series. Used directly by some data sources and used in calculation of auto interval. With streaming data this value is used for the rolling buffer."
    >
      <Input
        type="number"
        width={10}
        placeholder={DEFAULT_MAX_DATA_POINTS.toLocaleString()}
        spellCheck={false}
        onBlur={onMaxDataPointsBlur}
        defaultValue={value}
      />
    </InlineField>
  );
}

export function MinIntervalOption({
  options,
  onChange,
}: {
  options: AlertQueryOptions;
  onChange: (options: AlertQueryOptions) => void;
}) {
  const value = options.minInterval ?? '';

  const onMinIntervalBlur = (event: ChangeEvent<HTMLInputElement>) => {
    const minInterval = event.target.value;
    if (minInterval !== value) {
      onChange({
        ...options,
        minInterval,
      });
    }
  };

  return (
    <InlineField
      label="Min interval"
      labelWidth={24}
      tooltip={
        <>
          A lower limit for the interval. Recommended to be set to write frequency, for example <code>1m</code> if your
          data is written every minute.
        </>
      }
    >
      <Input
        type="text"
        width={10}
        placeholder={DEFAULT_MIN_INTERVAL}
        spellCheck={false}
        onBlur={onMinIntervalBlur}
        defaultValue={value}
      />
    </InlineField>
  );
}

const getStyles = (theme: GrafanaTheme2) => ({
  wrapper: css`
    label: AlertingQueryWrapper;
    margin-bottom: ${theme.spacing(1)};
    border: 1px solid ${theme.colors.border.weak};
    border-radius: ${theme.shape.radius.default};

    button {
      overflow: visible;
    }
  `,
  dsTooltip: css`
    display: flex;
    align-items: center;
    &:hover {
      opacity: 0.85;
      cursor: pointer;
    }
  `,
});
