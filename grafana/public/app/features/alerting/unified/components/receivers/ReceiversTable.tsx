import pluralize from 'pluralize';
import React, { useMemo, useState } from 'react';
import { useToggle } from 'react-use';

import { dateTime, dateTimeFormat } from '@grafana/data';
import { Stack } from '@grafana/experimental';
import { Badge, Button, ConfirmModal, Icon, Modal, useStyles2 } from '@grafana/ui';
import { AlertManagerCortexConfig } from 'app/plugins/datasource/alertmanager/types';
import { ContactPointsState, NotifiersState, ReceiversState, useDispatch } from 'app/types';

import { useGetContactPointsState } from '../../api/receiversApi';
import { Authorize } from '../../components/Authorize';
import { AlertmanagerAction, useAlertmanagerAbility } from '../../hooks/useAbilities';
import { useUnifiedAlertingSelector } from '../../hooks/useUnifiedAlertingSelector';
import { deleteReceiverAction } from '../../state/actions';
import { getAlertTableStyles } from '../../styles/table';
import { SupportedPlugin } from '../../types/pluginBridges';
import { isReceiverUsed } from '../../utils/alertmanager';
import { GRAFANA_RULES_SOURCE_NAME, isVanillaPrometheusAlertManagerDataSource } from '../../utils/datasource';
import { makeAMLink } from '../../utils/misc';
import { extractNotifierTypeCounts } from '../../utils/receivers';
import { DynamicTable, DynamicTableColumnProps, DynamicTableItemProps } from '../DynamicTable';
import { ProvisioningBadge } from '../Provisioning';
import { GrafanaReceiverExporter } from '../export/GrafanaReceiverExporter';
import { ActionIcon } from '../rules/ActionIcon';

import { ReceiversSection } from './ReceiversSection';
import { ReceiverMetadataBadge } from './grafanaAppReceivers/ReceiverMetadataBadge';
import { ReceiverMetadata, useReceiversMetadata } from './grafanaAppReceivers/useReceiversMetadata';
import { AlertmanagerConfigHealth, useAlertmanagerConfigHealth } from './useAlertmanagerConfigHealth';

interface UpdateActionProps extends ActionProps {
  onClickDeleteReceiver: (receiverName: string) => void;
}

function UpdateActions({ alertManagerName, receiverName, onClickDeleteReceiver }: UpdateActionProps) {
  return (
    <>
      <Authorize actions={[AlertmanagerAction.UpdateContactPoint]}>
        <ActionIcon
          aria-label="Edit"
          data-testid="edit"
          to={makeAMLink(
            `/alerting/notifications/receivers/${encodeURIComponent(receiverName)}/edit`,
            alertManagerName
          )}
          tooltip="Edit contact point"
          icon="pen"
        />
      </Authorize>
      <Authorize actions={[AlertmanagerAction.DeleteContactPoint]}>
        <ActionIcon
          onClick={() => onClickDeleteReceiver(receiverName)}
          tooltip="Delete contact point"
          icon="trash-alt"
        />
      </Authorize>
    </>
  );
}

interface ActionProps {
  alertManagerName: string;
  receiverName: string;
  canReadSecrets?: boolean;
}

function ViewAction({ alertManagerName, receiverName }: ActionProps) {
  return (
    <Authorize actions={[AlertmanagerAction.UpdateContactPoint]}>
      <ActionIcon
        data-testid="view"
        to={makeAMLink(`/alerting/notifications/receivers/${encodeURIComponent(receiverName)}/edit`, alertManagerName)}
        tooltip="View contact point"
        icon="file-alt"
      />
    </Authorize>
  );
}

function ExportAction({ receiverName, canReadSecrets = false }: ActionProps) {
  const [showExportDrawer, toggleShowExportDrawer] = useToggle(false);

  return (
    <Authorize actions={[AlertmanagerAction.ExportContactPoint]}>
      <ActionIcon
        data-testid="export"
        tooltip={
          canReadSecrets ? 'Export contact point with decrypted secrets' : 'Export contact point with redacted secrets'
        }
        icon="download-alt"
        onClick={toggleShowExportDrawer}
      />
      {showExportDrawer && (
        <GrafanaReceiverExporter
          receiverName={receiverName}
          decrypt={canReadSecrets}
          onClose={toggleShowExportDrawer}
        />
      )}
    </Authorize>
  );
}

interface ReceiverErrorProps {
  errorCount: number;
  errorDetail?: string;
  showErrorCount: boolean;
  tooltip?: string;
}

