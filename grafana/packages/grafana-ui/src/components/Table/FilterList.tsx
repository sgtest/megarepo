import { css, cx } from '@emotion/css';
import React, { useCallback, useMemo, useState } from 'react';
import { FixedSizeList as List } from 'react-window';

import { GrafanaTheme2, formattedValueToString, getValueFormat, SelectableValue } from '@grafana/data';

import { ButtonSelect, Checkbox, FilterInput, HorizontalGroup, Label, VerticalGroup } from '..';
import { useStyles2, useTheme2 } from '../../themes';

interface Props {
  values: SelectableValue[];
  options: SelectableValue[];
  onChange: (options: SelectableValue[]) => void;
  caseSensitive?: boolean;
  showOperators?: boolean;
}

const ITEM_HEIGHT = 28;
const MIN_HEIGHT = ITEM_HEIGHT * 5;

const operatorSelectableValues: { [key: string]: SelectableValue<string> } = {
  Contains: { label: 'Contains', value: 'Contains', description: 'Contains' },
  '=': { label: '=', value: '=', description: 'Equals' },
  '!=': { label: '!=', value: '!=', description: 'Not equals' },
  '>': { label: '>', value: '>', description: 'Greater' },
  '>=': { label: '>=', value: '>=', description: 'Greater or Equal' },
  '<': { label: '<', value: '<', description: 'Less' },
  '<=': { label: '<=', value: '<=', description: 'Less or Equal' },
  Expression: {
    label: 'Expression',
    value: 'Expression',
    description: 'Bool Expression (Char $ represents the column value in the expression, e.g. "$ >= 10 && $ <= 12")',
  },
};
const OPERATORS = Object.values(operatorSelectableValues);
const REGEX_OPERATOR = operatorSelectableValues['Contains'];
const XPR_OPERATOR = operatorSelectableValues['Expression'];

const comparableValue = (value: string): string | number | Date | boolean => {
  value = value.trim().replace(/\\/g, '');

  // Does it look like a Date (Starting with pattern YYYY-MM-DD* or YYYY/MM/DD*)?
  if (/^(\d{4}-\d{2}-\d{2}|\d{4}\/\d{2}\/\d{2})/.test(value)) {
    const date = new Date(value);
    if (!isNaN(date.getTime())) {
      const fmt = getValueFormat('dateTimeAsIso');
      return formattedValueToString(fmt(date.getTime()));
    }
  }
  // Does it look like a Number?
  const num = parseFloat(value);
  if (!isNaN(num)) {
    return num;
  }
  // Does it look like a Bool?
  const lvalue = value.toLowerCase();
  if (lvalue === 'true' || lvalue === 'false') {
    return lvalue === 'true';
  }
  // Anything else
  return value;
};

