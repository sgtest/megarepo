import { css, cx } from '@emotion/css';
import React from 'react';

import { GrafanaTheme2 } from '@grafana/data';

import { useStyles2 } from '../../themes';

import { ColorIndicator } from './types';
import { getColorIndicatorClass } from './utils';

interface Props {
  color: string;
  colorIndicator: ColorIndicator;
}

export type ColorIndicatorStyles = ReturnType<typeof getStyles>;

export const VizTooltipColorIndicator = ({ color, colorIndicator = ColorIndicator.value }: Props) => {
  const styles = useStyles2(getStyles);

  return (
    <span
      style={{ backgroundColor: color }}
      className={cx(styles.colorIndicator, getColorIndicatorClass(colorIndicator, styles))}
    />
  );
};

// @TODO Update classes/add svgs
const getStyles = (theme: GrafanaTheme2) => ({
  colorIndicator: css({
    marginRight: theme.spacing(0.5),
  }),
  series: css({
    width: '14px',
    height: '4px',
    borderRadius: theme.shape.radius.pill,
    minWidth: '14px',
  }),
  value: css({
    width: '12px',
    height: '12px',
    borderRadius: theme.shape.radius.default,
    fontWeight: 500,
    minWidth: '12px',
  }),
  hexagon: css({}),
  pie_1_4: css({}),
  pie_2_4: css({}),
  pie_3_4: css({}),
  marker_sm: css({
    width: '4px',
    height: '4px',
    borderRadius: theme.shape.radius.circle,
    minWidth: '4px',
  }),
  marker_md: css({
    width: '8px',
    height: '8px',
    borderRadius: theme.shape.radius.circle,
    minWidth: '8px',
  }),
  marker_lg: css({
    width: '12px',
    height: '12px',
    borderRadius: theme.shape.radius.circle,
    minWidth: '12px',
  }),
});