function ReceiverError({ errorCount, errorDetail, showErrorCount, tooltip }: ReceiverErrorProps) {
  const text = showErrorCount ? `${errorCount} ${pluralize('error', errorCount)}` : 'Error';
  const tooltipToRender = tooltip ?? errorDetail ?? 'Error';

  return <Badge color="red" icon="exclamation-circle" text={text} tooltip={tooltipToRender} />;
}

interface NotifierHealthProps {
  errorsByNotifier: number;
  errorDetail?: string;
  lastNotify: string;
}

function NotifierHealth({ errorsByNotifier, errorDetail, lastNotify }: NotifierHealthProps) {
  const hasErrors = errorsByNotifier > 0;
  const noAttempts = isLastNotifyNullDate(lastNotify);

  if (hasErrors) {
    return <ReceiverError errorCount={errorsByNotifier} errorDetail={errorDetail} showErrorCount={false} />;
  }

  if (noAttempts) {
    return <>No attempts</>;
  }

  return <Badge color="green" text="OK" />;
}

interface ReceiverHealthProps {
  errorsByReceiver: number;
  someWithNoAttempt: boolean;
}

function ReceiverHealth({ errorsByReceiver, someWithNoAttempt }: ReceiverHealthProps) {
  const hasErrors = errorsByReceiver > 0;

  if (hasErrors) {
    return (
      <ReceiverError
        errorCount={errorsByReceiver}
        showErrorCount={true}
        tooltip="Expand the contact point to see error details."
      />
    );
  }

  if (someWithNoAttempt) {
    return <>No attempts</>;
  }

  return <Badge color="green" text="OK" />;
}

const useContactPointsState = (alertManagerName: string) => {
  const contactPointsState = useGetContactPointsState(alertManagerName);
  const receivers: ReceiversState = contactPointsState?.receivers ?? {};
  const errorStateAvailable = Object.keys(receivers).length > 0;
  return { contactPointsState, errorStateAvailable };
};

interface ReceiverItem {
  name: string;
  types: string[];
  provisioned?: boolean;
  grafanaAppReceiverType?: SupportedPlugin;
  metadata?: ReceiverMetadata;
}

interface NotifierStatus {
  lastError?: null | string;
  lastNotify: string;
  lastNotifyDuration: string;
  type: string;
  sendResolved?: boolean;
}

type RowTableColumnProps = DynamicTableColumnProps<ReceiverItem>;
type RowItemTableProps = DynamicTableItemProps<ReceiverItem>;

type NotifierTableColumnProps = DynamicTableColumnProps<NotifierStatus>;
type NotifierItemTableProps = DynamicTableItemProps<NotifierStatus>;

interface NotifiersTableProps {
  notifiersState: NotifiersState;
}

const isLastNotifyNullDate = (lastNotify: string) => lastNotify === '0001-01-01T00:00:00.000Z';

function LastNotify({ lastNotifyDate }: { lastNotifyDate: string }) {
  if (isLastNotifyNullDate(lastNotifyDate)) {
    return <>{'-'}</>;
  } else {
    return (
      <Stack alignItems="center">
        <div>{`${dateTime(lastNotifyDate).locale('en').fromNow(true)} ago`}</div>
        <Icon name="clock-nine" />
        <div>{`${dateTimeFormat(lastNotifyDate, { format: 'YYYY-MM-DD HH:mm:ss' })}`}</div>
      </Stack>
    );
  }
}

const possibleNullDurations = ['', '0', '0ms', '0s', '0m', '0h', '0d', '0w', '0y'];
const durationIsNull = (duration: string) => possibleNullDurations.includes(duration);

function NotifiersTable({ notifiersState }: NotifiersTableProps) {
  function getNotifierColumns(): NotifierTableColumnProps[] {
    return [
      {
        id: 'health',
        label: 'Health',
        renderCell: ({ data: { lastError, lastNotify } }) => {
          return (
            <NotifierHealth
              errorsByNotifier={lastError ? 1 : 0}
              errorDetail={lastError ?? undefined}
              lastNotify={lastNotify}
            />
          );
        },
        size: 0.5,
      },
      {
        id: 'name',
        label: 'Name',
        renderCell: ({ data: { type }, id }) => <>{`${type}[${id}]`}</>,
        size: 1,
      },
      {
        id: 'lastNotify',
        label: 'Last delivery attempt',
        renderCell: ({ data: { lastNotify } }) => <LastNotify lastNotifyDate={lastNotify} />,
        size: 3,
      },
      {
        id: 'lastNotifyDuration',
        label: 'Last duration',
        renderCell: ({ data: { lastNotify, lastNotifyDuration } }) => (
          <>{isLastNotifyNullDate(lastNotify) && durationIsNull(lastNotifyDuration) ? '-' : lastNotifyDuration}</>
        ),
        size: 1,
      },
      {
        id: 'sendResolved',
        label: 'Send resolved',
        renderCell: ({ data: { sendResolved } }) => <>{String(Boolean(sendResolved))}</>,
        size: 1,
      },
    ];
  }

  const notifierRows: NotifierItemTableProps[] = Object.entries(notifiersState).flatMap((typeState) =>
    typeState[1].map((notifierStatus, index) => {
      return {
        id: index,
        data: {
          type: typeState[0],
          lastError: notifierStatus.lastNotifyAttemptError,
          lastNotify: notifierStatus.lastNotifyAttempt,
          lastNotifyDuration: notifierStatus.lastNotifyAttemptDuration,
          sendResolved: notifierStatus.sendResolved,
        },
      };
    })
  );

  return <DynamicTable items={notifierRows} cols={getNotifierColumns()} pagination={{ itemsPerPage: 25 }} />;
}

