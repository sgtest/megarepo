import { cx, css } from '@emotion/css';
import React, { PureComponent, ReactElement } from 'react';

import { GrafanaTheme2 } from '@grafana/data';

import { stylesFactory, withTheme2 } from '../../themes';
import { Themeable2 } from '../../types';
import { ComponentSize } from '../../types/size';
import { Button, ButtonVariant } from '../Button';

export interface Props extends Themeable2 {
  /** Confirm action callback */
  onConfirm(): void;
  children: string | ReactElement;
  /** Custom button styles */
  className?: string;
  /** Button size */
  size?: ComponentSize;
  /** Text for the Confirm button */
  confirmText?: string;
  /** Disable button click action */
  disabled?: boolean;
  /** Variant of the Confirm button */
  confirmVariant?: ButtonVariant;
  /** Hide confirm actions when after of them is clicked */
  closeOnConfirm?: boolean;
  /** Move focus to button when mounted */
  autoFocus?: boolean;

  /** Optional on click handler for the original button */
  onClick?(): void;
  /** Callback for the cancel action */
  onCancel?(): void;
}

interface State {
  showConfirm: boolean;
}

class UnThemedConfirmButton extends PureComponent<Props, State> {
  mainButtonRef = React.createRef<HTMLButtonElement>();
  confirmButtonRef = React.createRef<HTMLButtonElement>();
  state: State = {
    showConfirm: false,
  };

  onClickButton = (event: React.MouseEvent<HTMLButtonElement>) => {
    if (event) {
      event.preventDefault();
    }

    this.setState(
      {
        showConfirm: true,
      },
      () => {
        if (this.props.autoFocus && this.confirmButtonRef.current) {
          this.confirmButtonRef.current.focus();
        }
      }
    );

    if (this.props.onClick) {
      this.props.onClick();
    }
  };

  onClickCancel = (event: React.MouseEvent<HTMLButtonElement>) => {
    if (event) {
      event.preventDefault();
    }
    this.setState(
      {
        showConfirm: false,
      },
      () => {
        this.mainButtonRef.current?.focus();
      }
    );
    if (this.props.onCancel) {
      this.props.onCancel();
    }
  };
  onConfirm = (event: React.MouseEvent<HTMLButtonElement>) => {
    if (event) {
      event.preventDefault();
    }
    this.props.onConfirm();
    if (this.props.closeOnConfirm) {
      this.setState({
        showConfirm: false,
      });
    }
  };

  render() {
    const {
      theme,
      className,
      size,
      disabled,
      confirmText,
      confirmVariant: confirmButtonVariant,
      children,
    } = this.props;
    const styles = getStyles(theme);
    const buttonClass = cx(
      className,
      this.state.showConfirm ? styles.buttonHide : styles.buttonShow,
      disabled && styles.buttonDisabled
    );
    const confirmButtonClass = cx(
      styles.confirmButton,
      this.state.showConfirm ? styles.confirmButtonShow : styles.confirmButtonHide
    );

    const onClick = disabled ? () => {} : this.onClickButton;

    return (
      <span className={styles.buttonContainer}>
        <div className={cx(disabled && styles.disabled)}>
          <span className={buttonClass}>
            {typeof children === 'string' ? (
              <Button size={size} fill="text" onClick={onClick} ref={this.mainButtonRef}>
                {children}
              </Button>
            ) : (
              React.cloneElement(children, { onClick, ref: this.mainButtonRef })
            )}
          </span>
        </div>
        <span className={confirmButtonClass}>
          <Button size={size} variant={confirmButtonVariant} onClick={this.onConfirm} ref={this.confirmButtonRef}>
            {confirmText}
          </Button>
          <Button size={size} fill="text" onClick={this.onClickCancel}>
            Cancel
          </Button>
        </span>
      </span>
    );
  }
}

export const ConfirmButton = withTheme2(UnThemedConfirmButton);

const getStyles = stylesFactory((theme: GrafanaTheme2) => {
  return {
    buttonContainer: css({
      display: 'flex',
      alignItems: 'center',
      justifyContent: 'flex-end',
    }),
    buttonDisabled: css({
      textDecoration: 'none',
      color: theme.colors.text.primary,
      opacity: 0.65,
      pointerEvents: 'none',
    }),
    buttonShow: css({
      opacity: 1,
      transition: 'opacity 0.1s ease',
      zIndex: 2,
    }),
    buttonHide: css({
      opacity: 0,
      transition: 'opacity 0.1s ease, visibility 0 0.1s',
      visibility: 'hidden',
      zIndex: 0,
    }),
    confirmButton: css({
      alignItems: 'flex-start',
      background: theme.colors.background.primary,
      display: 'flex',
      position: 'absolute',
      pointerEvents: 'none',
    }),
    confirmButtonShow: css({
      zIndex: 1,
      opacity: 1,
      transition: 'opacity 0.08s ease-out, transform 0.1s ease-out',
      transform: 'translateX(0)',
      pointerEvents: 'all',
    }),
    confirmButtonHide: css({
      opacity: 0,
      visibility: 'hidden',
      transition: 'opacity 0.12s ease-in, transform 0.14s ease-in, visibility 0s 0.12s',
      transform: 'translateX(100px)',
    }),
    disabled: css({
      cursor: 'not-allowed',
    }),
  };
});

// Declare defaultProps directly on the themed component so they are displayed
// in the props table
ConfirmButton.defaultProps = {
  size: 'md',
  confirmText: 'Save',
  disabled: false,
  confirmVariant: 'primary',
};
ConfirmButton.displayName = 'ConfirmButton';
