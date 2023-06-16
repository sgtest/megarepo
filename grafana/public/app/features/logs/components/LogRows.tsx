import memoizeOne from 'memoize-one';
import React, { PureComponent } from 'react';

import {
  TimeZone,
  LogsDedupStrategy,
  LogRowModel,
  Field,
  LinkModel,
  LogsSortOrder,
  CoreApp,
  DataFrame,
} from '@grafana/data';
import { withTheme2, Themeable2 } from '@grafana/ui';

import { sortLogRows } from '../utils';

//Components
import { LogRow } from './LogRow';
import { getLogRowStyles } from './getLogRowStyles';

export const PREVIEW_LIMIT = 100;

export interface Props extends Themeable2 {
  logRows?: LogRowModel[];
  deduplicatedRows?: LogRowModel[];
  dedupStrategy: LogsDedupStrategy;
  showLabels: boolean;
  showTime: boolean;
  wrapLogMessage: boolean;
  prettifyLogMessage: boolean;
  timeZone: TimeZone;
  enableLogDetails: boolean;
  logsSortOrder?: LogsSortOrder | null;
  previewLimit?: number;
  forceEscape?: boolean;
  displayedFields?: string[];
  app?: CoreApp;
  showContextToggle?: (row?: LogRowModel) => boolean;
  onClickFilterLabel?: (key: string, value: string) => void;
  onClickFilterOutLabel?: (key: string, value: string) => void;
  getFieldLinks?: (field: Field, rowIndex: number, dataFrame: DataFrame) => Array<LinkModel<Field>>;
  onClickShowField?: (key: string) => void;
  onClickHideField?: (key: string) => void;
  onLogRowHover?: (row?: LogRowModel) => void;
  onOpenContext?: (row: LogRowModel, onClose: () => void) => void;
  onPermalinkClick?: (row: LogRowModel) => Promise<void>;
  permalinkedRowId?: string;
  scrollIntoView?: (element: HTMLElement) => void;
}

interface State {
  renderAll: boolean;
}

class UnThemedLogRows extends PureComponent<Props, State> {
  renderAllTimer: number | null = null;

  static defaultProps = {
    previewLimit: PREVIEW_LIMIT,
  };

  state: State = {
    renderAll: false,
  };

  /**
   * Toggle the `contextIsOpen` state when a context of one LogRow is opened in order to not show the menu of the other log rows.
   */
  openContext = (row: LogRowModel, onClose: () => void): void => {
    if (this.props.onOpenContext) {
      this.props.onOpenContext(row, onClose);
    }
  };

  componentDidMount() {
    // Staged rendering
    const { logRows, previewLimit } = this.props;
    const rowCount = logRows ? logRows.length : 0;
    // Render all right away if not too far over the limit
    const renderAll = rowCount <= previewLimit! * 2;
    if (renderAll) {
      this.setState({ renderAll });
    } else {
      this.renderAllTimer = window.setTimeout(() => this.setState({ renderAll: true }), 2000);
    }
  }

  componentWillUnmount() {
    if (this.renderAllTimer) {
      clearTimeout(this.renderAllTimer);
    }
  }

  makeGetRows = memoizeOne((orderedRows: LogRowModel[]) => {
    return () => orderedRows;
  });

  sortLogs = memoizeOne((logRows: LogRowModel[], logsSortOrder: LogsSortOrder): LogRowModel[] =>
    sortLogRows(logRows, logsSortOrder)
  );

  render() {
    const {
      dedupStrategy,
      showContextToggle,
      showLabels,
      showTime,
      wrapLogMessage,
      prettifyLogMessage,
      logRows,
      deduplicatedRows,
      timeZone,
      onClickFilterLabel,
      onClickFilterOutLabel,
      theme,
      enableLogDetails,
      previewLimit,
      getFieldLinks,
      logsSortOrder,
      displayedFields,
      onClickShowField,
      onClickHideField,
      forceEscape,
      onLogRowHover,
      app,
    } = this.props;
    const { renderAll } = this.state;
    const styles = getLogRowStyles(theme);
    const dedupedRows = deduplicatedRows ? deduplicatedRows : logRows;
    const hasData = logRows && logRows.length > 0;
    const dedupCount = dedupedRows
      ? dedupedRows.reduce((sum, row) => (row.duplicates ? sum + row.duplicates : sum), 0)
      : 0;
    const showDuplicates = dedupStrategy !== LogsDedupStrategy.none && dedupCount > 0;
    // Staged rendering
    const processedRows = dedupedRows ? dedupedRows : [];
    const orderedRows = logsSortOrder ? this.sortLogs(processedRows, logsSortOrder) : processedRows;
    const firstRows = orderedRows.slice(0, previewLimit!);
    const lastRows = orderedRows.slice(previewLimit!, orderedRows.length);

    // React profiler becomes unusable if we pass all rows to all rows and their labels, using getter instead
    const getRows = this.makeGetRows(orderedRows);

    const getLogRowProperties = (row: LogRowModel) => {
      return {
        getRows: getRows,
        row: row,
        showContextToggle: showContextToggle,
        showDuplicates: showDuplicates,
        showLabels: showLabels,
        showTime: showTime,
        displayedFields: displayedFields,
        wrapLogMessage: wrapLogMessage,
        prettifyLogMessage: prettifyLogMessage,
        timeZone: timeZone,
        enableLogDetails: enableLogDetails,
        onClickFilterLabel: onClickFilterLabel,
        onClickFilterOutLabel: onClickFilterOutLabel,
        onClickShowField: onClickShowField,
        onClickHideField: onClickHideField,
        getFieldLinks: getFieldLinks,
        logsSortOrder: logsSortOrder,
        forceEscape: forceEscape,
        onOpenContext: this.openContext,
        onLogRowHover: onLogRowHover,
        app: app,
        styles: styles,
        onPermalinkClick: this.props.onPermalinkClick,
        scrollIntoView: this.props.scrollIntoView,
        permalinkedRowId: this.props.permalinkedRowId,
      };
    };
    return (
      <table className={styles.logsRowsTable}>
        <tbody>
          {hasData && firstRows.map((row) => <LogRow key={row.uid} {...getLogRowProperties(row)} />)}
          {hasData && renderAll && lastRows.map((row) => <LogRow key={row.uid} {...getLogRowProperties(row)} />)}
          {hasData && !renderAll && (
            <tr>
              <td colSpan={5}>Rendering {orderedRows.length - previewLimit!} rows...</td>
            </tr>
          )}
        </tbody>
      </table>
    );
  }
}

export const LogRows = withTheme2(UnThemedLogRows);
LogRows.displayName = 'LogsRows';