interface Props {
  config: AlertManagerCortexConfig;
  alertManagerName: string;
}

export const ReceiversTable = ({ config, alertManagerName }: Props) => {
  const dispatch = useDispatch();
  const isVanillaAM = isVanillaPrometheusAlertManagerDataSource(alertManagerName);
  const grafanaNotifiers = useUnifiedAlertingSelector((state) => state.grafanaNotifiers);

  const configHealth = useAlertmanagerConfigHealth(config.alertmanager_config);
  const { contactPointsState, errorStateAvailable } = useContactPointsState(alertManagerName);
  const receiversMetadata = useReceiversMetadata(config.alertmanager_config.receivers ?? []);

  // receiver name slated for deletion. If this is set, a confirmation modal is shown. If user approves, this receiver is deleted
  const [receiverToDelete, setReceiverToDelete] = useState<string>();
  const [showCannotDeleteReceiverModal, setShowCannotDeleteReceiverModal] = useState(false);

  const [supportsExport, allowedToExport] = useAlertmanagerAbility(AlertmanagerAction.ExportContactPoint);
  const showExport = supportsExport && allowedToExport;

  const onClickDeleteReceiver = (receiverName: string): void => {
    if (isReceiverUsed(receiverName, config)) {
      setShowCannotDeleteReceiverModal(true);
    } else {
      setReceiverToDelete(receiverName);
    }
  };

  const deleteReceiver = () => {
    if (receiverToDelete) {
      dispatch(deleteReceiverAction(receiverToDelete, alertManagerName));
    }
    setReceiverToDelete(undefined);
  };

  const rows: RowItemTableProps[] = useMemo(() => {
    const receivers = config.alertmanager_config.receivers ?? [];

    return (
      receivers.map((receiver) => ({
        id: receiver.name,
        data: {
          name: receiver.name,
          types: Object.entries(extractNotifierTypeCounts(receiver, grafanaNotifiers.result ?? [])).map(
            ([type, count]) => {
              if (count > 1) {
                return `${type} (${count})`;
              }
              return type;
            }
          ),
          provisioned: receiver.grafana_managed_receiver_configs?.some((receiver) => receiver.provenance),
          metadata: receiversMetadata.get(receiver),
        },
      })) ?? []
    );
  }, [grafanaNotifiers.result, config.alertmanager_config, receiversMetadata]);

  const [createSupported, createAllowed] = useAlertmanagerAbility(AlertmanagerAction.CreateContactPoint);

  const [_, canReadSecrets] = useAlertmanagerAbility(AlertmanagerAction.DecryptSecrets);

  const columns = useGetColumns(
    alertManagerName,
    errorStateAvailable,
    contactPointsState,
    configHealth,
    onClickDeleteReceiver,
    isVanillaAM,
    canReadSecrets
  );

  return (
    <ReceiversSection
      canReadSecrets={canReadSecrets}
      title="Contact points"
      description="Define where notifications are sent, for example, email or Slack."
      showButton={createSupported && createAllowed}
      addButtonLabel={'Add contact point'}
      addButtonTo={makeAMLink('/alerting/notifications/receivers/new', alertManagerName)}
      showExport={showExport}
    >
      <DynamicTable
        pagination={{ itemsPerPage: 25 }}
        items={rows}
        cols={columns}
        isExpandable={errorStateAvailable}
        renderExpandedContent={
          errorStateAvailable
            ? ({ data: { name } }) => (
                <NotifiersTable notifiersState={contactPointsState?.receivers[name]?.notifiers ?? {}} />
              )
            : undefined
        }
      />
      {!!showCannotDeleteReceiverModal && (
        <Modal
          isOpen={true}
          title="Cannot delete contact point"
          onDismiss={() => setShowCannotDeleteReceiverModal(false)}
        >
          <p>
            Contact point cannot be deleted because it is used in more policies. Please update or delete these policies
            first.
          </p>
          <Modal.ButtonRow>
            <Button variant="secondary" onClick={() => setShowCannotDeleteReceiverModal(false)} fill="outline">
              Close
            </Button>
          </Modal.ButtonRow>
        </Modal>
      )}
      {!!receiverToDelete && (
        <ConfirmModal
          isOpen={true}
          title="Delete contact point"
          body={`Are you sure you want to delete contact point "${receiverToDelete}"?`}
          confirmText="Yes, delete"
          onConfirm={deleteReceiver}
          onDismiss={() => setReceiverToDelete(undefined)}
        />
      )}
    </ReceiversSection>
  );
};
const errorsByReceiver = (contactPointsState: ContactPointsState, receiverName: string) =>
  contactPointsState?.receivers[receiverName]?.errorCount ?? 0;

