import { css } from '@emotion/css';
import React from 'react';

import { DataFrame, FieldMatcherID, getFrameDisplayName, PanelProps, SelectableValue } from '@grafana/data';
import { PanelDataErrorView, reportInteraction } from '@grafana/runtime';
import { Select, Table, usePanelContext, useTheme2 } from '@grafana/ui';
import { TableSortByFieldState } from '@grafana/ui/src/components/Table/types';

import { hasDeprecatedParentRowIndex, migrateFromParentRowIndexToNestedFrames } from './migrations';
import { Options } from './panelcfg.gen';

export const INTERACTION_EVENT_NAME = 'table_panel_usage';
export const INTERACTION_ITEM = {
  COLUMN_RESIZE: 'column_resize',
  SORT_BY: 'sort_by',
  TABLE_SELECTION_CHANGE: 'table_selection_change',
  ERROR_VIEW: 'error_view',
  CELL_TYPE_CHANGE: 'cell_type_change',
};

interface Props extends PanelProps<Options> {}

export function TablePanel(props: Props) {
  const { data, height, width, options, fieldConfig, id, timeRange } = props;

  const theme = useTheme2();
  const panelContext = usePanelContext();
  const frames = hasDeprecatedParentRowIndex(data.series)
    ? migrateFromParentRowIndexToNestedFrames(data.series)
    : data.series;
  const count = frames?.length;
  const hasFields = frames[0]?.fields.length;
  const currentIndex = getCurrentFrameIndex(frames, options);
  const main = frames[currentIndex];

  let tableHeight = height;

  if (!count || !hasFields) {
    reportInteraction(INTERACTION_EVENT_NAME, { item: INTERACTION_ITEM.ERROR_VIEW });

    return <PanelDataErrorView panelId={id} fieldConfig={fieldConfig} data={data} />;
  }

  if (count > 1) {
    const inputHeight = theme.spacing.gridSize * theme.components.height.md;
    const padding = theme.spacing.gridSize;

    tableHeight = height - inputHeight - padding;
  }

  const tableElement = (
    <Table
      height={tableHeight}
      width={width}
      data={main}
      noHeader={!options.showHeader}
      showTypeIcons={options.showTypeIcons}
      resizable={true}
      initialSortBy={options.sortBy}
      onSortByChange={(sortBy) => onSortByChange(sortBy, props)}
      onColumnResize={(displayName, resizedWidth) => onColumnResize(displayName, resizedWidth, props)}
      onCellFilterAdded={panelContext.onAddAdHocFilter}
      footerOptions={options.footer}
      enablePagination={options.footer?.enablePagination}
      cellHeight={options.cellHeight}
      timeRange={timeRange}
    />
  );

  if (count === 1) {
    return tableElement;
  }

  const names = frames.map((frame, index) => {
    return {
      label: getFrameDisplayName(frame),
      value: index,
    };
  });

  return (
    <div className={tableStyles.wrapper}>
      {tableElement}
      <div className={tableStyles.selectWrapper}>
        <Select options={names} value={names[currentIndex]} onChange={(val) => onChangeTableSelection(val, props)} />
      </div>
    </div>
  );
}

function getCurrentFrameIndex(frames: DataFrame[], options: Options) {
  return options.frameIndex > 0 && options.frameIndex < frames.length ? options.frameIndex : 0;
}

function onColumnResize(fieldDisplayName: string, width: number, props: Props) {
  const { fieldConfig } = props;
  const { overrides } = fieldConfig;

  const matcherId = FieldMatcherID.byName;
  const propId = 'custom.width';

  // look for existing override
  const override = overrides.find((o) => o.matcher.id === matcherId && o.matcher.options === fieldDisplayName);

  if (override) {
    // look for existing property
    const property = override.properties.find((prop) => prop.id === propId);
    if (property) {
      property.value = width;
    } else {
      override.properties.push({ id: propId, value: width });
    }
  } else {
    overrides.push({
      matcher: { id: matcherId, options: fieldDisplayName },
      properties: [{ id: propId, value: width }],
    });
  }

  reportInteraction(INTERACTION_EVENT_NAME, { item: INTERACTION_ITEM.COLUMN_RESIZE });

  props.onFieldConfigChange({
    ...fieldConfig,
    overrides,
  });
}

function onSortByChange(sortBy: TableSortByFieldState[], props: Props) {
  reportInteraction(INTERACTION_EVENT_NAME, { item: INTERACTION_ITEM.SORT_BY });

  props.onOptionsChange({
    ...props.options,
    sortBy,
  });
}

function onChangeTableSelection(val: SelectableValue<number>, props: Props) {
  reportInteraction(INTERACTION_EVENT_NAME, { item: INTERACTION_ITEM.TABLE_SELECTION_CHANGE });

  props.onOptionsChange({
    ...props.options,
    frameIndex: val.value || 0,
  });
}

const tableStyles = {
  wrapper: css`
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    height: 100%;
  `,
  selectWrapper: css`
    padding: 8px 8px 0px 8px;
  `,
};
