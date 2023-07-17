import { css, cx } from '@emotion/css';
import React, { HTMLAttributes, ReactNode } from 'react';

import { GrafanaTheme2 } from '@grafana/data';

import { stylesFactory, useTheme2 } from '../../themes';

interface Props extends HTMLAttributes<HTMLDivElement> {
  children: ReactNode;
}

const getTabContentStyle = stylesFactory((theme: GrafanaTheme2) => {
  return {
    tabContent: css({
      background: theme.colors.background.primary,
    }),
  };
});

export const TabContent = ({ children, className, ...restProps }: Props) => {
  const theme = useTheme2();
  const styles = getTabContentStyle(theme);

  return (
    <div {...restProps} className={cx(styles.tabContent, className)}>
      {children}
    </div>
  );
};
