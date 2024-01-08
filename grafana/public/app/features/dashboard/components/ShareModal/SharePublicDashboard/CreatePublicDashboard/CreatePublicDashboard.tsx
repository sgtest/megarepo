import { css } from '@emotion/css';
import React from 'react';
import { FormState, UseFormRegister } from 'react-hook-form';

import { GrafanaTheme2 } from '@grafana/data/src';
import { selectors as e2eSelectors } from '@grafana/e2e-selectors/src';
import { Button, Form, Spinner, useStyles2 } from '@grafana/ui/src';
import { Trans } from 'app/core/internationalization';
import { useCreatePublicDashboardMutation } from 'app/features/dashboard/api/publicDashboardApi';
import { DashboardModel } from 'app/features/dashboard/state';
import { DashboardScene } from 'app/features/dashboard-scene/scene/DashboardScene';
import { DashboardInteractions } from 'app/features/dashboard-scene/utils/interactions';

import { contextSrv } from '../../../../../../core/services/context_srv';
import { AccessControlAction, useSelector } from '../../../../../../types';
import { NoUpsertPermissionsAlert } from '../ModalAlerts/NoUpsertPermissionsAlert';
import { UnsupportedDataSourcesAlert } from '../ModalAlerts/UnsupportedDataSourcesAlert';
import { UnsupportedTemplateVariablesAlert } from '../ModalAlerts/UnsupportedTemplateVariablesAlert';
import { dashboardHasTemplateVariables } from '../SharePublicDashboardUtils';
import { useGetUnsupportedDataSources } from '../useGetUnsupportedDataSources';

import { AcknowledgeCheckboxes } from './AcknowledgeCheckboxes';

const selectors = e2eSelectors.pages.ShareDashboardModal.PublicDashboard;

export type SharePublicDashboardAcknowledgmentInputs = {
  publicAcknowledgment: boolean;
  dataSourcesAcknowledgment: boolean;
  usageAcknowledgment: boolean;
};

interface CreatePublicDashboarBaseProps {
  unsupportedDatasources?: string[];
  unsupportedTemplateVariables?: boolean;
  dashboard: DashboardModel | DashboardScene;
  hasError?: boolean;
}

export const CreatePublicDashboardBase = ({
  unsupportedDatasources = [],
  unsupportedTemplateVariables = false,
  dashboard,
  hasError = false,
}: CreatePublicDashboarBaseProps) => {
  const styles = useStyles2(getStyles);
  const hasWritePermissions = contextSrv.hasPermission(AccessControlAction.DashboardsPublicWrite);
  const [createPublicDashboard, { isLoading, isError }] = useCreatePublicDashboardMutation();
  const onCreate = () => {
    createPublicDashboard({ dashboard, payload: { isEnabled: true } });
    DashboardInteractions.generatePublicDashboardUrlClicked({});
  };

  const disableInputs = !hasWritePermissions || isLoading || isError || hasError;

  return (
    <div className={styles.container}>
      <div>
        <p className={styles.title}>
          <Trans i18nKey="public-dashboard.create-page.welcome-title">Welcome to public dashboards!</Trans>
        </p>
        <p className={styles.description}>
          <Trans i18nKey="public-dashboard.create-page.unsupported-features-desc">
            Currently, we don’t support template variables or frontend data sources
          </Trans>
        </p>
      </div>

      {!hasWritePermissions && <NoUpsertPermissionsAlert mode="create" />}

      {unsupportedTemplateVariables && <UnsupportedTemplateVariablesAlert />}

      {unsupportedDatasources.length > 0 && (
        <UnsupportedDataSourcesAlert unsupportedDataSources={unsupportedDatasources.join(', ')} />
      )}

      <Form onSubmit={onCreate} validateOn="onChange" maxWidth="none">
        {({
          register,
          formState: { isValid },
        }: {
          register: UseFormRegister<SharePublicDashboardAcknowledgmentInputs>;
          formState: FormState<SharePublicDashboardAcknowledgmentInputs>;
        }) => (
          <>
            <div className={styles.checkboxes}>
              <AcknowledgeCheckboxes disabled={disableInputs} register={register} />
            </div>
            <div className={styles.buttonContainer}>
              <Button type="submit" disabled={disableInputs || !isValid} data-testid={selectors.CreateButton}>
                <Trans i18nKey="public-dashboard.create-page.generate-public-url-button">Generate public URL</Trans>
                {isLoading && <Spinner className={styles.loadingSpinner} />}
              </Button>
            </div>
          </>
        )}
      </Form>
    </div>
  );
};

export function CreatePublicDashboard({ hasError }: { hasError?: boolean }) {
  const dashboardState = useSelector((store) => store.dashboard);
  const dashboard = dashboardState.getModel()!;
  const { unsupportedDataSources } = useGetUnsupportedDataSources(dashboard);
  const hasTemplateVariables = dashboardHasTemplateVariables(dashboard.getVariables());

  return (
    <CreatePublicDashboardBase
      dashboard={dashboard}
      unsupportedDatasources={unsupportedDataSources}
      unsupportedTemplateVariables={hasTemplateVariables}
      hasError={hasError}
    />
  );
}

const getStyles = (theme: GrafanaTheme2) => ({
  container: css`
    display: flex;
    flex-direction: column;
    gap: ${theme.spacing(4)};
  `,
  title: css`
    font-size: ${theme.typography.h4.fontSize};
    margin: ${theme.spacing(0, 0, 2)};
  `,
  description: css`
    color: ${theme.colors.text.secondary};
    margin-bottom: ${theme.spacing(0)};
  `,
  checkboxes: css`
    margin: ${theme.spacing(0, 0, 4)};
  `,
  buttonContainer: css`
    display: flex;
    justify-content: end;
  `,
  loadingSpinner: css`
    margin-left: ${theme.spacing(1)};
  `,
});
