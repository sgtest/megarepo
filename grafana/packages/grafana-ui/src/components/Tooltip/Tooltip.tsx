import React, { useCallback, useEffect } from 'react';
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
    const [controlledVisible, setControlledVisible] = React.useState(show);

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
      placement: placement,
      interactive: interactive,
      delayHide: interactive ? 100 : 0,
      delayShow: 150,
      offset: [0, 8],
      trigger: ['hover', 'focus'],
      onVisibleChange: setControlledVisible,
    });

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
          tabIndex: 0, // tooltip should be keyboard focusable
        })}
        {visible && (
          <Portal>
            <div
              data-testid={selectors.components.Tooltip.container}
              ref={setTooltipRef}
              {...getTooltipProps({ className: style.container })}
            >
              <div {...getArrowProps({ className: style.arrow })} />
              {typeof content === 'string' && content}
              {React.isValidElement(content) && React.cloneElement(content)}
              {typeof content === 'function' &&
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
