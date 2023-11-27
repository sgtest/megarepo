import React, { useCallback, useEffect, useId, useState } from 'react';
import { usePopperTooltip } from 'react-popper-tooltip';

import { GrafanaTheme2 } from '@grafana/data';
import { selectors } from '@grafana/e2e-selectors';

import { useStyles2 } from '../../themes/ThemeContext';
import { buildTooltipTheme } from '../../utils/tooltipUtils';
import { Portal } from '../Portal/Portal';

import { PopoverContent, TooltipPlacement } from './types';

export interface TooltipProps {
  theme?: 'info' | 'error' | 'info-alt';
  show?: boolean;
  placement?: TooltipPlacement;
  content: PopoverContent;
  children: JSX.Element;
  /**
   * Set to true if you want the tooltip to stay long enough so the user can move mouse over content to select text or click a link
   */
  interactive?: boolean;
}

export const Tooltip = React.forwardRef<HTMLElement, TooltipProps>(
  ({ children, theme, interactive, show, placement, content }, forwardedRef) => {
    const [controlledVisible, setControlledVisible] = useState(show);
    const tooltipId = useId();

    useEffect(() => {
      if (controlledVisible !== false) {
        const handleKeyDown = (enterKey: KeyboardEvent) => {
          if (enterKey.key === 'Escape') {
            setControlledVisible(false);
          }
        };
        document.addEventListener('keydown', handleKeyDown);
        return () => {
          document.removeEventListener('keydown', handleKeyDown);
        };
      } else {
        return;
      }
    }, [controlledVisible]);

    const { getArrowProps, getTooltipProps, setTooltipRef, setTriggerRef, visible, update } = usePopperTooltip({
      visible: show ?? controlledVisible,
      placement,
      interactive,
      delayHide: interactive ? 100 : 0,
      offset: [0, 8],
      trigger: ['hover', 'focus'],
      onVisibleChange: setControlledVisible,
    });

    const contentIsFunction = typeof content === 'function';

    /**
     * If content is a function we need to call popper update function to make sure the tooltip is positioned correctly
     * if it's close to the viewport boundary
     **/
    useEffect(() => {
      if (update && contentIsFunction) {
        update();
      }
    }, [visible, update, contentIsFunction]);

    const styles = useStyles2(getStyles);
    const style = styles[theme ?? 'info'];

    const handleRef = useCallback(
      (ref: HTMLElement | null) => {
        setTriggerRef(ref);

        if (typeof forwardedRef === 'function') {
          forwardedRef(ref);
        } else if (forwardedRef) {
          forwardedRef.current = ref;
        }
      },
      [forwardedRef, setTriggerRef]
    );

    return (
      <>
        {React.cloneElement(children, {
          ref: handleRef,
          tabIndex: 0, // tooltip trigger should be keyboard focusable
          'aria-describedby': visible ? tooltipId : undefined,
        })}
        {visible && (
          <Portal>
            <div
              data-testid={selectors.components.Tooltip.container}
              ref={setTooltipRef}
              id={tooltipId}
              role="tooltip"
              {...getTooltipProps({ className: style.container })}
            >
              <div {...getArrowProps({ className: style.arrow })} />
              {typeof content === 'string' && content}
              {React.isValidElement(content) && React.cloneElement(content)}
              {contentIsFunction &&
                update &&
                content({
                  updatePopperPosition: update,
                })}
            </div>
          </Portal>
        )}
      </>
    );
  }
);

Tooltip.displayName = 'Tooltip';

export const getStyles = (theme: GrafanaTheme2) => {
  const info = buildTooltipTheme(
    theme,
    theme.components.tooltip.background,
    theme.components.tooltip.background,
    theme.components.tooltip.text,
    { topBottom: 0.5, rightLeft: 1 }
  );
  const error = buildTooltipTheme(
    theme,
    theme.colors.error.main,
    theme.colors.error.main,
    theme.colors.error.contrastText,
    { topBottom: 0.5, rightLeft: 1 }
  );

  return {
    info,
    ['info-alt']: info,
    error,
  };
};
