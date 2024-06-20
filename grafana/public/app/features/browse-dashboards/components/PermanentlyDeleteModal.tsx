import React from 'react';

import { ConfirmModal, Text } from '@grafana/ui';

import { Trans, t } from '../../../core/internationalization';

import { Props as ModalProps } from './RestoreModal';

export const PermanentlyDeleteModal = ({
  onConfirm,
  onDismiss,
  selectedDashboards,
  isLoading,
  ...props
}: ModalProps) => {
  const numberOfDashboards = selectedDashboards.length;

  const onDelete = async () => {
    await onConfirm();
    onDismiss();
  };
  return (
    <ConfirmModal
      body={
        <Text element="p">
          <Trans i18nKey="recently-deleted.permanently-delete-modal.text" count={numberOfDashboards}>
            This action will delete {{ numberOfDashboards }} dashboards.
          </Trans>
        </Text>
      }
      title={t('recently-deleted.permanently-delete-modal.title', 'Permanently Delete Dashboards')}
      confirmationText={t('recently-deleted.permanently-delete-modal.confirm-text', 'Delete')}
      confirmText={
        isLoading
          ? t('recently-deleted.permanently-delete-modal.delete-loading', 'Deleting...')
          : t('recently-deleted.permanently-delete-modal.delete-button', 'Delete')
      }
      confirmButtonVariant="destructive"
      onConfirm={onDelete}
      onDismiss={onDismiss}
      {...props}
    />
  );
};