export const FilterList = ({ options, values, caseSensitive, showOperators, onChange }: Props) => {
  const [operator, setOperator] = useState<SelectableValue<string>>(REGEX_OPERATOR);
  const [searchFilter, setSearchFilter] = useState('');
  const regex = useMemo(() => new RegExp(searchFilter, caseSensitive ? undefined : 'i'), [searchFilter, caseSensitive]);
  const items = useMemo(
    () =>
      options.filter((option) => {
        if (!showOperators || !searchFilter || operator.value === REGEX_OPERATOR.value) {
          if (option.label === undefined) {
            return false;
          }
          return regex.test(option.label);
        } else if (operator.value === XPR_OPERATOR.value) {
          if (option.value === undefined) {
            return false;
          }
          try {
            const xpr = searchFilter.replace(/\\/g, '');
            const fnc = new Function('$', `'use strict'; return ${xpr};`);
            const val = comparableValue(option.value);
            return fnc(val);
          } catch (_) {}
          return false;
        } else {
          if (option.value === undefined) {
            return false;
          }

          const value1 = comparableValue(option.value);
          const value2 = comparableValue(searchFilter);

          switch (operator.value) {
            case '=':
              return value1 === value2;
            case '!=':
              return value1 !== value2;
            case '>':
              return value1 > value2;
            case '>=':
              return value1 >= value2;
            case '<':
              return value1 < value2;
            case '<=':
              return value1 <= value2;
          }
          return false;
        }
      }),
    [options, regex, showOperators, operator, searchFilter]
  );
  const selectedItems = useMemo(() => items.filter((item) => values.includes(item)), [items, values]);

  const selectCheckValue = useMemo(() => items.length === selectedItems.length, [items, selectedItems]);
  const selectCheckIndeterminate = useMemo(
    () => selectedItems.length > 0 && items.length > selectedItems.length,
    [items, selectedItems]
  );
  const selectCheckLabel = useMemo(
    () => (selectedItems.length ? `${selectedItems.length} selected` : `Select all`),
    [selectedItems]
  );
  const selectCheckDescription = useMemo(
    () =>
      items.length !== selectedItems.length
        ? 'Add all displayed values to the filter'
        : 'Remove all displayed values from the filter',
    [items, selectedItems]
  );

  const styles = useStyles2(getStyles);
  const theme = useTheme2();
  const gutter = theme.spacing.gridSize;
  const height = useMemo(() => Math.min(items.length * ITEM_HEIGHT, MIN_HEIGHT) + gutter, [gutter, items.length]);

  const onCheckedChanged = useCallback(
    (option: SelectableValue) => (event: React.FormEvent<HTMLInputElement>) => {
      const newValues = event.currentTarget.checked
        ? values.concat(option)
        : values.filter((c) => c.value !== option.value);

      onChange(newValues);
    },
    [onChange, values]
  );

  const onSelectChanged = useCallback(() => {
    if (items.length === selectedItems.length) {
      const newValues = values.filter((item) => !items.includes(item));
      onChange(newValues);
    } else {
      const newValues = [...new Set([...values, ...items])];
      onChange(newValues);
    }
  }, [onChange, values, items, selectedItems]);

  return (
    <VerticalGroup spacing="md">
      {!showOperators && <FilterInput placeholder="Filter values" onChange={setSearchFilter} value={searchFilter} />}
      {showOperators && (
        <HorizontalGroup>
          <ButtonSelect<string>
            variant="canvas"
            options={OPERATORS}
            onChange={setOperator}
            value={operator}
            tooltip={operator.description}
          />
          <FilterInput placeholder="Filter values" onChange={setSearchFilter} value={searchFilter} />
        </HorizontalGroup>
      )}
      {!items.length && <Label>No values</Label>}
      {items.length && (
        <List
          height={height}
          itemCount={items.length}
          itemSize={ITEM_HEIGHT}
          width="100%"
          className={styles.filterList}
        >
          {({ index, style }) => {
            const option = items[index];
            const { value, label } = option;
            const isChecked = values.find((s) => s.value === value) !== undefined;

            return (
              <div className={styles.filterListRow} style={style} title={label}>
                <Checkbox value={isChecked} label={label} onChange={onCheckedChanged(option)} />
              </div>
            );
          }}
        </List>
      )}
      {items.length && (
        <VerticalGroup spacing="xs">
          <div className={cx(styles.selectDivider)} />
          <div className={cx(styles.filterListRow)}>
            <Checkbox
              value={selectCheckValue}
              indeterminate={selectCheckIndeterminate}
              label={selectCheckLabel}
              description={selectCheckDescription}
              onChange={onSelectChanged}
            />
          </div>
        </VerticalGroup>
      )}
    </VerticalGroup>
  );
};

const getStyles = (theme: GrafanaTheme2) => ({
  filterList: css({
    label: 'filterList',
  }),
  filterListRow: css({
    label: 'filterListRow',
    cursor: 'pointer',
    whiteSpace: 'nowrap',
    overflow: 'hidden',
    textOverflow: 'ellipsis',
    padding: theme.spacing(0.5),

    ':hover': {
      backgroundColor: theme.colors.action.hover,
    },
  }),
  selectDivider: css({
    label: 'selectDivider',
    width: '100%',
    borderTop: `1px solid ${theme.colors.border.medium}`,
    padding: theme.spacing(0.5, 2),
  }),
});
