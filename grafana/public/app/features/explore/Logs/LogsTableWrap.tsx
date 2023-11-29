import { css } from '@emotion/css';
import React, { useCallback, useEffect, useState } from 'react';

import {
  DataFrame,
  ExploreLogsPanelState,
  GrafanaTheme2,
  Labels,
  LogsSortOrder,
  SelectableValue,
  SplitOpen,
  TimeRange,
} from '@grafana/data';
import { reportInteraction } from '@grafana/runtime/src';
import { InlineField, Select, Themeable2 } from '@grafana/ui/';

import { parseLogsFrame } from '../../logs/logsFrame';

import { LogsColumnSearch } from './LogsColumnSearch';
import { LogsTable } from './LogsTable';
import { LogsTableMultiSelect } from './LogsTableMultiSelect';
import { fuzzySearch } from './utils/uFuzzy';

interface Props extends Themeable2 {
  logsFrames: DataFrame[];
  width: number;
  timeZone: string;
  splitOpen: SplitOpen;
  range: TimeRange;
  logsSortOrder: LogsSortOrder;
  panelState: ExploreLogsPanelState | undefined;
  updatePanelState: (panelState: Partial<ExploreLogsPanelState>) => void;
  onClickFilterLabel?: (key: string, value: string, frame?: DataFrame) => void;
  onClickFilterOutLabel?: (key: string, value: string, frame?: DataFrame) => void;
  datasourceType?: string;
}

export type fieldNameMeta = {
  percentOfLinesWithLabel: number;
  active: boolean | undefined;
  type?: 'BODY_FIELD' | 'TIME_FIELD';
};
type fieldName = string;
type fieldNameMetaStore = Record<fieldName, fieldNameMeta>;