const someNotifiersWithNoAttempt = (contactPointsState: ContactPointsState, receiverName: string) => {
  const notifiers = Object.values(contactPointsState?.receivers[receiverName]?.notifiers ?? {});

  if (notifiers.length === 0) {
    return false;
  }

  const hasSomeWitNoAttempt = notifiers.flat().some((status) => isLastNotifyNullDate(status.lastNotifyAttempt));
  return hasSomeWitNoAttempt;
};

function useGetColumns(
  alertManagerName: string,
  errorStateAvailable: boolean,
  contactPointsState: ContactPointsState | undefined,
  configHealth: AlertmanagerConfigHealth,
  onClickDeleteReceiver: (receiverName: string) => void,
  isVanillaAM: boolean,
  canReadSecrets: boolean
): RowTableColumnProps[] {
  const tableStyles = useStyles2(getAlertTableStyles);

  const enableHealthColumn =
    errorStateAvailable || Object.values(configHealth.contactPoints).some((cp) => cp.matchingRoutes === 0);

  const isGrafanaAlertManager = alertManagerName === GRAFANA_RULES_SOURCE_NAME;

  const baseColumns: RowTableColumnProps[] = [
    {
      id: 'name',
      label: 'Contact point name',
      renderCell: ({ data: { name, provisioned } }) => (
        <>
          <div>{name}</div>
          {provisioned && <ProvisioningBadge />}
        </>
      ),
      size: 3,
      className: tableStyles.nameCell,
    },
    {
      id: 'type',
      label: 'Type',
      renderCell: ({ data: { types, metadata } }) => (
        <>{metadata ? <ReceiverMetadataBadge metadata={metadata} /> : types.join(', ')}</>
      ),
      size: 2,
    },
  ];
  const healthColumn: RowTableColumnProps = {
    id: 'health',
    label: 'Health',
    renderCell: ({ data: { name } }) => {
      if (configHealth.contactPoints[name]?.matchingRoutes === 0) {
        return <UnusedContactPointBadge />;
      }

      return (
        contactPointsState &&
        Object.entries(contactPointsState.receivers).length > 0 && (
          <ReceiverHealth
            errorsByReceiver={errorsByReceiver(contactPointsState, name)}
            someWithNoAttempt={someNotifiersWithNoAttempt(contactPointsState, name)}
          />
        )
      );
    },
    size: '160px',
  };

  return [
    ...baseColumns,
    ...(enableHealthColumn ? [healthColumn] : []),
    {
      id: 'actions',
      label: 'Actions',
      renderCell: ({ data: { provisioned, name } }) => (
        <Authorize
          actions={[
            AlertmanagerAction.UpdateContactPoint,
            AlertmanagerAction.DeleteContactPoint,
            AlertmanagerAction.ExportContactPoint,
          ]}
        >
          <div className={tableStyles.actionsCell}>
            {!isVanillaAM && !provisioned && (
              <UpdateActions
                alertManagerName={alertManagerName}
                receiverName={name}
                onClickDeleteReceiver={onClickDeleteReceiver}
              />
            )}
            {(isVanillaAM || provisioned) && <ViewAction alertManagerName={alertManagerName} receiverName={name} />}
            {isGrafanaAlertManager && (
              <ExportAction alertManagerName={alertManagerName} receiverName={name} canReadSecrets={canReadSecrets} />
            )}
          </div>
        </Authorize>
      ),
      size: '100px',
    },
  ];
}

export function UnusedContactPointBadge() {
  return (
    <Badge
      text="Unused"
      color="orange"
      icon="exclamation-triangle"
      tooltip="This contact point is not used in any notification policy and it will not receive any alerts"
    />
  );
}
