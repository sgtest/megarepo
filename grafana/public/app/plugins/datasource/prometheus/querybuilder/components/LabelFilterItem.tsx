import debounce from 'debounce-promise';
import React, { useState } from 'react';

import { SelectableValue, toOption } from '@grafana/data';
import { selectors } from '@grafana/e2e-selectors';
import { AccessoryButton, InputGroup } from '@grafana/experimental';
import { AsyncSelect, Select } from '@grafana/ui';

import { truncateResult } from '../../language_utils';
import { QueryBuilderLabelFilter } from '../shared/types';

export interface Props {
  defaultOp: string;
  item: Partial<QueryBuilderLabelFilter>;
  onChange: (value: QueryBuilderLabelFilter) => void;
  onGetLabelNames: (forLabel: Partial<QueryBuilderLabelFilter>) => Promise<SelectableValue[]>;
  onGetLabelValues: (forLabel: Partial<QueryBuilderLabelFilter>) => Promise<SelectableValue[]>;
  onDelete: () => void;
  invalidLabel?: boolean;
  invalidValue?: boolean;
  getLabelValuesAutofillSuggestions: (query: string, labelName?: string) => Promise<SelectableValue[]>;
  debounceDuration: number;
}

export function LabelFilterItem({
  item,
  defaultOp,
  onChange,
  onDelete,
  onGetLabelNames,
  onGetLabelValues,
  invalidLabel,
  invalidValue,
  getLabelValuesAutofillSuggestions,
  debounceDuration,
}: Props) {
  const [state, setState] = useState<{
    labelNames?: SelectableValue[];
    labelValues?: SelectableValue[];
    isLoadingLabelNames?: boolean;
    isLoadingLabelValues?: boolean;
  }>({});
  // there's a bug in react-select where the menu doesn't recalculate its position when the options are loaded asynchronously
  // see https://github.com/grafana/grafana/issues/63558
  // instead, we explicitly control the menu visibility and prevent showing it until the options have fully loaded
  const [labelNamesMenuOpen, setLabelNamesMenuOpen] = useState(false);
  const [labelValuesMenuOpen, setLabelValuesMenuOpen] = useState(false);

  const isMultiSelect = (operator = item.op) => {
    return operators.find((op) => op.label === operator)?.isMultiValue;
  };

  const getSelectOptionsFromString = (item?: string): string[] => {
    if (item) {
      if (item.indexOf('|') > 0) {
        return item.split('|');
      }
      return [item];
    }
    return [];
  };

  const labelValueSearch = debounce(
    (query: string) => getLabelValuesAutofillSuggestions(query, item.label),
    debounceDuration
  );

  return (
    <div data-testid="prometheus-dimensions-filter-item">
      <InputGroup>
        {/* Label name select, loads all values at once */}
        <Select
          placeholder="Select label"
          aria-label={selectors.components.QueryBuilder.labelSelect}
          inputId="prometheus-dimensions-filter-item-key"
          width="auto"
          value={item.label ? toOption(item.label) : null}
          allowCustomValue
          onOpenMenu={async () => {
            setState({ isLoadingLabelNames: true });
            const labelNames = await onGetLabelNames(item);
            setLabelNamesMenuOpen(true);
            setState({ labelNames, isLoadingLabelNames: undefined });
          }}
          onCloseMenu={() => {
            setLabelNamesMenuOpen(false);
          }}
          isOpen={labelNamesMenuOpen}
          isLoading={state.isLoadingLabelNames ?? false}
          options={state.labelNames}
          onChange={(change) => {
            if (change.label) {
              onChange({
                ...item,
                op: item.op ?? defaultOp,
                label: change.label,
                // eslint-ignore
              } as QueryBuilderLabelFilter);
            }
          }}
          invalid={invalidLabel}
        />

        {/* Operator select i.e.   = =~ != !~   */}
        <Select
          aria-label={selectors.components.QueryBuilder.matchOperatorSelect}
          className="query-segment-operator"
          value={toOption(item.op ?? defaultOp)}
          options={operators}
          width="auto"
          onChange={(change) => {
            if (change.value != null) {
              onChange({
                ...item,
                op: change.value,
                value: isMultiSelect(change.value) ? item.value : getSelectOptionsFromString(item?.value)[0],
                // eslint-ignore
              } as QueryBuilderLabelFilter);
            }
          }}
        />

        {/* Label value async select: autocomplete calls prometheus API */}
        <AsyncSelect
          placeholder="Select value"
          aria-label={selectors.components.QueryBuilder.valueSelect}
          inputId="prometheus-dimensions-filter-item-value"
          width="auto"
          value={
            isMultiSelect()
              ? getSelectOptionsFromString(item?.value).map(toOption)
              : getSelectOptionsFromString(item?.value).map(toOption)[0]
          }
          allowCustomValue
          onOpenMenu={async () => {
            setState({ isLoadingLabelValues: true });
            const labelValues = await onGetLabelValues(item);
            truncateResult(labelValues);
            setLabelValuesMenuOpen(true);
            setState({
              ...state,
              labelValues,
              isLoadingLabelValues: undefined,
            });
          }}
          onCloseMenu={() => {
            setLabelValuesMenuOpen(false);
          }}
          isOpen={labelValuesMenuOpen}
          defaultOptions={state.labelValues}
          isMulti={isMultiSelect()}
          isLoading={state.isLoadingLabelValues}
          loadOptions={labelValueSearch}
          onChange={(change) => {
            if (change.value) {
              onChange({
                ...item,
                value: change.value,
                op: item.op ?? defaultOp,
                // eslint-ignore
              } as QueryBuilderLabelFilter);
            } else {
              const changes = change
                .map((change: { label?: string }) => {
                  return change.label;
                })
                .join('|');
              // eslint-ignore
              onChange({ ...item, value: changes, op: item.op ?? defaultOp } as QueryBuilderLabelFilter);
            }
          }}
          invalid={invalidValue}
        />
        <AccessoryButton aria-label="remove" icon="times" variant="secondary" onClick={onDelete} />
      </InputGroup>
    </div>
  );
}

const operators = [
  { label: '=', value: '=', isMultiValue: false },
  { label: '!=', value: '!=', isMultiValue: false },
  { label: '<', value: '<', isMultiValue: false },
  { label: '>', value: '>', isMultiValue: false },
  { label: '=~', value: '=~', isMultiValue: true },
  { label: '!~', value: '!~', isMultiValue: true },
];
