import { css } from '@emotion/css';
import React, { ReactNode } from 'react';

import { TimeOption } from '@grafana/data';

import { stylesFactory } from '../../../themes';
import { t } from '../../../utils/i18n';

import { TimePickerTitle } from './TimePickerTitle';
import { TimeRangeOption } from './TimeRangeOption';

const getStyles = stylesFactory(() => {
  return {
    title: css({
      display: 'flex',
      alignItems: 'center',
      justifyContent: 'space-between',
      padding: '8px 16px 5px 9px',
    }),
  };
});

const getOptionsStyles = stylesFactory(() => {
  return {
    grow: css({
      flexGrow: 1,
      alignItems: 'flex-start',
    }),
  };
});

interface Props {
  title?: string;
  options: TimeOption[];
  value?: TimeOption;
  onChange: (option: TimeOption) => void;
  placeholderEmpty?: ReactNode;
}

export const TimeRangeList = (props: Props) => {
  const styles = getStyles();
  const { title, options, placeholderEmpty } = props;

  if (typeof placeholderEmpty !== 'undefined' && options.length <= 0) {
    return <>{placeholderEmpty}</>;
  }

  if (!title) {
    return <Options {...props} />;
  }

  return (
    <section aria-label={title}>
      <fieldset>
        <div className={styles.title}>
          <TimePickerTitle>{title}</TimePickerTitle>
        </div>
        <Options {...props} />
      </fieldset>
    </section>
  );
};

const Options = ({ options, value, onChange, title }: Props) => {
  const styles = getOptionsStyles();

  return (
    <>
      <ul aria-roledescription={t('time-picker.time-range.aria-role', 'Time range selection')}>
        {options.map((option, index) => (
          <TimeRangeOption
            key={keyForOption(option, index)}
            value={option}
            selected={isEqual(option, value)}
            onSelect={onChange}
            name={title ?? t('time-picker.time-range.default-title', 'Time ranges')}
          />
        ))}
      </ul>
      <div className={styles.grow} />
    </>
  );
};

function keyForOption(option: TimeOption, index: number): string {
  return `${option.from}-${option.to}-${index}`;
}

function isEqual(x: TimeOption, y?: TimeOption): boolean {
  if (!y || !x) {
    return false;
  }
  return y.from === x.from && y.to === x.to;
}
