import { css } from '@emotion/css';
import { DOMAttributes } from '@react-types/shared';
import React, { forwardRef } from 'react';
import { useLocation } from 'react-router-dom';

import { GrafanaTheme2 } from '@grafana/data';
import { selectors } from '@grafana/e2e-selectors';
import { CustomScrollbar, Icon, IconButton, useStyles2, Stack } from '@grafana/ui';
import { useGrafana } from 'app/core/context/GrafanaContext';
import { t } from 'app/core/internationalization';
import { useSelector } from 'app/types';

import { MegaMenuItem } from './MegaMenuItem';
import { enrichWithInteractionTracking, getActiveItem } from './utils';

export const MENU_WIDTH = '300px';

export interface Props extends DOMAttributes {
  onClose: () => void;
}

export const MegaMenu = React.memo(
  forwardRef<HTMLDivElement, Props>(({ onClose, ...restProps }, ref) => {
    const navTree = useSelector((state) => state.navBarTree);
    const styles = useStyles2(getStyles);
    const location = useLocation();
    const { chrome } = useGrafana();
    const state = chrome.useState();

    // Remove profile + help from tree
    const navItems = navTree
      .filter((item) => item.id !== 'profile' && item.id !== 'help')
      .map((item) => enrichWithInteractionTracking(item, state.megaMenu));

    const activeItem = getActiveItem(navItems, location.pathname);

    const handleDockedMenu = () => {
      chrome.setMegaMenu(state.megaMenu === 'docked' ? 'open' : 'docked');

      // refocus on dock/undock button when changing state
      setTimeout(() => {
        document.getElementById('dock-menu-button')?.focus();
      });
    };

    return (
      <div data-testid={selectors.components.NavMenu.Menu} ref={ref} {...restProps}>
        <div className={styles.mobileHeader}>
          <Icon name="bars" size="xl" />
          <IconButton
            tooltip={t('navigation.megamenu.close', 'Close menu')}
            name="times"
            onClick={onClose}
            size="xl"
            variant="secondary"
          />
        </div>
        <nav className={styles.content}>
          <CustomScrollbar showScrollIndicators hideHorizontalTrack>
            <ul className={styles.itemList}>
              {navItems.map((link, index) => (
                <Stack key={link.text} direction={index === 0 ? 'row-reverse' : 'row'} alignItems="center">
                  {index === 0 && (
                    <IconButton
                      id="dock-menu-button"
                      className={styles.dockMenuButton}
                      tooltip={
                        state.megaMenu === 'docked'
                          ? t('navigation.megamenu.undock', 'Undock menu')
                          : t('navigation.megamenu.dock', 'Dock menu')
                      }
                      name="web-section-alt"
                      onClick={handleDockedMenu}
                      variant="secondary"
                    />
                  )}
                  <MegaMenuItem
                    link={link}
                    onClick={state.megaMenu === 'open' ? onClose : undefined}
                    activeItem={activeItem}
                  />
                </Stack>
              ))}
            </ul>
          </CustomScrollbar>
        </nav>
      </div>
    );
  })
);

MegaMenu.displayName = 'MegaMenu';

const getStyles = (theme: GrafanaTheme2) => ({
  content: css({
    display: 'flex',
    flexDirection: 'column',
    height: '100%',
    minHeight: 0,
    position: 'relative',
  }),
  mobileHeader: css({
    display: 'flex',
    justifyContent: 'space-between',
    padding: theme.spacing(1, 1, 1, 2),
    borderBottom: `1px solid ${theme.colors.border.weak}`,

    [theme.breakpoints.up('md')]: {
      display: 'none',
    },
  }),
  itemList: css({
    boxSizing: 'border-box',
    display: 'flex',
    flexDirection: 'column',
    listStyleType: 'none',
    padding: theme.spacing(1),
    [theme.breakpoints.up('md')]: {
      width: MENU_WIDTH,
    },
  }),
  dockMenuButton: css({
    display: 'none',

    [theme.breakpoints.up('xl')]: {
      display: 'inline-flex',
    },
  }),
});
