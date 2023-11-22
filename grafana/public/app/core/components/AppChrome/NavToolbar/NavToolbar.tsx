import { css } from '@emotion/css';
import React, { useState } from 'react';

import { GrafanaTheme2, NavModelItem } from '@grafana/data';
import { Components } from '@grafana/e2e-selectors';
import { Icon, IconButton, ToolbarButton, useStyles2, useTheme2 } from '@grafana/ui';
import { useGrafana } from 'app/core/context/GrafanaContext';
import { useMediaQueryChange } from 'app/core/hooks/useMediaQueryChange';
import { t } from 'app/core/internationalization';
import { HOME_NAV_ID } from 'app/core/reducers/navModel';
import { useSelector } from 'app/types';

import { Breadcrumbs } from '../../Breadcrumbs/Breadcrumbs';
import { buildBreadcrumbs } from '../../Breadcrumbs/utils';
import { TOP_BAR_LEVEL_HEIGHT } from '../types';

import { NavToolbarSeparator } from './NavToolbarSeparator';

export const TOGGLE_BUTTON_ID = 'mega-menu-toggle';

export interface Props {
  onToggleSearchBar(): void;
  onToggleMegaMenu(): void;
  onToggleKioskMode(): void;
  searchBarHidden?: boolean;
  sectionNav: NavModelItem;
  pageNav?: NavModelItem;
  actions: React.ReactNode;
}

export function NavToolbar({
  actions,
  searchBarHidden,
  sectionNav,
  pageNav,
  onToggleMegaMenu,
  onToggleSearchBar,
  onToggleKioskMode,
}: Props) {
  const { chrome } = useGrafana();
  const state = chrome.useState();
  const homeNav = useSelector((state) => state.navIndex)[HOME_NAV_ID];
  const theme = useTheme2();
  const styles = useStyles2(getStyles);
  const breadcrumbs = buildBreadcrumbs(sectionNav, pageNav, homeNav);

  const dockMenuBreakpoint = theme.breakpoints.values.xl;
  const [isTooSmallForDockedMenu, setIsTooSmallForDockedMenu] = useState(
    !window.matchMedia(`(min-width: ${dockMenuBreakpoint}px)`).matches
  );

  useMediaQueryChange({
    breakpoint: dockMenuBreakpoint,
    onChange: (e) => {
      setIsTooSmallForDockedMenu(!e.matches);
    },
  });

  return (
    <div data-testid={Components.NavToolbar.container} className={styles.pageToolbar}>
      <div className={styles.menuButton}>
        <IconButton
          id={TOGGLE_BUTTON_ID}
          name="bars"
          tooltip={
            state.megaMenu === 'closed' || (state.megaMenu === 'docked' && isTooSmallForDockedMenu)
              ? t('navigation.toolbar.open-menu', 'Open menu')
              : t('navigation.toolbar.close-menu', 'Close menu')
          }
          tooltipPlacement="bottom"
          size="xl"
          onClick={onToggleMegaMenu}
          data-testid={Components.NavBar.Toggle.button}
        />
      </div>
      <Breadcrumbs breadcrumbs={breadcrumbs} className={styles.breadcrumbsWrapper} />
      <div className={styles.actions}>
        {actions}
        {actions && <NavToolbarSeparator />}
        {searchBarHidden && (
          <ToolbarButton
            onClick={onToggleKioskMode}
            narrow
            title={t('navigation.toolbar.enable-kiosk', 'Enable kiosk mode')}
            icon="monitor"
          />
        )}
        <ToolbarButton
          onClick={onToggleSearchBar}
          narrow
          title={t('navigation.toolbar.toggle-search-bar', 'Toggle top search bar')}
        >
          <Icon name={searchBarHidden ? 'angle-down' : 'angle-up'} size="xl" />
        </ToolbarButton>
      </div>
    </div>
  );
}

const getStyles = (theme: GrafanaTheme2) => {
  return {
    breadcrumbsWrapper: css({
      display: 'flex',
      overflow: 'hidden',
      [theme.breakpoints.down('sm')]: {
        minWidth: '50%',
      },
    }),
    pageToolbar: css({
      height: TOP_BAR_LEVEL_HEIGHT,
      display: 'flex',
      padding: theme.spacing(0, 1, 0, 2),
      alignItems: 'center',
    }),
    menuButton: css({
      display: 'flex',
      alignItems: 'center',
      marginRight: theme.spacing(1),
    }),
    actions: css({
      label: 'NavToolbar-actions',
      display: 'flex',
      alignItems: 'center',
      flexWrap: 'nowrap',
      justifyContent: 'flex-end',
      paddingLeft: theme.spacing(1),
      flexGrow: 1,
      gap: theme.spacing(0.5),
      minWidth: 0,

      '.body-drawer-open &': {
        display: 'none',
      },
    }),
  };
};
