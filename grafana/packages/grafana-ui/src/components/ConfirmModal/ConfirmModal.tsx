import { css, cx } from '@emotion/css';
import React, { useEffect, useRef, useState } from 'react';
import { useForm } from 'react-hook-form';

import { GrafanaTheme2 } from '@grafana/data';
import { selectors } from '@grafana/e2e-selectors';

import { useStyles2 } from '../../themes';
import { IconName } from '../../types/icon';
import { Button, ButtonVariant } from '../Button';
import { Input } from '../Input/Input';
import { Box } from '../Layout/Box/Box';
import { Stack } from '../Layout/Stack/Stack';
import { Modal } from '../Modal/Modal';

export interface ConfirmModalProps {
  /** Toggle modal's open/closed state */
  isOpen: boolean;
  /** Title for the modal header */
  title: string;
  /** Modal content */
  body: React.ReactNode;
  /** Modal description */
  description?: React.ReactNode;
  /** Text for confirm button */
  confirmText: string;
  /** Variant for confirm button */
  confirmVariant?: ButtonVariant;
  /** Text for dismiss button */
  dismissText?: string;
  /** Variant for dismiss button */
  dismissVariant?: ButtonVariant;
  /** Icon for the modal header */
  icon?: IconName;
  /** Additional styling for modal container */
  modalClass?: string;
  /** Text user needs to fill in before confirming */
  confirmationText?: string;
  /** Text for alternative button */
  alternativeText?: string;
  /** Confirm button variant */
  confirmButtonVariant?: ButtonVariant;
  /** Confirm action callback
   * Return a promise to disable the confirm button until the promise is resolved
   */
  onConfirm(): void | Promise<void>;
  /** Dismiss action callback */
  onDismiss(): void;
  /** Alternative action callback */
  onAlternative?(): void;
}

export const ConfirmModal = ({
  isOpen,
  title,
  body,
  description,
  confirmText,
  confirmVariant = 'destructive',
  confirmationText,
  dismissText = 'Cancel',
  dismissVariant = 'secondary',
  alternativeText,
  modalClass,
  icon = 'exclamation-triangle',
  onConfirm,
  onDismiss,
  onAlternative,
  confirmButtonVariant = 'destructive',
}: ConfirmModalProps): JSX.Element => {
  const [disabled, setDisabled] = useState(Boolean(confirmationText));
  const styles = useStyles2(getStyles);
  const buttonRef = useRef<HTMLButtonElement>(null);
  const onConfirmationTextChange = (event: React.FormEvent<HTMLInputElement>) => {
    setDisabled(confirmationText?.toLowerCase().localeCompare(event.currentTarget.value.toLowerCase()) !== 0);
  };

  useEffect(() => {
    // for some reason autoFocus property did no work on this button, but this does
    if (isOpen) {
      buttonRef.current?.focus();
    }
  }, [isOpen]);

  useEffect(() => {
    if (isOpen) {
      setDisabled(Boolean(confirmationText));
    }
  }, [isOpen, confirmationText]);

  const onConfirmClick = async () => {
    setDisabled(true);
    try {
      await onConfirm();
    } finally {
      setDisabled(false);
    }
  };

  const { handleSubmit } = useForm();

  return (
    <Modal className={cx(styles.modal, modalClass)} title={title} icon={icon} isOpen={isOpen} onDismiss={onDismiss}>
      <form onSubmit={handleSubmit(onConfirmClick)}>
        <div className={styles.modalText}>
          {body}
          {description ? <div className={styles.modalDescription}>{description}</div> : null}
          {confirmationText ? (
            <div className={styles.modalConfirmationInput}>
              <Stack alignItems="flex-start">
                <Box>
                  <Input placeholder={`Type "${confirmationText}" to confirm`} onChange={onConfirmationTextChange} />
                </Box>
              </Stack>
            </div>
          ) : null}
        </div>
        <Modal.ButtonRow>
          <Button variant={dismissVariant} onClick={onDismiss} fill="outline">
            {dismissText}
          </Button>
          <Button
            type="submit"
            variant={confirmButtonVariant}
            disabled={disabled}
            ref={buttonRef}
            data-testid={selectors.pages.ConfirmModal.delete}
          >
            {confirmText}
          </Button>
          {onAlternative ? (
            <Button variant="primary" onClick={onAlternative}>
              {alternativeText}
            </Button>
          ) : null}
        </Modal.ButtonRow>
      </form>
    </Modal>
  );
};

const getStyles = (theme: GrafanaTheme2) => ({
  modal: css({
    width: '500px',
  }),
  modalText: css({
    fontSize: theme.typography.h5.fontSize,
    color: theme.colors.text.primary,
  }),
  modalDescription: css({
    fontSize: theme.typography.body.fontSize,
  }),
  modalConfirmationInput: css({
    paddingTop: theme.spacing(1),
  }),
});
