import React, { useCallback, useState } from 'react';

import { IconSize } from '../../types/icon';
import { IconButton } from '../IconButton/IconButton';
import { HorizontalGroup } from '../Layout/Layout';
import { TooltipPlacement } from '../Tooltip';

import { TableCellInspectModal } from './TableCellInspectModal';
import { FILTER_FOR_OPERATOR, FILTER_OUT_OPERATOR, TableCellProps } from './types';
import { getTextAlign } from './utils';

interface CellActionProps extends TableCellProps {
  previewMode: 'text' | 'code';
}

interface CommonButtonProps {
  size: IconSize;
  showFilters?: boolean;
  tooltipPlacement: TooltipPlacement;
}

export function CellActions({ field, cell, previewMode, showFilters, onCellFilterAdded }: CellActionProps) {
  const [isInspecting, setIsInspecting] = useState(false);

  const isRightAligned = getTextAlign(field) === 'flex-end';
  const inspectEnabled = Boolean(field.config.custom?.inspect);
  const commonButtonProps: CommonButtonProps = {
    size: 'sm',
    tooltipPlacement: 'top',
  };

  const onFilterFor = useCallback(
    (event: React.MouseEvent<HTMLButtonElement>) => {
      if (onCellFilterAdded) {
        onCellFilterAdded({ key: field.name, operator: FILTER_FOR_OPERATOR, value: cell.value });
      }
    },
    [cell, field, onCellFilterAdded]
  );
  const onFilterOut = useCallback(
    (event: React.MouseEvent<HTMLButtonElement>) => {
      if (onCellFilterAdded) {
        onCellFilterAdded({ key: field.name, operator: FILTER_OUT_OPERATOR, value: cell.value });
      }
    },
    [cell, field, onCellFilterAdded]
  );

  return (
    <>
      <div className={`cellActions ${isRightAligned ? 'cellActionsLeft' : ''}`}>
        <HorizontalGroup spacing="xs">
          {inspectEnabled && (
            <IconButton
              name="eye"
              tooltip="Inspect value"
              onClick={() => {
                setIsInspecting(true);
              }}
              {...commonButtonProps}
            />
          )}
          {showFilters && (
            <IconButton name={'search-plus'} onClick={onFilterFor} tooltip="Filter for value" {...commonButtonProps} />
          )}
          {showFilters && (
            <IconButton name={'search-minus'} onClick={onFilterOut} tooltip="Filter out value" {...commonButtonProps} />
          )}
        </HorizontalGroup>
      </div>

      {isInspecting && (
        <TableCellInspectModal
          mode={previewMode}
          value={cell.value}
          onDismiss={() => {
            setIsInspecting(false);
          }}
        />
      )}
    </>
  );
}
