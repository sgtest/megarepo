/*---------------------------------------------------------------------------------------------
 *  Copyright (c) Microsoft Corporation. All rights reserved.
 *  Licensed under the MIT License. See License.txt in the project root for license information.
 *--------------------------------------------------------------------------------------------*/

import * as http from 'http';
import * as https from 'https';
import * as tls from 'tls';
import * as net from 'net';

import { IExtHostWorkspaceProvider } from 'vs/workbench/api/common/extHostWorkspace';
import { ExtHostConfigProvider } from 'vs/workbench/api/common/extHostConfiguration';
import { MainThreadTelemetryShape } from 'vs/workbench/api/common/extHost.protocol';
import { IExtensionHostInitData } from 'vs/workbench/services/extensions/common/extensionHostProtocol';
import { ExtHostExtensionService } from 'vs/workbench/api/node/extHostExtensionService';
import { URI } from 'vs/base/common/uri';
import { ILogService } from 'vs/platform/log/common/log';
import { IExtensionDescription } from 'vs/platform/extensions/common/extensions';
import { LogLevel, createHttpPatch, createProxyResolver, createTlsPatch, ProxySupportSetting, ProxyAgentParams, createNetPatch } from '@vscode/proxy-agent';

export function connectProxyResolver(
	extHostWorkspace: IExtHostWorkspaceProvider,
	configProvider: ExtHostConfigProvider,
	extensionService: ExtHostExtensionService,
	extHostLogService: ILogService,
	mainThreadTelemetry: MainThreadTelemetryShape,
	initData: IExtensionHostInitData,
) {
	const useHostProxy = initData.environment.useHostProxy;
	const doUseHostProxy = typeof useHostProxy === 'boolean' ? useHostProxy : !initData.remote.isRemote;
	const params: ProxyAgentParams = {
		resolveProxy: url => extHostWorkspace.resolveProxy(url),
		lookupProxyAuthorization: lookupProxyAuthorization.bind(undefined, extHostLogService, configProvider, {}, {}),
		getProxyURL: () => configProvider.getConfiguration('http').get('proxy'),
		getProxySupport: () => configProvider.getConfiguration('http').get<ProxySupportSetting>('proxySupport') || 'off',
		getSystemCertificatesV1: () => certSettingV1(configProvider),
		getSystemCertificatesV2: () => certSettingV2(configProvider),
		log: (level, message, ...args) => {
			switch (level) {
				case LogLevel.Trace: extHostLogService.trace(message, ...args); break;
				case LogLevel.Debug: extHostLogService.debug(message, ...args); break;
				case LogLevel.Info: extHostLogService.info(message, ...args); break;
				case LogLevel.Warning: extHostLogService.warn(message, ...args); break;
				case LogLevel.Error: extHostLogService.error(message, ...args); break;
				case LogLevel.Critical: extHostLogService.error(message, ...args); break;
				case LogLevel.Off: break;
				default: never(level, message, args); break;
			}
			function never(level: never, message: string, ...args: any[]) {
				extHostLogService.error('Unknown log level', level);
				extHostLogService.error(message, ...args);
			}
		},
		getLogLevel: () => extHostLogService.getLevel(),
		// TODO @chrmarti Remove this from proxy agent
		proxyResolveTelemetry: () => { },
		useHostProxy: doUseHostProxy,
		addCertificates: [],
		env: process.env,
	};
	const resolveProxy = createProxyResolver(params);
	const lookup = createPatchedModules(params, resolveProxy);
	return configureModuleLoading(extensionService, lookup);
}

function createPatchedModules(params: ProxyAgentParams, resolveProxy: ReturnType<typeof createProxyResolver>) {
	return {
		http: Object.assign(http, createHttpPatch(params, http, resolveProxy)),
		https: Object.assign(https, createHttpPatch(params, https, resolveProxy)),
		net: Object.assign(net, createNetPatch(params, net)),
		tls: Object.assign(tls, createTlsPatch(params, tls))
	};
}

function certSettingV1(configProvider: ExtHostConfigProvider) {
	const http = configProvider.getConfiguration('http');
	return !http.get<boolean>('experimental.systemCertificatesV2') && !!http.get<boolean>('systemCertificates');
}

function certSettingV2(configProvider: ExtHostConfigProvider) {
	const http = configProvider.getConfiguration('http');
	return !!http.get<boolean>('experimental.systemCertificatesV2') && !!http.get<boolean>('systemCertificates');
}

const modulesCache = new Map<IExtensionDescription | undefined, { http?: typeof http; https?: typeof https }>();
function configureModuleLoading(extensionService: ExtHostExtensionService, lookup: ReturnType<typeof createPatchedModules>): Promise<void> {
	return extensionService.getExtensionPathIndex()
		.then(extensionPaths => {
			const node_module = <any>globalThis._VSCODE_NODE_MODULES.module;
			const original = node_module._load;
			node_module._load = function load(request: string, parent: { filename: string }, isMain: boolean) {
				if (request === 'net') {
					return lookup.net;
				}

				if (request === 'tls') {
					return lookup.tls;
				}

				if (request !== 'http' && request !== 'https') {
					return original.apply(this, arguments);
				}

				const ext = extensionPaths.findSubstr(URI.file(parent.filename));
				let cache = modulesCache.get(ext);
				if (!cache) {
					modulesCache.set(ext, cache = {});
				}
				if (!cache[request]) {
					const mod = lookup[request];
					cache[request] = <any>{ ...mod }; // Copy to work around #93167.
				}
				return cache[request];
			};
		});
}

async function lookupProxyAuthorization(
	extHostLogService: ILogService,
	configProvider: ExtHostConfigProvider,
	proxyAuthenticateCache: Record<string, string | string[] | undefined>,
	pendingLookups: Record<string, Promise<string | undefined>>,
	proxyURL: string,
	proxyAuthenticate?: string | string[]
): Promise<string | undefined> {
	const cached = proxyAuthenticateCache[proxyURL];
	if (proxyAuthenticate) {
		proxyAuthenticateCache[proxyURL] = proxyAuthenticate;
	}
	extHostLogService.trace('ProxyResolver#lookupProxyAuthorization callback', `proxyURL:${proxyURL}`, `proxyAuthenticate:${proxyAuthenticate}`, `proxyAuthenticateCache:${cached}`);
	const header = proxyAuthenticate || cached;
	const authenticate = Array.isArray(header) ? header : typeof header === 'string' ? [header] : [];
	if (authenticate.some(a => /^(Negotiate|Kerberos)( |$)/i.test(a))) {
		const lookupKey = `${proxyURL}:Negotiate`;
		return pendingLookups[lookupKey] ??= (async () => {
			try {
				const kerberos = await import('kerberos');
				const url = new URL(proxyURL);
				const spn = configProvider.getConfiguration('http').get<string>('proxyKerberosServicePrincipal')
					|| (process.platform === 'win32' ? `HTTP/${url.hostname}` : `HTTP@${url.hostname}`);
				extHostLogService.debug('ProxyResolver#lookupProxyAuthorization Kerberos authentication lookup', `proxyURL:${proxyURL}`, `spn:${spn}`);
				const client = await kerberos.initializeClient(spn);
				const response = await client.step('');
				return 'Negotiate ' + response;
			} catch (err) {
				extHostLogService.error('ProxyResolver#lookupProxyAuthorization Kerberos authentication failed', err);
				return undefined;
			} finally {
				delete pendingLookups[lookupKey];
			}
		})();
	}
	return undefined;
}
