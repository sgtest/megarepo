import { css, cx } from '@emotion/css';
import React, { HTMLProps, useCallback } from 'react';

import { GrafanaTheme2 } from '@grafana/data';

import { useTheme2 } from '../../themes';
import { getFocusStyles, getMouseFocusStyles } from '../../themes/mixins';

import { getLabelStyles } from './Label';

export interface CheckboxProps extends Omit<HTMLProps<HTMLInputElement>, 'value'> {
  /** Label to display next to checkbox */
  label?: string;
  /** Description to display under the label */
  description?: string;
  /** Current value of the checkbox */
  value?: boolean;
  /** htmlValue allows to specify the input "value" attribute */
  htmlValue?: string | number;
  /** Sets the checkbox into a "mixed" state. This is only a visual change and does not affect the value. */
  indeterminate?: boolean;
  /** Show an invalid state around the input */
  invalid?: boolean;
}

export const Checkbox = React.forwardRef<HTMLInputElement, CheckboxProps>(
  (
    { label, description, value, htmlValue, onChange, disabled, className, indeterminate, invalid, ...inputProps },
    ref
  ) => {
    const handleOnChange = useCallback(
      (e: React.ChangeEvent<HTMLInputElement>) => {
        if (onChange) {
          onChange(e);
        }
      },
      [onChange]
    );
    const theme = useTheme2();
    const styles = getCheckboxStyles(theme, invalid);

    const ariaChecked = indeterminate ? 'mixed' : undefined;

    return (
      <label className={cx(styles.wrapper, className)}>
        <div className={styles.checkboxWrapper}>
          <input
            type="checkbox"
            className={cx(styles.input, indeterminate && styles.inputIndeterminate)}
            checked={value}
            disabled={disabled}
            onChange={handleOnChange}
            value={htmlValue}
            aria-checked={ariaChecked}
            {...inputProps}
            ref={ref}
          />
          <span className={styles.checkmark} />
        </div>
        {label && <span className={styles.label}>{label}</span>}
        {description && <span className={styles.description}>{description}</span>}
      </label>
    );
  }
);

export const getCheckboxStyles = (theme: GrafanaTheme2, invalid = false) => {
  const labelStyles = getLabelStyles(theme);
  const checkboxSize = 2;
  const labelPadding = 1;

  const getBorderColor = (color: string) => {
    return invalid ? theme.colors.error.border : color;
  };

  return {
    wrapper: css`
      display: inline-grid;
      align-items: center;
      column-gap: ${theme.spacing(labelPadding)};
      position: relative;
      vertical-align: middle;
    `,
    input: css`
      position: absolute;
      z-index: 1;
      top: 0;
      left: 0;
      width: 100% !important; // global styles unset this
      height: 100%;
      opacity: 0;

      &:focus + span,
      &:focus-visible + span {
        ${getFocusStyles(theme)}
      }

      &:focus:not(:focus-visible) + span {
        ${getMouseFocusStyles(theme)}
      }

      /**
       * Using adjacent sibling selector to style checked state.
       * Primarily to limit the classes necessary to use when these classes will be used
       * for angular components styling
       * */
      &:checked + span {
        background: ${theme.colors.primary.main};
        border: 1px solid ${getBorderColor(theme.colors.primary.main)};

        &:hover {
          background: ${theme.colors.primary.shade};
        }

        &:after {
          content: '';
          position: absolute;
          z-index: 2;
          left: 4px;
          top: 0px;
          width: 6px;
          height: 12px;
          border: solid ${theme.colors.primary.contrastText};
          border-width: 0 3px 3px 0;
          transform: rotate(45deg);
        }
      }

      &:disabled + span {
        background-color: ${theme.colors.action.disabledBackground};
        cursor: not-allowed;
        border: 1px solid ${getBorderColor(theme.colors.action.disabledBackground)};

        &:hover {
          background-color: ${theme.colors.action.disabledBackground};
        }

        &:after {
          border-color: ${theme.colors.action.disabledText};
        }
      }
    `,

    inputIndeterminate: css`
      &[aria-checked='mixed'] + span {
        border: 1px solid ${getBorderColor(theme.colors.primary.main)};
        background: ${theme.colors.primary.main};

        &:hover {
          background: ${theme.colors.primary.shade};
        }

        &:after {
          content: '';
          position: absolute;
          z-index: 2;
          left: 2px;
          right: 2px;
          top: calc(50% - 1.5px);
          height: 3px;
          border: 1.5px solid ${theme.colors.primary.contrastText};
          background-color: ${theme.colors.primary.contrastText};
          width: auto;
          transform: none;
        }
      }
      &:disabled[aria-checked='mixed'] + span {
        background-color: ${theme.colors.action.disabledBackground};
        border: 1px solid ${getBorderColor(theme.colors.error.transparent)};

        &:after {
          border-color: ${theme.colors.action.disabledText};
        }
      }
    `,

    checkboxWrapper: css`
      display: flex;
      align-items: center;
      grid-column-start: 1;
      grid-row-start: 1;
    `,
    checkmark: css`
      position: relative; /* Checkbox should be layered on top of the invisible input so it recieves :hover */
      z-index: 2;
      display: inline-block;
      width: ${theme.spacing(checkboxSize)};
      height: ${theme.spacing(checkboxSize)};
      border-radius: ${theme.shape.borderRadius()};
      background: ${theme.components.input.background};
      border: 1px solid ${getBorderColor(theme.components.input.borderColor)};

      &:hover {
        cursor: pointer;
        border-color: ${getBorderColor(theme.components.input.borderHover)};
      }
    `,
    label: cx(
      labelStyles.label,
      css`
        grid-column-start: 2;
        grid-row-start: 1;
        position: relative;
        z-index: 2;
        cursor: pointer;
        max-width: fit-content;
        line-height: ${theme.typography.bodySmall.lineHeight};
        margin-bottom: 0;
      `
    ),
    description: cx(
      labelStyles.description,
      css`
        grid-column-start: 2;
        grid-row-start: 2;
        line-height: ${theme.typography.bodySmall.lineHeight};
        margin-top: 0; /* The margin effectively comes from the top: -2px on the label above it */
      `
    ),
  };
};

Checkbox.displayName = 'Checkbox';
