import type {
  PluginExtension,
  PluginExtensionConfig,
  PluginExtensionLink,
  PluginExtensionLinkConfig,
} from '@grafana/data';
import { isPluginExtensionLink } from '@grafana/runtime';

import { isPluginExtensionComponentConfig, isPluginExtensionLinkConfig, logWarning } from './utils';

export function assertPluginExtensionLink(
  extension: PluginExtension | undefined,
  errorMessage = 'extension is not a link extension'
): asserts extension is PluginExtensionLink {
  if (!isPluginExtensionLink(extension)) {
    throw new Error(errorMessage);
  }
}

export function assertPluginExtensionLinkConfig(
  extension: PluginExtensionLinkConfig,
  errorMessage = 'extension is not a command extension config'
): asserts extension is PluginExtensionLinkConfig {
  if (!isPluginExtensionLinkConfig(extension)) {
    throw new Error(errorMessage);
  }
}

export function assertLinkPathIsValid(pluginId: string, path: string) {
  if (!isLinkPathValid(pluginId, path)) {
    throw new Error(
      `Invalid link extension. The "path" is required and should start with "/a/${pluginId}/" (currently: "${path}"). Skipping the extension.`
    );
  }
}

export function assertIsReactComponent(component: React.ComponentType) {
  if (!isReactComponent(component)) {
    throw new Error(`Invalid component extension, the "component" property needs to be a valid React component.`);
  }
}

export function assertExtensionPointIdIsValid(extension: PluginExtensionConfig) {
  if (!isExtensionPointIdValid(extension)) {
    throw new Error(
      `Invalid extension "${extension.title}". The extensionPointId should start with either "grafana/" or "plugins/" (currently: "${extension.extensionPointId}"). Skipping the extension.`
    );
  }
}

export function assertConfigureIsValid(extension: PluginExtensionLinkConfig) {
  if (!isConfigureFnValid(extension)) {
    throw new Error(
      `Invalid extension "${extension.title}". The "configure" property must be a function. Skipping the extension.`
    );
  }
}

export function assertStringProps(extension: Record<string, unknown>, props: string[]) {
  for (const prop of props) {
    if (!isStringPropValid(extension[prop])) {
      throw new Error(
        `Invalid extension "${extension.title}". Property "${prop}" must be a string and cannot be empty. Skipping the extension.`
      );
    }
  }
}

export function assertIsNotPromise(value: unknown, errorMessage = 'The provided value is a Promise.'): void {
  if (isPromise(value)) {
    throw new Error(errorMessage);
  }
}

export function isLinkPathValid(pluginId: string, path: string) {
  return Boolean(typeof path === 'string' && path.length > 0 && path.startsWith(`/a/${pluginId}/`));
}

export function isExtensionPointIdValid(extension: PluginExtensionConfig) {
  return Boolean(
    extension.extensionPointId?.startsWith('grafana/') || extension.extensionPointId?.startsWith('plugins/')
  );
}

export function isConfigureFnValid(extension: PluginExtensionLinkConfig) {
  return extension.configure ? typeof extension.configure === 'function' : true;
}

export function isStringPropValid(prop: unknown) {
  return typeof prop === 'string' && prop.length > 0;
}

export function isPluginExtensionConfigValid(pluginId: string, extension: PluginExtensionConfig): boolean {
  try {
    assertStringProps(extension, ['title', 'description', 'extensionPointId']);
    assertExtensionPointIdIsValid(extension);

    if (isPluginExtensionLinkConfig(extension)) {
      assertConfigureIsValid(extension);

      if (!extension.path && !extension.onClick) {
        logWarning(`Invalid extension "${extension.title}". Either "path" or "onClick" is required.`);
        return false;
      }

      if (extension.path) {
        assertLinkPathIsValid(pluginId, extension.path);
      }
    }

    if (isPluginExtensionComponentConfig(extension)) {
      assertIsReactComponent(extension.component);
    }

    return true;
  } catch (error) {
    if (error instanceof Error) {
      logWarning(error.message);
    }

    return false;
  }
}

export function isPromise(value: unknown): value is Promise<unknown> {
  return (
    value instanceof Promise || (typeof value === 'object' && value !== null && 'then' in value && 'catch' in value)
  );
}

export function isReactComponent(component: unknown): component is React.ComponentType {
  const hasReactTypeProp = (obj: unknown): obj is { $$typeof: Symbol } =>
    typeof obj === 'object' && obj !== null && '$$typeof' in obj;

  // The sandbox wraps the plugin components with React.memo.
  const isReactMemoObject = (obj: unknown): boolean =>
    hasReactTypeProp(obj) && obj.$$typeof === Symbol.for('react.memo');

  // We currently don't have any strict runtime-checking for this.
  // (The main reason is that we don't want to start depending on React implementation details.)
  return typeof component === 'function' || isReactMemoObject(component);
}
