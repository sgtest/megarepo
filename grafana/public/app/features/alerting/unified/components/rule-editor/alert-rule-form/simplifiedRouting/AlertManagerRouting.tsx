import { css } from '@emotion/css';
import React, { useState } from 'react';

import { GrafanaTheme2 } from '@grafana/data';
import { Alert, CollapsableSection, Icon, Link, LoadingPlaceholder, Stack, Text, useStyles2 } from '@grafana/ui';
import { AlertManagerDataSource } from 'app/features/alerting/unified/utils/datasource';
import { createUrl } from 'app/features/alerting/unified/utils/url';

import { useContactPointsWithStatus } from '../../../contact-points/useContactPoints';
import { ContactPointWithMetadata } from '../../../contact-points/utils';

import { ContactPointDetails } from './contactPoint/ContactPointDetails';
import { ContactPointSelector } from './contactPoint/ContactPointSelector';
import { MuteTimingFields } from './route-settings/MuteTimingFields';
import { RoutingSettings } from './route-settings/RouteSettings';

interface AlertManagerManualRoutingProps {
  alertManager: AlertManagerDataSource;
}

export function AlertManagerManualRouting({ alertManager }: AlertManagerManualRoutingProps) {
  const styles = useStyles2(getStyles);

  const alertManagerName = alertManager.name;
  const { isLoading, error: errorInContactPointStatus, contactPoints } = useContactPointsWithStatus();
  const shouldShowAM = true;
  const [selectedContactPointWithMetadata, setSelectedContactPointWithMetadata] = useState<
    ContactPointWithMetadata | undefined
  >();

  if (errorInContactPointStatus) {
    return <Alert title="Failed to fetch contact points" severity="error" />;
  }
  if (isLoading) {
    return <LoadingPlaceholder text={'Loading...'} />;
  }
  return (
    <Stack direction="column">
      {shouldShowAM && (
        <Stack direction="row" alignItems="center">
          <div className={styles.firstAlertManagerLine}></div>
          <div className={styles.alertManagerName}>
            Alert manager:
            <img src={alertManager.imgUrl} alt="Alert manager logo" className={styles.img} />
            {alertManagerName}
          </div>
          <div className={styles.secondAlertManagerLine}></div>
        </Stack>
      )}
      <Stack direction="row" gap={1} alignItems="center">
        <ContactPointSelector
          alertManager={alertManagerName}
          contactPoints={contactPoints}
          onSelectContactPoint={setSelectedContactPointWithMetadata}
        />
        <LinkToContactPoints />
      </Stack>
      {selectedContactPointWithMetadata?.grafana_managed_receiver_configs && (
        <ContactPointDetails receivers={selectedContactPointWithMetadata.grafana_managed_receiver_configs} />
      )}
      <div className={styles.routingSection}>
        <CollapsableSection label="Muting, grouping and timings" isOpen={false} className={styles.collapsableSection}>
          <Stack direction="column" gap={1}>
            <MuteTimingFields alertManager={alertManagerName} />
            <RoutingSettings alertManager={alertManagerName} />
          </Stack>
        </CollapsableSection>
      </div>
    </Stack>
  );
}
function LinkToContactPoints() {
  const hrefToContactPoints = '/alerting/notifications';
  return (
    <Link target="_blank" href={createUrl(hrefToContactPoints)} rel="noopener" aria-label="View alert rule">
      <Stack direction="row" gap={1} alignItems="center" justifyContent="center">
        <Text color="secondary">To browse contact points and create new ones go to</Text>
        <Text color="link">Contact points</Text>
        <Icon name={'external-link-alt'} size="sm" color="link" />
      </Stack>
    </Link>
  );
}

const getStyles = (theme: GrafanaTheme2) => ({
  firstAlertManagerLine: css({
    height: 1,
    width: theme.spacing(4),
    backgroundColor: theme.colors.secondary.main,
  }),
  alertManagerName: css({
    with: 'fit-content',
  }),
  secondAlertManagerLine: css({
    height: '1px',
    width: '100%',
    flex: 1,
    backgroundColor: theme.colors.secondary.main,
  }),
  img: css({
    marginLeft: theme.spacing(2),
    width: theme.spacing(3),
    height: theme.spacing(3),
    marginRight: theme.spacing(1),
  }),
  collapsableSection: css({
    width: 'fit-content',
    fontSize: theme.typography.body.fontSize,
  }),
  routingSection: css({
    display: 'flex',
    flexDirection: 'column',
    maxWidth: theme.breakpoints.values.xl,
    border: `solid 1px ${theme.colors.border.weak}`,
    borderRadius: theme.shape.radius.default,
    padding: `${theme.spacing(1)} ${theme.spacing(2)}`,
    marginTop: theme.spacing(2),
  }),
});
