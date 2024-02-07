/*---------------------------------------------------------------------------------------------
 *  Copyright (c) Microsoft Corporation. All rights reserved.
 *  Licensed under the MIT License. See License.txt in the project root for license information.
 *--------------------------------------------------------------------------------------------*/

import { CancellationToken, CancellationTokenSource } from 'vs/base/common/cancellation';
import { IDisposable, toDisposable } from 'vs/base/common/lifecycle';
import { ILogService } from 'vs/platform/log/common/log';
import { ExtHostChatProviderShape, IMainContext, MainContext, MainThreadChatProviderShape } from 'vs/workbench/api/common/extHost.protocol';
import * as typeConvert from 'vs/workbench/api/common/extHostTypeConverters';
import type * as vscode from 'vscode';
import { Progress } from 'vs/platform/progress/common/progress';
import { IChatMessage, IChatResponseFragment } from 'vs/workbench/contrib/chat/common/chatProvider';
import { ExtensionIdentifier, ExtensionIdentifierMap, ExtensionIdentifierSet } from 'vs/platform/extensions/common/extensions';
import { AsyncIterableSource } from 'vs/base/common/async';
import { Emitter, Event } from 'vs/base/common/event';

type LanguageModelData = {
	readonly extension: ExtensionIdentifier;
	readonly provider: vscode.ChatResponseProvider;
};

class LanguageModelResponseStream {

	readonly apiObj: vscode.LanguageModelResponseStream;
	readonly stream = new AsyncIterableSource<string>();

	constructor(option: number, stream?: AsyncIterableSource<string>) {
		this.stream = stream ?? new AsyncIterableSource<string>();
		const that = this;
		this.apiObj = {
			option: option,
			response: that.stream.asyncIterable
		};
	}
}

class LanguageModelRequest {

	readonly apiObject: vscode.LanguageModelRequest;

	private readonly _onDidStart = new Emitter<vscode.LanguageModelResponseStream>();
	private readonly _responseStreams = new Map<number, LanguageModelResponseStream>();
	private readonly _defaultStream = new AsyncIterableSource<string>();
	private _isDone: boolean = false;

	constructor(
		promise: Promise<any>,
		readonly cts: CancellationTokenSource
	) {
		const that = this;
		this.apiObject = {
			result: promise,
			response: that._defaultStream.asyncIterable,
			onDidStartResponseStream: that._onDidStart.event,
			cancel() { cts.cancel(); },
		};

		promise.finally(() => {
			this._isDone = true;
			if (this._responseStreams.size > 0) {
				for (const [, value] of this._responseStreams) {
					value.stream.resolve();
				}
			} else {
				this._defaultStream.resolve();
			}
		});
	}

	handleFragment(fragment: IChatResponseFragment): void {
		if (this._isDone) {
			return;
		}
		let res = this._responseStreams.get(fragment.index);
		if (!res) {
			if (this._responseStreams.size === 0) {
				// the first response claims the default response
				res = new LanguageModelResponseStream(fragment.index, this._defaultStream);
			} else {
				res = new LanguageModelResponseStream(fragment.index);
			}
			this._responseStreams.set(fragment.index, res);
			this._onDidStart.fire(res.apiObj);
		}
		res.stream.emitOne(fragment.part);
	}

}

export class ExtHostChatProvider implements ExtHostChatProviderShape {

	private static _idPool = 1;

	private readonly _proxy: MainThreadChatProviderShape;
	private readonly _onDidChangeAccess = new Emitter<ExtensionIdentifierSet>();
	private readonly _onDidChangeProviders = new Emitter<vscode.LanguageModelChangeEvent>();
	readonly onDidChangeProviders = this._onDidChangeProviders.event;

	private readonly _languageModels = new Map<number, LanguageModelData>();
	private readonly _languageModelIds = new Set<string>(); // these are ALL models, not just the one in this EH
	private readonly _accessAllowlist = new ExtensionIdentifierMap<boolean>();
	private readonly _pendingRequest = new Map<number, { languageModelId: string; res: LanguageModelRequest }>();


	constructor(
		mainContext: IMainContext,
		private readonly _logService: ILogService,
	) {
		this._proxy = mainContext.getProxy(MainContext.MainThreadChatProvider);
	}

	dispose(): void {
		this._onDidChangeAccess.dispose();
		this._onDidChangeProviders.dispose();
	}

