import { useCallback } from 'react';

import { SelectableValue } from '@grafana/data';
import { selectors } from '@grafana/e2e-selectors';

import { Select } from '../Select/Select';

export interface Props {
  onChange: (weekStart: string) => void;
  value: string;
  width?: number;
  autoFocus?: boolean;
  onBlur?: () => void;
  disabled?: boolean;
  inputId?: string;
}

export type WeekStart = 'saturday' | 'sunday' | 'monday';
const weekStarts: Array<SelectableValue<WeekStart | ''>> = [
  { value: '', label: 'Default' },
  { value: 'saturday', label: 'Saturday' },
  { value: 'sunday', label: 'Sunday' },
  { value: 'monday', label: 'Monday' },
];

const isWeekStart = (value: string): value is WeekStart => {
  return ['saturday', 'sunday', 'monday'].includes(value);
};

export const getWeekStart = (value: string): WeekStart => {
  if (isWeekStart(value)) {
    return value;
  }

  return 'monday';
};

export const WeekStartPicker = (props: Props) => {
  const { onChange, width, autoFocus = false, onBlur, value, disabled = false, inputId } = props;

  const onChangeWeekStart = useCallback(
    (selectable: SelectableValue<string>) => {
      if (selectable.value !== undefined) {
        onChange(selectable.value);
      }
    },
    [onChange]
  );

  return (
    <Select
      inputId={inputId}
      value={weekStarts.find((item) => item.value === value)?.value}
      placeholder={selectors.components.WeekStartPicker.placeholder}
      autoFocus={autoFocus}
      openMenuOnFocus={true}
      width={width}
      options={weekStarts}
      onChange={onChangeWeekStart}
      onBlur={onBlur}
      disabled={disabled}
    />
  );
};
