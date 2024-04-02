import React, { ReactNode } from 'react';

import { FieldType, getFieldDisplayName, TimeRange } from '@grafana/data';
import { SortOrder } from '@grafana/schema/dist/esm/common/common.gen';
import { TooltipDisplayMode, useStyles2 } from '@grafana/ui';
import { VizTooltipContent } from '@grafana/ui/src/components/VizTooltip/VizTooltipContent';
import { VizTooltipFooter } from '@grafana/ui/src/components/VizTooltip/VizTooltipFooter';
import { VizTooltipHeader } from '@grafana/ui/src/components/VizTooltip/VizTooltipHeader';
import { VizTooltipItem } from '@grafana/ui/src/components/VizTooltip/types';
import { getContentItems } from '@grafana/ui/src/components/VizTooltip/utils';
import { findNextStateIndex, fmtDuration } from 'app/core/components/TimelineChart/utils';

import { getDataLinks } from '../status-history/utils';
import { TimeSeriesTooltipProps, getStyles } from '../timeseries/TimeSeriesTooltip';

interface StateTimelineTooltip2Props extends TimeSeriesTooltipProps {
  timeRange: TimeRange;
  withDuration: boolean;
}

export const StateTimelineTooltip2 = ({
  frames,
  seriesFrame,
  dataIdxs,
  seriesIdx,
  mode = TooltipDisplayMode.Single,
  sortOrder = SortOrder.None,
  scrollable = false,
  isPinned,
  annotate,
  timeRange,
  withDuration,
  maxHeight,
}: StateTimelineTooltip2Props) => {
  const styles = useStyles2(getStyles);

  const xField = seriesFrame.fields[0];

  const dataIdx = seriesIdx != null ? dataIdxs[seriesIdx] : dataIdxs.find((idx) => idx != null);

  const xVal = xField.display!(xField.values[dataIdx!]).text;

  mode = isPinned ? TooltipDisplayMode.Single : mode;

  const contentItems = getContentItems(seriesFrame.fields, xField, dataIdxs, seriesIdx, mode, sortOrder);

  // append duration in single mode
  if (withDuration && mode === TooltipDisplayMode.Single) {
    const field = seriesFrame.fields[seriesIdx!];
    const nextStateIdx = findNextStateIndex(field, dataIdx!);
    let nextStateTs;
    if (nextStateIdx) {
      nextStateTs = xField.values[nextStateIdx!];
    }

    const stateTs = xField.values[dataIdx!];
    let duration: string;

    if (nextStateTs) {
      duration = nextStateTs && fmtDuration(nextStateTs - stateTs);
    } else {
      const to = timeRange.to.valueOf();
      duration = fmtDuration(to - stateTs);
    }

    contentItems.push({ label: 'Duration', value: duration });
  }

  let footer: ReactNode;

  if (isPinned && seriesIdx != null) {
    const field = seriesFrame.fields[seriesIdx];
    const dataIdx = dataIdxs[seriesIdx]!;
    const links = getDataLinks(field, dataIdx);

    footer = <VizTooltipFooter dataLinks={links} annotate={annotate} />;
  }

  const headerItem: VizTooltipItem = {
    label: xField.type === FieldType.time ? '' : getFieldDisplayName(xField, seriesFrame, frames),
    value: xVal,
  };

  return (
    <div className={styles.wrapper}>
      <VizTooltipHeader item={headerItem} isPinned={isPinned} />
      <VizTooltipContent items={contentItems} isPinned={isPinned} scrollable={scrollable} maxHeight={maxHeight} />
      {footer}
    </div>
  );
};