	registerLanguageModel(extension: ExtensionIdentifier, identifier: string, provider: vscode.ChatResponseProvider, metadata: vscode.ChatResponseProviderMetadata): IDisposable {

		const handle = ExtHostChatProvider._idPool++;
		this._languageModels.set(handle, { extension, provider });
		this._proxy.$registerProvider(handle, identifier, { extension, model: metadata.name ?? '' });

		return toDisposable(() => {
			this._languageModels.delete(handle);
			this._proxy.$unregisterProvider(handle);
		});
	}

	async $provideLanguageModelResponse(handle: number, requestId: number, messages: IChatMessage[], options: { [name: string]: any }, token: CancellationToken): Promise<any> {
		const data = this._languageModels.get(handle);
		if (!data) {
			return;
		}
		const progress = new Progress<vscode.ChatResponseFragment>(async fragment => {
			if (token.isCancellationRequested) {
				this._logService.warn(`[CHAT](${data.extension.value}) CANNOT send progress because the REQUEST IS CANCELLED`);
				return;
			}
			this._proxy.$handleProgressChunk(requestId, { index: fragment.index, part: fragment.part });
		});

		return data.provider.provideChatResponse(messages.map(typeConvert.ChatMessage.to), options, progress, token);
	}

	//#region --- making request




	$updateLanguageModels(data: { added?: string[] | undefined; removed?: string[] | undefined }): void {
		const added: string[] = [];
		const removed: string[] = [];
		if (data.added) {
			for (const id of data.added) {
				this._languageModelIds.add(id);
				added.push(id);
			}
		}
		if (data.removed) {
			for (const id of data.removed) {
				// clean up
				this._languageModelIds.delete(id);
				removed.push(id);

				// cancel pending requests for this model
				for (const [key, value] of this._pendingRequest) {
					if (value.languageModelId === id) {
						value.res.cts.cancel();
						this._pendingRequest.delete(key);
					}
				}
			}
		}

		this._onDidChangeProviders.fire(Object.freeze({
			added: Object.freeze(added),
			removed: Object.freeze(removed)
		}));
	}

	getLanguageModelIds(): string[] {
		return Array.from(this._languageModelIds);
	}

	$updateAllowlist(data: { extension: ExtensionIdentifier; allowed: boolean }[]): void {
		const updated = new ExtensionIdentifierSet();
		for (const { extension, allowed } of data) {
			const oldValue = this._accessAllowlist.get(extension);
			if (oldValue !== allowed) {
				this._accessAllowlist.set(extension, allowed);
				updated.add(extension);
			}
		}
		this._onDidChangeAccess.fire(updated);
	}

	async requestLanguageModelAccess(from: ExtensionIdentifier, languageModelId: string, options?: vscode.LanguageModelAccessOptions): Promise<vscode.LanguageModelAccess> {
		if (!this._accessAllowlist.get(from)) {
			throw new Error('Extension is NOT allowed to make chat requests');
		}

		const metadata = await this._proxy.$prepareChatAccess(languageModelId, options?.justification);
		if (!metadata) {
			throw new Error(`Language model '${languageModelId}' NOT found`);
		}

		const that = this;

		return {
			get model() {
				return metadata.model;
			},
			get isRevoked() {
				return !that._accessAllowlist.get(from) || !that._languageModelIds.has(languageModelId);
			},
			get onDidChangeAccess() {
				const onDidChangeAccess = Event.filter(that._onDidChangeAccess.event, set => set.has(from));
				const onDidRemoveLM = Event.filter(that._onDidChangeProviders.event, e => e.removed.includes(languageModelId));
				return Event.signal(Event.any(onDidChangeAccess, onDidRemoveLM));
			},
			makeRequest(messages, options, token) {
				if (!that._accessAllowlist.get(from)) {
					throw new Error('Access to chat has been revoked');
				}
				if (!that._languageModelIds.has(languageModelId)) {
					throw new Error('Language Model has been removed');
				}
				const cts = new CancellationTokenSource(token);
				const requestId = (Math.random() * 1e6) | 0;
				const requestPromise = that._proxy.$fetchResponse(from, languageModelId, requestId, messages.map(typeConvert.ChatMessage.from), options ?? {}, cts.token);
				const res = new LanguageModelRequest(requestPromise, cts);
				that._pendingRequest.set(requestId, { languageModelId, res });

				requestPromise.finally(() => {
					that._pendingRequest.delete(requestId);
					cts.dispose();
				});

				return res.apiObject;
			},
		};
	}

	async $handleResponseFragment(requestId: number, chunk: IChatResponseFragment): Promise<void> {
		const data = this._pendingRequest.get(requestId);//.report(chunk);
		if (data) {
			data.res.handleFragment(chunk);
		}
	}
}
