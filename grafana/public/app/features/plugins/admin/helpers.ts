import { PluginSignatureStatus, dateTimeParse, PluginError, PluginType, PluginErrorCode } from '@grafana/data';
import { config, featureEnabled } from '@grafana/runtime';
import { Settings } from 'app/core/config';
import { contextSrv } from 'app/core/core';
import { getBackendSrv } from 'app/core/services/backend_srv';
import { AccessControlAction } from 'app/types';

import { CatalogPlugin, LocalPlugin, RemotePlugin, RemotePluginStatus, Version } from './types';

export function mergeLocalsAndRemotes(
  local: LocalPlugin[] = [],
  remote: RemotePlugin[] = [],
  errors?: PluginError[]
): CatalogPlugin[] {
  const catalogPlugins: CatalogPlugin[] = [];
  const errorByPluginId = groupErrorsByPluginId(errors);

  // add locals
  local.forEach((localPlugin) => {
    const remoteCounterpart = remote.find((r) => r.slug === localPlugin.id);
    const error = errorByPluginId[localPlugin.id];

    if (!remoteCounterpart) {
      catalogPlugins.push(mergeLocalAndRemote(localPlugin, undefined, error));
    }
  });

  // add remote
  remote.forEach((remotePlugin) => {
    const localCounterpart = local.find((l) => l.id === remotePlugin.slug);
    const error = errorByPluginId[remotePlugin.slug];
    const shouldSkip = remotePlugin.status === RemotePluginStatus.Deprecated && !localCounterpart; // We are only listing deprecated plugins in case they are installed.

    if (!shouldSkip) {
      catalogPlugins.push(mergeLocalAndRemote(localCounterpart, remotePlugin, error));
    }
  });

  return catalogPlugins;
}

export function mergeLocalAndRemote(local?: LocalPlugin, remote?: RemotePlugin, error?: PluginError): CatalogPlugin {
  if (!local && remote) {
    return mapRemoteToCatalog(remote, error);
  }

  if (local && !remote) {
    return mapLocalToCatalog(local, error);
  }

  return mapToCatalogPlugin(local, remote, error);
}

export function mapRemoteToCatalog(plugin: RemotePlugin, error?: PluginError): CatalogPlugin {
  const {
    name,
    slug: id,
    description,
    version,
    orgName,
    popularity,
    downloads,
    typeCode,
    updatedAt,
    createdAt: publishedAt,
    status,
    angularDetected,
  } = plugin;

  const isDisabled = !!error || isDisabledSecretsPlugin(typeCode);
  return {
    description,
    downloads,
    id,
    info: {
      logos: {
        small: `https://grafana.com/api/plugins/${id}/versions/${version}/logos/small`,
        large: `https://grafana.com/api/plugins/${id}/versions/${version}/logos/large`,
      },
    },
    name,
    orgName,
    popularity,
    publishedAt,
    signature: getPluginSignature({ remote: plugin, error }),
    updatedAt,
    hasUpdate: false,
    isPublished: true,
    isInstalled: isDisabled,
    isDisabled: isDisabled,
    isDeprecated: status === RemotePluginStatus.Deprecated,
    isCore: plugin.internal,
    isDev: false,
    isEnterprise: status === RemotePluginStatus.Enterprise,
    type: typeCode,
    error: error?.errorCode,
    angularDetected,
  };
}

export function mapLocalToCatalog(plugin: LocalPlugin, error?: PluginError): CatalogPlugin {
  const {
    name,
    info: { description, version, logos, updated, author },
    id,
    dev,
    type,
    signature,
    signatureOrg,
    signatureType,
    hasUpdate,
    accessControl,
    angularDetected,
  } = plugin;

  const isDisabled = !!error || isDisabledSecretsPlugin(type);
  return {
    description,
    downloads: 0,
    id,
    info: { logos },
    name,
    orgName: author.name,
    popularity: 0,
    publishedAt: '',
    signature: getPluginSignature({ local: plugin, error }),
    signatureOrg,
    signatureType,
    updatedAt: updated,
    installedVersion: version,
    hasUpdate,
    isInstalled: true,
    isDisabled: isDisabled,
    isCore: signature === 'internal',
    isPublished: false,
    isDeprecated: false,
    isDev: Boolean(dev),
    isEnterprise: false,
    type,
    error: error?.errorCode,
    accessControl: accessControl,
    angularDetected,
  };
}

// TODO: change the signature by removing the optionals for local and remote.
export function mapToCatalogPlugin(local?: LocalPlugin, remote?: RemotePlugin, error?: PluginError): CatalogPlugin {
  const installedVersion = local?.info.version;
  const id = remote?.slug || local?.id || '';
  const type = local?.type || remote?.typeCode;
  const isDisabled = !!error || isDisabledSecretsPlugin(type);

  let logos = {
    small: `/public/img/icn-${type}.svg`,
    large: `/public/img/icn-${type}.svg`,
  };

  if (remote) {
    logos = {
      small: `https://grafana.com/api/plugins/${id}/versions/${remote.version}/logos/small`,
      large: `https://grafana.com/api/plugins/${id}/versions/${remote.version}/logos/large`,
    };
  } else if (local && local.info.logos) {
    logos = local.info.logos;
  }

  return {
    description: local?.info.description || remote?.description || '',
    downloads: remote?.downloads || 0,
    hasUpdate: local?.hasUpdate || false,
    id,
    info: {
      logos,
    },
    isCore: Boolean(remote?.internal || local?.signature === PluginSignatureStatus.internal),
    isDev: Boolean(local?.dev),
    isEnterprise: remote?.status === RemotePluginStatus.Enterprise,
    isInstalled: Boolean(local) || isDisabled,
    isDisabled: isDisabled,
    isDeprecated: remote?.status === RemotePluginStatus.Deprecated,
    isPublished: true,
    // TODO<check if we would like to keep preferring the remote version>
    name: remote?.name || local?.name || '',
    // TODO<check if we would like to keep preferring the remote version>
    orgName: remote?.orgName || local?.info.author.name || '',
    popularity: remote?.popularity || 0,
    publishedAt: remote?.createdAt || '',
    type,
    signature: getPluginSignature({ local, remote, error }),
    signatureOrg: local?.signatureOrg || remote?.versionSignedByOrgName,
    signatureType: local?.signatureType || remote?.versionSignatureType || remote?.signatureType || undefined,
    // TODO<check if we would like to keep preferring the remote version>
    updatedAt: remote?.updatedAt || local?.info.updated || '',
    installedVersion,
    error: error?.errorCode,
    // Only local plugins have access control metadata
    accessControl: local?.accessControl,
    angularDetected: local?.angularDetected || remote?.angularDetected,
  };
}

