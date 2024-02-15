import { css } from '@emotion/css';
import React from 'react';

import { DataFrame, FieldType, LinkModel, Field, getFieldDisplayName } from '@grafana/data';
import { SortOrder, TooltipDisplayMode } from '@grafana/schema/dist/esm/common/common.gen';
import { useStyles2 } from '@grafana/ui';
import { VizTooltipContent } from '@grafana/ui/src/components/VizTooltip/VizTooltipContent';
import { VizTooltipFooter } from '@grafana/ui/src/components/VizTooltip/VizTooltipFooter';
import { VizTooltipHeader } from '@grafana/ui/src/components/VizTooltip/VizTooltipHeader';
import { LabelValue } from '@grafana/ui/src/components/VizTooltip/types';
import { getContentItems } from '@grafana/ui/src/components/VizTooltip/utils';

import { getDataLinks } from '../status-history/utils';

// exemplar / annotation / time region hovering?
// add annotation UI / alert dismiss UI?

export interface TimeSeriesTooltipProps {
  frames?: DataFrame[];
  // aligned series frame
  seriesFrame: DataFrame;
  // hovered points
  dataIdxs: Array<number | null>;
  // closest/hovered series
  seriesIdx?: number | null;
  mode?: TooltipDisplayMode;
  sortOrder?: SortOrder;

  isPinned: boolean;
  scrollable?: boolean;

  annotate?: () => void;
}

export const TimeSeriesTooltip = ({
  frames,
  seriesFrame,
  dataIdxs,
  seriesIdx,
  mode = TooltipDisplayMode.Single,
  sortOrder = SortOrder.None,
  scrollable = false,
  isPinned,
  annotate,
}: TimeSeriesTooltipProps) => {
  const styles = useStyles2(getStyles);

  const xField = seriesFrame.fields[0];

  const xVal = xField.display!(xField.values[dataIdxs[0]!]).text;

  const contentItems = getContentItems(
    seriesFrame.fields,
    xField,
    dataIdxs,
    seriesIdx,
    mode,
    sortOrder,
    (field) => field.type === FieldType.number
  );

  let links: Array<LinkModel<Field>> = [];

  if (seriesIdx != null) {
    const field = seriesFrame.fields[seriesIdx];
    const dataIdx = dataIdxs[seriesIdx]!;
    links = getDataLinks(field, dataIdx);
  }

  const headerItem: LabelValue = {
    label: xField.type === FieldType.time ? '' : getFieldDisplayName(xField, seriesFrame, frames),
    value: xVal,
  };

  return (
    <div>
      <div className={styles.wrapper}>
        <VizTooltipHeader headerLabel={headerItem} isPinned={isPinned} />
        <VizTooltipContent contentLabelValue={contentItems} isPinned={isPinned} scrollable={scrollable} />
        {isPinned && <VizTooltipFooter dataLinks={links} annotate={annotate} />}
      </div>
    </div>
  );
};

export const getStyles = () => ({
  wrapper: css({
    display: 'flex',
    flexDirection: 'column',
  }),
});
