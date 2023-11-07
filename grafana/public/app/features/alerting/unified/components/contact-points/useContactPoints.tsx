/**
 * This hook will combine data from both the Alertmanager config
 * and (if available) it will also fetch the status from the Grafana Managed status endpoint
 */

import { produce } from 'immer';
import { remove } from 'lodash';

import { alertmanagerApi } from '../../api/alertmanagerApi';
import { onCallApi, OnCallIntegrationDTO } from '../../api/onCallApi';
import { usePluginBridge } from '../../hooks/usePluginBridge';
import { useAlertmanager } from '../../state/AlertmanagerContext';
import { SupportedPlugin } from '../../types/pluginBridges';

import { enhanceContactPointsWithMetadata } from './utils';

export const RECEIVER_STATUS_KEY = Symbol('receiver_status');
export const RECEIVER_META_KEY = Symbol('receiver_metadata');
export const RECEIVER_PLUGIN_META_KEY = Symbol('receiver_plugin_metadata');

const RECEIVER_STATUS_POLLING_INTERVAL = 10 * 1000; // 10 seconds

/**
 * This hook will combine data from several endpoints;
 * 1. the alertmanager config endpoint where the definition of the receivers are
 * 2. (if available) the alertmanager receiver status endpoint, currently Grafana Managed only
 * 3. (if available) additional metadata about Grafana Managed contact points
 * 4. (if available) the OnCall plugin metadata
 */
export function useContactPointsWithStatus() {
  const { selectedAlertmanager, isGrafanaAlertmanager } = useAlertmanager();
  const { installed: onCallPluginInstalled, loading: onCallPluginStatusLoading } = usePluginBridge(
    SupportedPlugin.OnCall
  );

  // fetch receiver status if we're dealing with a Grafana Managed Alertmanager
  const fetchContactPointsStatus = alertmanagerApi.endpoints.getContactPointsStatus.useQuery(undefined, {
    refetchOnFocus: true,
    refetchOnReconnect: true,
    // re-fetch status every so often for up-to-date information
    pollingInterval: RECEIVER_STATUS_POLLING_INTERVAL,
    // skip fetching receiver statuses if not Grafana AM
    skip: !isGrafanaAlertmanager,
  });

  // fetch notifier metadata from the Grafana API if we're using a Grafana AM – this will be used to add additional
  // metadata and canonical names to the receiver
  const fetchReceiverMetadata = alertmanagerApi.endpoints.grafanaNotifiers.useQuery(undefined, {
    skip: !isGrafanaAlertmanager,
  });

  // if the OnCall plugin is installed, fetch its list of integrations so we can match those to the Grafana Managed contact points
  const { data: onCallIntegrations, isLoading: onCallPluginIntegrationsLoading } =
    onCallApi.endpoints.grafanaOnCallIntegrations.useQuery(undefined, {
      skip: !onCallPluginInstalled || !isGrafanaAlertmanager,
    });

  // null = no installed, undefined = loading, [n] is installed with integrations
  let onCallMetadata: null | undefined | OnCallIntegrationDTO[] = undefined;
  if (onCallPluginInstalled) {
    onCallMetadata = onCallIntegrations ?? [];
  } else if (onCallPluginInstalled === false) {
    onCallMetadata = null;
  }

  // fetch the latest config from the Alertmanager
  const fetchAlertmanagerConfiguration = alertmanagerApi.endpoints.getAlertmanagerConfiguration.useQuery(
    selectedAlertmanager!,
    {
      refetchOnFocus: true,
      refetchOnReconnect: true,
      selectFromResult: (result) => ({
        ...result,
        contactPoints: result.data
          ? enhanceContactPointsWithMetadata(
              result.data,
              fetchContactPointsStatus.data,
              fetchReceiverMetadata.data,
              onCallMetadata
            )
          : [],
      }),
    }
  );

  // we will fail silently for fetching OnCall plugin status and integrations
  const error = fetchAlertmanagerConfiguration.error ?? fetchContactPointsStatus.error;
  const isLoading =
    fetchAlertmanagerConfiguration.isLoading ||
    fetchContactPointsStatus.isLoading ||
    onCallPluginStatusLoading ||
    onCallPluginIntegrationsLoading;

  const contactPoints = fetchAlertmanagerConfiguration.contactPoints.sort((a, b) => a.name.localeCompare(b.name));

  return {
    error,
    isLoading,
    contactPoints,
  };
}

export function useDeleteContactPoint(selectedAlertmanager: string) {
  const [fetchAlertmanagerConfig] = alertmanagerApi.endpoints.getAlertmanagerConfiguration.useLazyQuery();
  const [updateAlertManager, updateAlertmanagerState] =
    alertmanagerApi.endpoints.updateAlertmanagerConfiguration.useMutation();

  const deleteTrigger = (contactPointName: string) => {
    return fetchAlertmanagerConfig(selectedAlertmanager).then(({ data }) => {
      if (!data) {
        return;
      }

      const newConfig = produce(data, (draft) => {
        remove(draft?.alertmanager_config?.receivers ?? [], (receiver) => receiver.name === contactPointName);
        return draft;
      });

      return updateAlertManager({
        selectedAlertmanager,
        config: newConfig,
      }).unwrap();
    });
  };

  return {
    deleteTrigger,
    updateAlertmanagerState,
  };
}
