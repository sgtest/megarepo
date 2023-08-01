import { css } from '@emotion/css';
import React, { lazy, Suspense, useCallback, useMemo, useState } from 'react';

import { GrafanaTheme2 } from '@grafana/data';
import { Modal, useStyles2 } from '@grafana/ui';
import { RulerGrafanaRuleDTO } from 'app/types/unified-alerting-dto';

import { getHistoryImplementation } from '../components/rules/state-history/common';

const AnnotationsStateHistory = lazy(() => import('../components/rules/state-history/StateHistory'));
const LokiStateHistory = lazy(() => import('../components/rules/state-history/LokiStateHistory'));

export enum StateHistoryImplementation {
  Loki = 'loki',
  Annotations = 'annotations',
}

function useStateHistoryModal() {
  const [showModal, setShowModal] = useState<boolean>(false);
  const [rule, setRule] = useState<RulerGrafanaRuleDTO | undefined>();

  const styles = useStyles2(getStyles);

  const implementation = getHistoryImplementation();

  const dismissModal = useCallback(() => {
    setRule(undefined);
    setShowModal(false);
  }, []);

  const openModal = useCallback((rule: RulerGrafanaRuleDTO) => {
    setRule(rule);
    setShowModal(true);
  }, []);

  const StateHistoryModal = useMemo(() => {
    if (!rule) {
      return null;
    }

    return (
      <Modal
        isOpen={showModal}
        onDismiss={dismissModal}
        closeOnBackdropClick={true}
        closeOnEscape={true}
        title="State history"
        className={styles.modal}
        contentClassName={styles.modalContent}
      >
        <Suspense fallback={'Loading...'}>
          {implementation === StateHistoryImplementation.Loki && <LokiStateHistory ruleUID={rule.grafana_alert.uid} />}
          {implementation === StateHistoryImplementation.Annotations && (
            <AnnotationsStateHistory alertId={rule.grafana_alert.id ?? ''} />
          )}
        </Suspense>
      </Modal>
    );
  }, [rule, showModal, dismissModal, implementation, styles]);

  return {
    StateHistoryModal,
    showStateHistoryModal: openModal,
    hideStateHistoryModal: dismissModal,
  };
}

const getStyles = (theme: GrafanaTheme2) => ({
  modal: css`
    width: 80%;
    height: 80%;
    min-width: 800px;
  `,
  modalContent: css`
    height: 100%;
    width: 100%;
    padding: ${theme.spacing(2)};
  `,
});

export { useStateHistoryModal };
