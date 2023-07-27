import { capitalize, isEmpty, times } from 'lodash';

import { receiverTypeNames } from 'app/plugins/datasource/alertmanager/consts';
import { GrafanaManagedReceiverConfig, Receiver } from 'app/plugins/datasource/alertmanager/types';
import { NotifierDTO } from 'app/types';

// extract notifier type name to count map, eg { Slack: 1, Email: 2 }

type NotifierTypeCounts = Record<string, number>; // name : count

export function extractNotifierTypeCounts(receiver: Receiver, grafanaNotifiers: NotifierDTO[]): NotifierTypeCounts {
  if ('grafana_managed_receiver_configs' in receiver) {
    return getGrafanaNotifierTypeCounts(receiver.grafana_managed_receiver_configs ?? [], grafanaNotifiers);
  }
  return getCortexAlertManagerNotifierTypeCounts(receiver);
}

function getCortexAlertManagerNotifierTypeCounts(receiver: Receiver): NotifierTypeCounts {
  return Object.entries(receiver)
    .filter(([key]) => key !== 'grafana_managed_receiver_configs' && key.endsWith('_configs')) // filter out only properties that are alertmanager notifier
    .filter(([_, value]) => Array.isArray(value) && !!value.length) // check that there are actually notifiers of this type configured
    .reduce<NotifierTypeCounts>((acc, [key, value]) => {
      const type = key.replace('_configs', ''); // remove the `_config` part from the key, making it intto a notifier name
      const name = receiverTypeNames[type] ?? capitalize(type);
      return {
        ...acc,
        [name]: (acc[name] ?? 0) + (Array.isArray(value) ? value.length : 1),
      };
    }, {});
}

/**
 * This function will extract the integrations that have been defined for either grafana managed contact point
 * or vanilla Alertmanager receiver.
 *
 * It will attempt to normalize the data structure to how they have been defined for Grafana managed contact points.
 * That way we can work with the same data structure in the UI.
 *
 * We don't normalize the configuration settings and those are blank for vanilla Alertmanager receivers.
 *
 * Example input:
 *  { name: 'my receiver', email_configs: [{ from: "foo@bar.com" }] }
 *
 * Example output:
 *  { name: 'my receiver', grafana_managed_receiver_configs: [{ type: 'email', settings: {} }] }
 */
export function extractReceivers(receiver: Receiver): GrafanaManagedReceiverConfig[] {
  if ('grafana_managed_receiver_configs' in receiver) {
    return receiver.grafana_managed_receiver_configs ?? [];
  }

  const integrations = Object.entries(receiver)
    .filter(([key]) => key !== 'grafana_managed_receiver_configs' && key.endsWith('_configs'))
    .filter(([_, value]) => Array.isArray(value) && !isEmpty(value))
    .reduce((acc: GrafanaManagedReceiverConfig[], [key, value]) => {
      const type = key.replace('_configs', '');

      const configs = times(value.length, () => ({
        name: receiver.name,
        type: type,
        settings: [], // we don't normalize the configuration values
        disableResolveMessage: false,
      }));

      return acc.concat(configs);
    }, []);

  return integrations;
}

function getGrafanaNotifierTypeCounts(
  configs: GrafanaManagedReceiverConfig[],
  grafanaNotifiers: NotifierDTO[]
): NotifierTypeCounts {
  return configs
    .map((recv) => recv.type) // extract types from config
    .map((type) => grafanaNotifiers.find((r) => r.type === type)?.name ?? capitalize(type)) // get readable name from notifier cofnig, or if not available, just capitalize
    .reduce<NotifierTypeCounts>(
      (acc, type) => ({
        ...acc,
        [type]: (acc[type] ?? 0) + 1,
      }),
      {}
    );
}