export function LogsTableWrap(props: Props) {
  const { logsFrames, updatePanelState, panelState } = props;
  const propsColumns = panelState?.columns;
  // Save the normalized cardinality of each label
  const [columnsWithMeta, setColumnsWithMeta] = useState<fieldNameMetaStore | undefined>(undefined);

  // Filtered copy of columnsWithMeta that only includes matching results
  const [filteredColumnsWithMeta, setFilteredColumnsWithMeta] = useState<fieldNameMetaStore | undefined>(undefined);
  const [searchValue, setSearchValue] = useState<string>('');

  const height = getLogsTableHeight();
  const panelStateRefId = props?.panelState?.refId;

  // The current dataFrame containing the refId of the current query
  const [currentDataFrame, setCurrentDataFrame] = useState<DataFrame>(
    logsFrames.find((f) => f.refId === panelStateRefId) ?? logsFrames[0]
  );

  const getColumnsFromProps = useCallback(
    (fieldNames: fieldNameMetaStore) => {
      const previouslySelected = props.panelState?.columns;
      if (previouslySelected) {
        Object.values(previouslySelected).forEach((key) => {
          if (fieldNames[key]) {
            fieldNames[key].active = true;
          }
        });
      }
      return fieldNames;
    },
    [props.panelState?.columns]
  );
  const logsFrame = parseLogsFrame(currentDataFrame);

  useEffect(() => {
    if (logsFrame?.timeField.name && logsFrame?.bodyField.name && !propsColumns) {
      const defaultColumns = { 0: logsFrame?.timeField.name ?? '', 1: logsFrame?.bodyField.name ?? '' };
      updatePanelState({
        columns: Object.values(defaultColumns),
        visualisationType: 'table',
        labelFieldName: logsFrame?.getLabelFieldName() ?? undefined,
      });
    }
  }, [logsFrame, propsColumns, updatePanelState]);

  /**
   * When logs frame updates (e.g. query|range changes), we need to set the selected frame to state
   */
  useEffect(() => {
    const newFrame = logsFrames.find((f) => f.refId === panelStateRefId) ?? logsFrames[0];
    if (newFrame) {
      setCurrentDataFrame(newFrame);
    }
  }, [logsFrames, panelStateRefId]);

  /**
   * Keeps the filteredColumnsWithMeta state in sync with the columnsWithMeta state,
   * which can be updated by explore browser history state changes
   * This prevents an edge case bug where the user is navigating while a search is open.
   */
  useEffect(() => {
    if (!columnsWithMeta || !filteredColumnsWithMeta) {
      return;
    }
    let newFiltered = { ...filteredColumnsWithMeta };
    let flag = false;
    Object.keys(columnsWithMeta).forEach((key) => {
      if (newFiltered[key] && newFiltered[key].active !== columnsWithMeta[key].active) {
        newFiltered[key] = columnsWithMeta[key];
        flag = true;
      }
    });
    if (flag) {
      setFilteredColumnsWithMeta(newFiltered);
    }
  }, [columnsWithMeta, filteredColumnsWithMeta]);

  /**
   * when the query results change, we need to update the columnsWithMeta state
   * and reset any local search state
   *
   * This will also find all the unique labels, and calculate how many log lines have each label into the labelCardinality Map
   * Then it normalizes the counts
   *
   */
  useEffect(() => {
    // If the data frame is empty, there's nothing to viz, it could mean the user has unselected all columns
    if (!currentDataFrame.length) {
      return;
    }
    const numberOfLogLines = currentDataFrame ? currentDataFrame.length : 0;
    const logsFrame = parseLogsFrame(currentDataFrame);
    const labels = logsFrame?.getLogFrameLabelsAsLabels();

    const otherFields = [];

    if (logsFrame) {
      otherFields.push(...logsFrame.extraFields.filter((field) => !field?.config?.custom?.hidden));
    }
    if (logsFrame?.severityField) {
      otherFields.push(logsFrame?.severityField);
    }
    if (logsFrame?.bodyField) {
      otherFields.push(logsFrame?.bodyField);
    }
    if (logsFrame?.timeField) {
      otherFields.push(logsFrame?.timeField);
    }

    // Use a map to dedupe labels and count their occurrences in the logs
    const labelCardinality = new Map<fieldName, fieldNameMeta>();

    // What the label state will look like
    let pendingLabelState: fieldNameMetaStore = {};

    // If we have labels and log lines
    if (labels?.length && numberOfLogLines) {
      // Iterate through all of Labels
      labels.forEach((labels: Labels) => {
        const labelsArray = Object.keys(labels);
        // Iterate through the label values
        labelsArray.forEach((label) => {
          // If it's already in our map, increment the count
          if (labelCardinality.has(label)) {
            const value = labelCardinality.get(label);
            if (value) {
              labelCardinality.set(label, {
                percentOfLinesWithLabel: value.percentOfLinesWithLabel + 1,
                active: value?.active,
              });
            }
            // Otherwise add it
          } else {
            labelCardinality.set(label, { percentOfLinesWithLabel: 1, active: undefined });
          }
        });
      });

      // Converting the map to an object
      pendingLabelState = Object.fromEntries(labelCardinality);

      // Convert count to percent of log lines
      Object.keys(pendingLabelState).forEach((key) => {
        pendingLabelState[key].percentOfLinesWithLabel = normalize(
          pendingLabelState[key].percentOfLinesWithLabel,
          numberOfLogLines
        );
      });
    }

    // Normalize the other fields
    otherFields.forEach((field) => {
      pendingLabelState[field.name] = {
        percentOfLinesWithLabel: normalize(
          field.values.filter((value) => value !== null && value !== undefined).length,
          numberOfLogLines
        ),
        active: pendingLabelState[field.name]?.active,
      };
    });

    pendingLabelState = getColumnsFromProps(pendingLabelState);

    // Get all active columns
    const active = Object.keys(pendingLabelState).filter((key) => pendingLabelState[key].active);

    // If nothing is selected, then select the default columns
    if (active.length === 0) {
      if (logsFrame?.bodyField?.name) {
        pendingLabelState[logsFrame.bodyField.name].active = true;
      }
      if (logsFrame?.timeField?.name) {
        pendingLabelState[logsFrame.timeField.name].active = true;
      }
    }

    if (logsFrame?.bodyField?.name && logsFrame?.timeField?.name) {
      pendingLabelState[logsFrame.bodyField.name].type = 'BODY_FIELD';
      pendingLabelState[logsFrame.timeField.name].type = 'TIME_FIELD';
    }

    setColumnsWithMeta(pendingLabelState);

    // The panel state is updated when the user interacts with the multi-select sidebar
  }, [currentDataFrame, getColumnsFromProps]);

  if (!columnsWithMeta) {
    return null;
  }

  function columnFilterEvent(columnName: string) {
    if (columnsWithMeta) {
      const newState = !columnsWithMeta[columnName]?.active;
      const priorActiveCount = Object.keys(columnsWithMeta).filter((column) => columnsWithMeta[column]?.active)?.length;
      const event = {
        columnAction: newState ? 'add' : 'remove',
        columnCount: newState ? priorActiveCount + 1 : priorActiveCount - 1,
        datasourceType: props.datasourceType,
      };
      reportInteraction('grafana_explore_logs_table_column_filter_clicked', event);
    }
  }

  function searchFilterEvent(searchResultCount: number) {
    reportInteraction('grafana_explore_logs_table_text_search_result_count', {
      resultCount: searchResultCount,
      datasourceType: props.datasourceType ?? 'unknown',
    });
  }

  const clearSelection = () => {
    const pendingLabelState = { ...columnsWithMeta };
    Object.keys(pendingLabelState).forEach((key) => {
      pendingLabelState[key].active = !!pendingLabelState[key].type;
    });
    setColumnsWithMeta(pendingLabelState);
  };

  // Toggle a column on or off when the user interacts with an element in the multi-select sidebar
  const toggleColumn = (columnName: fieldName) => {
    if (!columnsWithMeta || !(columnName in columnsWithMeta)) {
      console.warn('failed to get column', columnsWithMeta);
      return;
    }

    const pendingLabelState = {
      ...columnsWithMeta,
      [columnName]: { ...columnsWithMeta[columnName], active: !columnsWithMeta[columnName]?.active },
    };

    // Analytics
    columnFilterEvent(columnName);

    // Set local state
    setColumnsWithMeta(pendingLabelState);

    // If user is currently filtering, update filtered state
    if (filteredColumnsWithMeta) {
      const pendingFilteredLabelState = {
        ...filteredColumnsWithMeta,
        [columnName]: { ...filteredColumnsWithMeta[columnName], active: !filteredColumnsWithMeta[columnName]?.active },
      };
      setFilteredColumnsWithMeta(pendingFilteredLabelState);
    }

    const newColumns: Record<number, string> = Object.assign(
      {},
      // Get the keys of the object as an array
      Object.keys(pendingLabelState)
        // Only include active filters
        .filter((key) => pendingLabelState[key]?.active)
    );

    const defaultColumns = { 0: logsFrame?.timeField.name ?? '', 1: logsFrame?.bodyField.name ?? '' };
    const newPanelState: ExploreLogsPanelState = {
      ...props.panelState,
      // URL format requires our array of values be an object, so we convert it using object.assign
      columns: Object.keys(newColumns).length ? newColumns : defaultColumns,
      refId: currentDataFrame.refId,
      visualisationType: 'table',
      labelFieldName: logsFrame?.getLabelFieldName() ?? undefined,
    };

    // Update url state
    updatePanelState(newPanelState);
  };

  // uFuzzy search dispatcher, adds any matches to the local state
  const dispatcher = (data: string[][]) => {
    const matches = data[0];
    let newColumnsWithMeta: fieldNameMetaStore = {};
    let numberOfResults = 0;
    matches.forEach((match) => {
      if (match in columnsWithMeta) {
        newColumnsWithMeta[match] = columnsWithMeta[match];
        numberOfResults++;
      }
    });
    setFilteredColumnsWithMeta(newColumnsWithMeta);
    searchFilterEvent(numberOfResults);
  };

  // uFuzzy search
  const search = (needle: string) => {
    fuzzySearch(Object.keys(columnsWithMeta), needle, dispatcher);
  };

  // onChange handler for search input
  const onSearchInputChange = (e: React.FormEvent<HTMLInputElement>) => {
    const value = e.currentTarget?.value;
    setSearchValue(value);
    if (value) {
      search(value);
    } else {
      // If the search input is empty, reset the local search state.
      setFilteredColumnsWithMeta(undefined);
    }
  };

  const onFrameSelectorChange = (value: SelectableValue<string>) => {
    const matchingDataFrame = logsFrames.find((frame) => frame.refId === value.value);
    if (matchingDataFrame) {
      setCurrentDataFrame(logsFrames.find((frame) => frame.refId === value.value) ?? logsFrames[0]);
    }
    props.updatePanelState({ refId: value.value, labelFieldName: logsFrame?.getLabelFieldName() ?? undefined });
  };

  const sidebarWidth = 220;
  const totalWidth = props.width;
  const tableWidth = totalWidth - sidebarWidth;
  const styles = getStyles(props.theme, height, sidebarWidth);

  return (
    <>
      <div>
        {logsFrames.length > 1 && (
          <div>
            <InlineField
              label="Select query"
              htmlFor="explore_logs_table_frame_selector"
              labelWidth={22}
              tooltip="Select a query to visualize in the table."
            >
              <Select
                inputId={'explore_logs_table_frame_selector'}
                aria-label={'Select query by name'}
                value={currentDataFrame.refId}
                options={logsFrames.map((frame) => {
                  return {
                    label: frame.refId,
                    value: frame.refId,
                  };
                })}
                onChange={onFrameSelectorChange}
              />
            </InlineField>
          </div>
        )}
      </div>
      <div className={styles.wrapper}>
        <section className={styles.sidebar}>
          <LogsColumnSearch value={searchValue} onChange={onSearchInputChange} />
          <LogsTableMultiSelect
            toggleColumn={toggleColumn}
            filteredColumnsWithMeta={filteredColumnsWithMeta}
            columnsWithMeta={columnsWithMeta}
            clear={clearSelection}
          />
        </section>
        <LogsTable
          onClickFilterLabel={props.onClickFilterLabel}
          onClickFilterOutLabel={props.onClickFilterOutLabel}
          logsSortOrder={props.logsSortOrder}
          range={props.range}
          splitOpen={props.splitOpen}
          timeZone={props.timeZone}
          width={tableWidth}
          dataFrame={currentDataFrame}
          columnsWithMeta={columnsWithMeta}
          height={height}
        />
      </div>
    </>
  );
}

const normalize = (value: number, total: number): number => {
  return Math.ceil((100 * value) / total);
};

function getStyles(theme: GrafanaTheme2, height: number, width: number) {
  return {
    wrapper: css({
      display: 'flex',
    }),
    sidebar: css({
      height: height,
      fontSize: theme.typography.pxToRem(11),
      overflowY: 'hidden',
      width: width,
      paddingRight: theme.spacing(1.5),
    }),
  };
}

export const getLogsTableHeight = () => {
  // Instead of making the height of the table based on the content (like in the table panel itself), let's try to use the vertical space that is available.
  // Since this table is in explore, we can expect the user to be running multiple queries that return disparate numbers of rows and labels in the same session
  // Also changing the height of the table between queries can be and cause content to jump, so we'll set a minimum height of 500px, and a max based on the innerHeight
  // Ideally the table container should always be able to fit in the users viewport without needing to scroll
  return Math.max(window.innerHeight - 500, 500);
};
