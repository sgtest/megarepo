import { css, cx } from '@emotion/css';
import React, { MouseEventHandler } from 'react';
import { DraggableProvided } from 'react-beautiful-dnd';

import { GrafanaTheme2 } from '@grafana/data';
import { Stack } from '@grafana/experimental';
import { IconButton, useStyles2 } from '@grafana/ui';

export interface QueryOperationRowHeaderProps {
  actionsElement?: React.ReactNode;
  disabled?: boolean;
  draggable: boolean;
  collapsable?: boolean;
  dragHandleProps?: DraggableProvided['dragHandleProps'];
  headerElement?: React.ReactNode;
  isContentVisible: boolean;
  onRowToggle: () => void;
  reportDragMousePosition: MouseEventHandler<HTMLButtonElement>;
  title?: string;
  id: string;
  expanderMessages?: ExpanderMessages;
}

export interface ExpanderMessages {
  open: string;
  close: string;
}

export const QueryOperationRowHeader = ({
  actionsElement,
  disabled,
  draggable,
  collapsable = true,
  dragHandleProps,
  headerElement,
  isContentVisible,
  onRowToggle,
  reportDragMousePosition,
  title,
  id,
  expanderMessages,
}: QueryOperationRowHeaderProps) => {
  const styles = useStyles2(getStyles);

  let tooltipMessage = isContentVisible ? 'Collapse query row' : 'Expand query row';
  if (expanderMessages !== undefined && isContentVisible) {
    tooltipMessage = expanderMessages.close;
  } else if (expanderMessages !== undefined) {
    tooltipMessage = expanderMessages?.open;
  }

  return (
    <div className={styles.header}>
      <div className={styles.column}>
        {collapsable && (
          <IconButton
            name={isContentVisible ? 'angle-down' : 'angle-right'}
            tooltip={tooltipMessage}
            className={styles.collapseIcon}
            onClick={onRowToggle}
            aria-expanded={isContentVisible}
            aria-controls={id}
          />
        )}
        {title && (
          // disabling the a11y rules here as the IconButton above handles keyboard interactions
          // this is just to provide a better experience for mouse users
          // eslint-disable-next-line jsx-a11y/click-events-have-key-events, jsx-a11y/no-static-element-interactions
          <div className={styles.titleWrapper} onClick={onRowToggle} aria-label="Query operation row title">
            <div className={cx(styles.title, disabled && styles.disabled)}>{title}</div>
          </div>
        )}
        {headerElement}
      </div>

      <Stack gap={1} alignItems="center" wrap={false}>
        {actionsElement}
        {draggable && (
          <IconButton
            title="Drag and drop to reorder"
            name="draggabledots"
            tooltip="Drag and drop to reorder"
            tooltipPlacement="bottom"
            size="lg"
            className={styles.dragIcon}
            onMouseMove={reportDragMousePosition}
            {...dragHandleProps}
          />
        )}
      </Stack>
    </div>
  );
};

const getStyles = (theme: GrafanaTheme2) => ({
  header: css`
    label: Header;
    padding: ${theme.spacing(0.5, 0.5)};
    border-radius: ${theme.shape.radius.default};
    background: ${theme.colors.background.secondary};
    min-height: ${theme.spacing(4)};
    display: grid;
    grid-template-columns: minmax(100px, max-content) min-content;
    align-items: center;
    justify-content: space-between;
    white-space: nowrap;

    &:focus {
      outline: none;
    }
  `,
  column: css`
    label: Column;
    display: flex;
    align-items: center;
  `,
  dragIcon: css`
    cursor: grab;
    color: ${theme.colors.text.disabled};
    margin: ${theme.spacing(0, 0.5)};
    &:hover {
      color: ${theme.colors.text};
    }
  `,
  collapseIcon: css`
    margin-left: ${theme.spacing(0.5)};
    color: ${theme.colors.text.disabled};
    }
  `,
  titleWrapper: css`
    display: flex;
    align-items: center;
    flex-grow: 1;
    cursor: pointer;
    overflow: hidden;
    margin-right: ${theme.spacing(0.5)};
  `,
  title: css`
    font-weight: ${theme.typography.fontWeightBold};
    color: ${theme.colors.text.link};
    margin-left: ${theme.spacing(0.5)};
    overflow: hidden;
    text-overflow: ellipsis;
  `,
  disabled: css`
    color: ${theme.colors.text.disabled};
  `,
});

QueryOperationRowHeader.displayName = 'QueryOperationRowHeader';
