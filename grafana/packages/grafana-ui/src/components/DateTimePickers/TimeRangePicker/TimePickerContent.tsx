import { css, cx } from '@emotion/css';
import React, { memo, useMemo, useState } from 'react';

import { GrafanaTheme2, isDateTime, rangeUtil, RawTimeRange, TimeOption, TimeRange, TimeZone } from '@grafana/data';
import { selectors } from '@grafana/e2e-selectors';

import { FilterInput } from '../..';
import { stylesFactory, useTheme2 } from '../../../themes';
import { getFocusStyles } from '../../../themes/mixins';
import { t, Trans } from '../../../utils/i18n';
import { CustomScrollbar } from '../../CustomScrollbar/CustomScrollbar';
import { Icon } from '../../Icon/Icon';

import { TimePickerFooter } from './TimePickerFooter';
import { TimePickerTitle } from './TimePickerTitle';
import { TimeRangeContent } from './TimeRangeContent';
import { TimeRangeList } from './TimeRangeList';
import { mapOptionToTimeRange, mapRangeToTimeOption } from './mapper';

interface Props {
  value: TimeRange;
  onChange: (timeRange: TimeRange) => void;
  onChangeTimeZone: (timeZone: TimeZone) => void;
  onChangeFiscalYearStartMonth?: (month: number) => void;
  timeZone?: TimeZone;
  fiscalYearStartMonth?: number;
  quickOptions?: TimeOption[];
  history?: TimeRange[];
  showHistory?: boolean;
  className?: string;
  hideTimeZone?: boolean;
  /** Reverse the order of relative and absolute range pickers. Used to left align the picker in forms */
  isReversed?: boolean;
  hideQuickRanges?: boolean;
  widthOverride?: number;
}

export interface PropsWithScreenSize extends Props {
  isFullscreen: boolean;
}

interface FormProps extends Omit<Props, 'history'> {
  historyOptions?: TimeOption[];
}

export const TimePickerContentWithScreenSize = (props: PropsWithScreenSize) => {
  const {
    quickOptions = [],
    isReversed,
    isFullscreen,
    hideQuickRanges,
    timeZone,
    fiscalYearStartMonth,
    value,
    onChange,
    history,
    showHistory,
    className,
    hideTimeZone,
    onChangeTimeZone,
    onChangeFiscalYearStartMonth,
  } = props;
  const isHistoryEmpty = !history?.length;
  const isContainerTall =
    (isFullscreen && showHistory) || (!isFullscreen && ((showHistory && !isHistoryEmpty) || !hideQuickRanges));
  const theme = useTheme2();
  const styles = getStyles(theme, isReversed, hideQuickRanges, isContainerTall, isFullscreen);
  const historyOptions = mapToHistoryOptions(history, timeZone);
  const timeOption = useTimeOption(value.raw, quickOptions);
  const [searchTerm, setSearchQuery] = useState('');

  const filteredQuickOptions = quickOptions.filter((o) => o.display.toLowerCase().includes(searchTerm.toLowerCase()));

  const onChangeTimeOption = (timeOption: TimeOption) => {
    return onChange(mapOptionToTimeRange(timeOption));
  };

  return (
    <div id="TimePickerContent" className={cx(styles.container, className)}>
      <div className={styles.body}>
        {(!isFullscreen || !hideQuickRanges) && (
          <div className={styles.rightSide}>
            <div className={styles.timeRangeFilter}>
              <FilterInput
                width={0}
                autoFocus={true}
                value={searchTerm}
                onChange={setSearchQuery}
                placeholder={t('time-picker.content.filter-placeholder', 'Search quick ranges')}
              />
            </div>
            <CustomScrollbar>
              {!isFullscreen && <NarrowScreenForm {...props} historyOptions={historyOptions} />}
              {!hideQuickRanges && (
                <TimeRangeList options={filteredQuickOptions} onChange={onChangeTimeOption} value={timeOption} />
              )}
            </CustomScrollbar>
          </div>
        )}
        {isFullscreen && (
          <div className={styles.leftSide}>
            <FullScreenForm {...props} historyOptions={historyOptions} />
          </div>
        )}
      </div>
      {!hideTimeZone && isFullscreen && (
        <TimePickerFooter
          timeZone={timeZone}
          fiscalYearStartMonth={fiscalYearStartMonth}
          onChangeTimeZone={onChangeTimeZone}
          onChangeFiscalYearStartMonth={onChangeFiscalYearStartMonth}
        />
      )}
    </div>
  );
};

