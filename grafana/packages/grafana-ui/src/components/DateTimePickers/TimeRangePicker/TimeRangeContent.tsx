import { css } from '@emotion/css';
import React, { FormEvent, useCallback, useEffect, useState } from 'react';

import {
  DateTime,
  dateTimeFormat,
  dateTimeParse,
  GrafanaTheme2,
  isDateTime,
  rangeUtil,
  RawTimeRange,
  TimeRange,
  TimeZone,
} from '@grafana/data';
import { selectors } from '@grafana/e2e-selectors';

import { Icon, Tooltip } from '../..';
import { useStyles2 } from '../../..';
import { t, Trans } from '../../../utils/i18n';
import { Button } from '../../Button';
import { Field } from '../../Forms/Field';
import { Input } from '../../Input/Input';
import { isValid } from '../utils';

import TimePickerCalendar from './TimePickerCalendar';

interface Props {
  isFullscreen: boolean;
  value: TimeRange;
  onApply: (range: TimeRange) => void;
  timeZone?: TimeZone;
  fiscalYearStartMonth?: number;
  roundup?: boolean;
  isReversed?: boolean;
}

interface InputState {
  value: string;
  invalid: boolean;
  errorMessage: string;
}

const ERROR_MESSAGES = {
  default: () => t('time-picker.range-content.default-error', 'Please enter a past date or "now"'),
  range: () => t('time-picker.range-content.range-error', '"From" can\'t be after "To"'),
};

export const TimeRangeContent = (props: Props) => {
  const { value, isFullscreen = false, timeZone, onApply: onApplyFromProps, isReversed, fiscalYearStartMonth } = props;
  const [fromValue, toValue] = valueToState(value.raw.from, value.raw.to, timeZone);
  const style = useStyles2(getStyles);

  const [from, setFrom] = useState<InputState>(fromValue);
  const [to, setTo] = useState<InputState>(toValue);
  const [isOpen, setOpen] = useState(false);

  // Synchronize internal state with external value
  useEffect(() => {
    const [fromValue, toValue] = valueToState(value.raw.from, value.raw.to, timeZone);
    setFrom(fromValue);
    setTo(toValue);
  }, [value.raw.from, value.raw.to, timeZone]);

  const onOpen = useCallback(
    (event: FormEvent<HTMLElement>) => {
      event.preventDefault();
      setOpen(true);
    },
    [setOpen]
  );

  const onApply = useCallback(() => {
    if (to.invalid || from.invalid) {
      return;
    }

    const raw: RawTimeRange = { from: from.value, to: to.value };
    const timeRange = rangeUtil.convertRawToRange(raw, timeZone, fiscalYearStartMonth);

    onApplyFromProps(timeRange);
  }, [from.invalid, from.value, onApplyFromProps, timeZone, to.invalid, to.value, fiscalYearStartMonth]);

  const onChange = useCallback(
    (from: DateTime | string, to: DateTime | string) => {
      const [fromValue, toValue] = valueToState(from, to, timeZone);
      setFrom(fromValue);
      setTo(toValue);
    },
    [timeZone]
  );

  const submitOnEnter = (event: React.KeyboardEvent<HTMLInputElement>) => {
    if (event.key === 'Enter') {
      onApply();
    }
  };

  const fiscalYear = rangeUtil.convertRawToRange({ from: 'now/fy', to: 'now/fy' }, timeZone, fiscalYearStartMonth);
  const fiscalYearMessage = t('time-picker.range-content.fiscal-year', 'Fiscal year');

  const fyTooltip = (
    <div className={style.tooltip}>
      {rangeUtil.isFiscal(value) ? (
        <Tooltip
          content={`${fiscalYearMessage}: ${fiscalYear.from.format('MMM-DD')} - ${fiscalYear.to.format('MMM-DD')}`}
        >
          <Icon name="info-circle" />
        </Tooltip>
      ) : null}
    </div>
  );

  const icon = (
    <Button
      aria-label={selectors.components.TimePicker.calendar.openButton}
      icon="calendar-alt"
      variant="secondary"
      type="button"
      onClick={onOpen}
    />
  );

  return (
    <div>
      <div className={style.fieldContainer}>
        <Field
          label={t('time-picker.range-content.from-input', 'From')}
          invalid={from.invalid}
          error={from.errorMessage}
        >
          <Input
            onClick={(event) => event.stopPropagation()}
            onChange={(event) => onChange(event.currentTarget.value, to.value)}
            addonAfter={icon}
            onKeyDown={submitOnEnter}
            aria-label={selectors.components.TimePicker.fromField}
            value={from.value}
          />
        </Field>
        {fyTooltip}
      </div>
      <div className={style.fieldContainer}>
        <Field label={t('time-picker.range-content.to-input', 'To')} invalid={to.invalid} error={to.errorMessage}>
          <Input
            onClick={(event) => event.stopPropagation()}
            onChange={(event) => onChange(from.value, event.currentTarget.value)}
            addonAfter={icon}
            onKeyDown={submitOnEnter}
            aria-label={selectors.components.TimePicker.toField}
            value={to.value}
          />
        </Field>
        {fyTooltip}
      </div>
      <Button data-testid={selectors.components.TimePicker.applyTimeRange} type="button" onClick={onApply}>
        <Trans i18nKey="time-picker.range-content.apply-button">Apply time range</Trans>
      </Button>

      <TimePickerCalendar
        isFullscreen={isFullscreen}
        isOpen={isOpen}
        from={dateTimeParse(from.value, { timeZone })}
        to={dateTimeParse(to.value, { timeZone })}
        onApply={onApply}
        onClose={() => setOpen(false)}
        onChange={onChange}
        timeZone={timeZone}
        isReversed={isReversed}
      />
    </div>
  );
};

function isRangeInvalid(from: string, to: string, timezone?: string): boolean {
  const raw: RawTimeRange = { from, to };
  const timeRange = rangeUtil.convertRawToRange(raw, timezone);
  const valid = timeRange.from.isSame(timeRange.to) || timeRange.from.isBefore(timeRange.to);

  return !valid;
}

function valueToState(
  rawFrom: DateTime | string,
  rawTo: DateTime | string,
  timeZone?: TimeZone
): [InputState, InputState] {
  const fromValue = valueAsString(rawFrom, timeZone);
  const toValue = valueAsString(rawTo, timeZone);
  const fromInvalid = !isValid(fromValue, false, timeZone);
  const toInvalid = !isValid(toValue, true, timeZone);
  // If "To" is invalid, we should not check the range anyways
  const rangeInvalid = isRangeInvalid(fromValue, toValue, timeZone) && !toInvalid;

  return [
    {
      value: fromValue,
      invalid: fromInvalid || rangeInvalid,
      errorMessage: rangeInvalid && !fromInvalid ? ERROR_MESSAGES.range() : ERROR_MESSAGES.default(),
    },
    { value: toValue, invalid: toInvalid, errorMessage: ERROR_MESSAGES.default() },
  ];
}

function valueAsString(value: DateTime | string, timeZone?: TimeZone): string {
  if (isDateTime(value)) {
    return dateTimeFormat(value, { timeZone });
  }
  return value;
}

function getStyles(theme: GrafanaTheme2) {
  return {
    fieldContainer: css({
      display: 'flex',
    }),
    tooltip: css({
      paddingLeft: theme.spacing(1),
      paddingTop: theme.spacing(3),
    }),
  };
}
