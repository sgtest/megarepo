import { css } from '@emotion/css';
import React from 'react';

import { GrafanaTheme2, ThemeSpacingTokens } from '@grafana/data';

import { useStyles2 } from '../../../themes';
import { ResponsiveProp, getResponsiveStyle } from '../utils/responsiveness';

export type AlignItems =
  | 'stretch'
  | 'flex-start'
  | 'flex-end'
  | 'center'
  | 'baseline'
  | 'start'
  | 'end'
  | 'self-start'
  | 'self-end';

export type JustifyContent =
  | 'flex-start'
  | 'flex-end'
  | 'center'
  | 'space-between'
  | 'space-around'
  | 'space-evenly'
  | 'start'
  | 'end'
  | 'left'
  | 'right';

export type Direction = 'row' | 'row-reverse' | 'column' | 'column-reverse';

export type Wrap = 'nowrap' | 'wrap' | 'wrap-reverse';

interface FlexProps extends Omit<React.HTMLAttributes<HTMLElement>, 'className' | 'style'> {
  gap?: ResponsiveProp<ThemeSpacingTokens>;
  alignItems?: ResponsiveProp<AlignItems>;
  justifyContent?: ResponsiveProp<JustifyContent>;
  direction?: ResponsiveProp<Direction>;
  wrap?: ResponsiveProp<Wrap>;
  children?: React.ReactNode;
}

export const Flex = React.forwardRef<HTMLDivElement, FlexProps>(
  ({ gap = 1, alignItems, justifyContent, direction, wrap, children, ...rest }, ref) => {
    const styles = useStyles2(getStyles, gap, alignItems, justifyContent, direction, wrap);

    return (
      <div ref={ref} className={styles.flex} {...rest}>
        {children}
      </div>
    );
  }
);

Flex.displayName = 'Flex';

const getStyles = (
  theme: GrafanaTheme2,
  gap: FlexProps['gap'],
  alignItems: FlexProps['alignItems'],
  justifyContent: FlexProps['justifyContent'],
  direction: FlexProps['direction'],
  wrap: FlexProps['wrap']
) => {
  return {
    flex: css([
      {
        display: 'flex',
      },
      getResponsiveStyle<Direction>(theme, direction, (val) => ({
        flexDirection: val,
      })),
      getResponsiveStyle<Wrap>(theme, wrap, (val) => ({
        flexWrap: val,
      })),
      getResponsiveStyle<AlignItems>(theme, alignItems, (val) => ({
        alignItems: val,
      })),
      getResponsiveStyle<JustifyContent>(theme, justifyContent, (val) => ({
        justifyContent: val,
      })),
      getResponsiveStyle<ThemeSpacingTokens>(theme, gap, (val) => ({
        gap: theme.spacing(val),
      })),
    ]),
  };
};