export const TimePickerContent = (props: Props) => {
  const { widthOverride } = props;
  const theme = useTheme2();
  const isFullscreen = (widthOverride || window.innerWidth) >= theme.breakpoints.values.lg;
  return <TimePickerContentWithScreenSize {...props} isFullscreen={isFullscreen} />;
};

const NarrowScreenForm = (props: FormProps) => {
  const { value, hideQuickRanges, onChange, timeZone, historyOptions = [], showHistory } = props;
  const theme = useTheme2();
  const styles = getNarrowScreenStyles(theme);
  const isAbsolute = isDateTime(value.raw.from) || isDateTime(value.raw.to);
  const [collapsedFlag, setCollapsedFlag] = useState(!isAbsolute);
  const collapsed = hideQuickRanges ? false : collapsedFlag;

  const onChangeTimeOption = (timeOption: TimeOption) => {
    return onChange(mapOptionToTimeRange(timeOption, timeZone));
  };

  return (
    <fieldset>
      <div className={styles.header}>
        <button
          type={'button'}
          className={styles.expandButton}
          onClick={() => {
            if (!hideQuickRanges) {
              setCollapsedFlag(!collapsed);
            }
          }}
          data-testid={selectors.components.TimePicker.absoluteTimeRangeTitle}
          aria-expanded={!collapsed}
          aria-controls="expanded-timerange"
        >
          <TimePickerTitle>
            <Trans i18nKey="time-picker.absolute.title">Absolute time range</Trans>
          </TimePickerTitle>
          {!hideQuickRanges && <Icon name={!collapsed ? 'angle-up' : 'angle-down'} />}
        </button>
      </div>
      {!collapsed && (
        <div className={styles.body} id="expanded-timerange">
          <div className={styles.form}>
            <TimeRangeContent value={value} onApply={onChange} timeZone={timeZone} isFullscreen={false} />
          </div>
          {showHistory && (
            <TimeRangeList
              title={t('time-picker.absolute.recent-title', 'Recently used absolute ranges')}
              options={historyOptions}
              onChange={onChangeTimeOption}
              placeholderEmpty={null}
            />
          )}
        </div>
      )}
    </fieldset>
  );
};

const FullScreenForm = (props: FormProps) => {
  const { onChange, value, timeZone, fiscalYearStartMonth, isReversed, historyOptions } = props;
  const theme = useTheme2();
  const styles = getFullScreenStyles(theme, props.hideQuickRanges);
  const onChangeTimeOption = (timeOption: TimeOption) => {
    return onChange(mapOptionToTimeRange(timeOption, timeZone));
  };

  return (
    <>
      <div className={styles.container}>
        <div className={styles.title} data-testid={selectors.components.TimePicker.absoluteTimeRangeTitle}>
          <TimePickerTitle>
            <Trans i18nKey="time-picker.absolute.title">Absolute time range</Trans>
          </TimePickerTitle>
        </div>
        <TimeRangeContent
          value={value}
          timeZone={timeZone}
          fiscalYearStartMonth={fiscalYearStartMonth}
          onApply={onChange}
          isFullscreen={true}
          isReversed={isReversed}
        />
      </div>
      {props.showHistory && (
        <div className={styles.recent}>
          <TimeRangeList
            title={t('time-picker.absolute.recent-title', 'Recently used absolute ranges')}
            options={historyOptions || []}
            onChange={onChangeTimeOption}
            placeholderEmpty={<EmptyRecentList />}
          />
        </div>
      )}
    </>
  );
};