export const getExternalManageLink = (pluginId: string) => `${config.pluginCatalogURL}${pluginId}`;

export enum Sorters {
  nameAsc = 'nameAsc',
  nameDesc = 'nameDesc',
  updated = 'updated',
  published = 'published',
  downloads = 'downloads',
}

export const sortPlugins = (plugins: CatalogPlugin[], sortBy: Sorters) => {
  const sorters: { [name: string]: (a: CatalogPlugin, b: CatalogPlugin) => number } = {
    nameAsc: (a: CatalogPlugin, b: CatalogPlugin) => a.name.localeCompare(b.name),
    nameDesc: (a: CatalogPlugin, b: CatalogPlugin) => b.name.localeCompare(a.name),
    updated: (a: CatalogPlugin, b: CatalogPlugin) =>
      dateTimeParse(b.updatedAt).valueOf() - dateTimeParse(a.updatedAt).valueOf(),
    published: (a: CatalogPlugin, b: CatalogPlugin) =>
      dateTimeParse(b.publishedAt).valueOf() - dateTimeParse(a.publishedAt).valueOf(),
    downloads: (a: CatalogPlugin, b: CatalogPlugin) => b.downloads - a.downloads,
  };

  if (sorters[sortBy]) {
    return plugins.sort(sorters[sortBy]);
  }

  return plugins;
};

function groupErrorsByPluginId(errors: PluginError[] = []): Record<string, PluginError | undefined> {
  return errors.reduce<Record<string, PluginError | undefined>>((byId, error) => {
    byId[error.pluginId] = error;
    return byId;
  }, {});
}

function getPluginSignature(options: {
  local?: LocalPlugin;
  remote?: RemotePlugin;
  error?: PluginError;
}): PluginSignatureStatus {
  const { error, local, remote } = options;

  if (error) {
    switch (error.errorCode) {
      case PluginErrorCode.invalidSignature:
        return PluginSignatureStatus.invalid;
      case PluginErrorCode.missingSignature:
        return PluginSignatureStatus.missing;
      case PluginErrorCode.modifiedSignature:
        return PluginSignatureStatus.modified;
    }
  }

  if (local?.signature) {
    return local.signature;
  }

  if (remote?.signatureType && remote?.versionSignatureType) {
    return PluginSignatureStatus.valid;
  }

  return PluginSignatureStatus.missing;
}

// Updates the core Grafana config to have the correct list available panels
export const updatePanels = () =>
  getBackendSrv()
    .get('/api/frontend/settings')
    .then((settings: Settings) => {
      config.panels = settings.panels;
    });

export function getLatestCompatibleVersion(versions: Version[] | undefined): Version | undefined {
  if (!versions) {
    return;
  }
  const [latest] = versions.filter((v) => Boolean(v.isCompatible));

  return latest;
}

export const isInstallControlsEnabled = () => config.pluginAdminEnabled;

export const hasInstallControlWarning = (
  plugin: CatalogPlugin,
  isRemotePluginsAvailable: boolean,
  latestCompatibleVersion?: Version
) => {
  const isExternallyManaged = config.pluginAdminExternalManageEnabled;
  const hasPermission = contextSrv.hasPermission(AccessControlAction.PluginsInstall);
  const isCompatible = Boolean(latestCompatibleVersion);
  return (
    plugin.type === PluginType.renderer ||
    plugin.type === PluginType.secretsmanager ||
    (plugin.isEnterprise && !featureEnabled('enterprise.plugins')) ||
    plugin.isDev ||
    (!hasPermission && !isExternallyManaged) ||
    !plugin.isPublished ||
    !isCompatible ||
    !isRemotePluginsAvailable
  );
};

export const isLocalPluginVisibleByConfig = (p: LocalPlugin) => isNotHiddenByConfig(p.id);

export const isRemotePluginVisibleByConfig = (p: RemotePlugin) => isNotHiddenByConfig(p.slug);

function isNotHiddenByConfig(id: string) {
  const { pluginCatalogHiddenPlugins }: { pluginCatalogHiddenPlugins: string[] } = config;

  return !pluginCatalogHiddenPlugins.includes(id);
}

function isDisabledSecretsPlugin(type?: PluginType): boolean {
  return type === PluginType.secretsmanager && !config.secretsManagerPluginEnabled;
}

export function isLocalCorePlugin(local?: LocalPlugin): boolean {
  return Boolean(local?.signature === 'internal');
}