const EmptyRecentList = memo(() => {
  const theme = useTheme2();
  const styles = getEmptyListStyles(theme);

  return (
    <div className={styles.container}>
      <Trans i18nKey="time-picker.content.empty-recent-list">
        <div>
          <span>
            It looks like you haven&apos;t used this time picker before. As soon as you enter some time intervals,
            recently used intervals will appear here.
          </span>
        </div>
        <div>
          <a
            className={styles.link}
            href="https://grafana.com/docs/grafana/latest/dashboards/time-range-controls"
            target="_new"
          >
            Read the documentation
          </a>
          <span> to find out more about how to enter custom time ranges.</span>
        </div>
      </Trans>
    </div>
  );
});

function mapToHistoryOptions(ranges?: TimeRange[], timeZone?: TimeZone): TimeOption[] {
  if (!Array.isArray(ranges) || ranges.length === 0) {
    return [];
  }

  return ranges.map((range) => mapRangeToTimeOption(range, timeZone));
}

EmptyRecentList.displayName = 'EmptyRecentList';

const useTimeOption = (raw: RawTimeRange, quickOptions: TimeOption[]): TimeOption | undefined => {
  return useMemo(() => {
    if (!rangeUtil.isRelativeTimeRange(raw)) {
      return;
    }
    return quickOptions.find((option) => {
      return option.from === raw.from && option.to === raw.to;
    });
  }, [raw, quickOptions]);
};

const getStyles = stylesFactory((theme: GrafanaTheme2, isReversed, hideQuickRanges, isContainerTall, isFullscreen) => {
  return {
    container: css({
      background: theme.colors.background.primary,
      boxShadow: theme.shadows.z3,
      width: `${isFullscreen ? '546px' : '262px'}`,
      borderRadius: theme.shape.radius.default,
      border: `1px solid ${theme.colors.border.weak}`,
      [`${isReversed ? 'left' : 'right'}`]: 0,
    }),
    body: css({
      display: 'flex',
      flexDirection: 'row-reverse',
      height: `${isContainerTall ? '381px' : '217px'}`,
      maxHeight: '100vh',
    }),
    leftSide: css({
      display: 'flex',
      flexDirection: 'column',
      borderRight: `${isReversed ? 'none' : `1px solid ${theme.colors.border.weak}`}`,
      width: `${!hideQuickRanges ? '60%' : '100%'}`,
      overflow: 'hidden',
      order: isReversed ? 1 : 0,
    }),
    rightSide: css({
      width: `${isFullscreen ? '40%' : '100%'}; !important`,
      borderRight: isReversed ? `1px solid ${theme.colors.border.weak}` : 'none',
      display: 'flex',
      flexDirection: 'column',
    }),
    timeRangeFilter: css({
      padding: theme.spacing(1),
    }),
    spacing: css({
      marginTop: '16px',
    }),
  };
});

const getNarrowScreenStyles = stylesFactory((theme: GrafanaTheme2) => {
  return {
    header: css({
      display: 'flex',
      flexDirection: 'row',
      justifyContent: 'space-between',
      alignItems: 'center',
      borderBottom: `1px solid ${theme.colors.border.weak}`,
      padding: '7px 9px 7px 9px',
    }),
    expandButton: css({
      backgroundColor: 'transparent',
      border: 'none',
      display: 'flex',
      width: '100%',

      '&:focus-visible': getFocusStyles(theme),
    }),
    body: css({
      borderBottom: `1px solid ${theme.colors.border.weak}`,
    }),
    form: css({
      padding: '7px 9px 7px 9px',
    }),
  };
});

const getFullScreenStyles = stylesFactory((theme: GrafanaTheme2, hideQuickRanges?: boolean) => {
  return {
    container: css({
      paddingTop: '9px',
      paddingLeft: '11px',
      paddingRight: !hideQuickRanges ? '20%' : '11px',
    }),
    title: css({
      marginBottom: '11px',
    }),
    recent: css({
      flexGrow: 1,
      display: 'flex',
      flexDirection: 'column',
      justifyContent: 'flex-end',
      paddingTop: theme.spacing(1),
    }),
  };
});

const getEmptyListStyles = stylesFactory((theme: GrafanaTheme2) => {
  return {
    container: css({
      padding: '12px',
      margin: '12px',

      'a, span': {
        fontSize: '13px',
      },
    }),
    link: css({
      color: theme.colors.text.link,
    }),
  };
});
