/*---------------------------------------------------------------------------------------------
 *  Copyright (c) Microsoft Corporation. All rights reserved.
 *  Licensed under the MIT License. See License.txt in the project root for license information.
 *--------------------------------------------------------------------------------------------*/

import { DeferredPromise, raceCancellation } from 'vs/base/common/async';
import { CancellationToken } from 'vs/base/common/cancellation';
import { toErrorMessage } from 'vs/base/common/errorMessage';
import { assertType } from 'vs/base/common/types';
import { URI } from 'vs/base/common/uri';
import { ExtensionIdentifier } from 'vs/platform/extensions/common/extensions';
import { ILogService } from 'vs/platform/log/common/log';
import { Progress } from 'vs/platform/progress/common/progress';
import { ExtHostChatAgentsShape2, IMainContext, MainContext, MainThreadChatAgentsShape2 } from 'vs/workbench/api/common/extHost.protocol';
import { ExtHostChatProvider } from 'vs/workbench/api/common/extHostChatProvider';
import * as typeConvert from 'vs/workbench/api/common/extHostTypeConverters';
import { IChatAgentCommand, IChatAgentRequest, IChatAgentResult } from 'vs/workbench/contrib/chat/common/chatAgents';
import { IChatMessage } from 'vs/workbench/contrib/chat/common/chatProvider';
import { IChatFollowup } from 'vs/workbench/contrib/chat/common/chatService';
import type * as vscode from 'vscode';

export class ExtHostChatAgents2 implements ExtHostChatAgentsShape2 {

	private static _idPool = 0;

	private readonly _agents = new Map<number, ExtHostChatAgent>();
	private readonly _proxy: MainThreadChatAgentsShape2;

	constructor(
		mainContext: IMainContext,
		private readonly _extHostChatProvider: ExtHostChatProvider,
		private readonly _logService: ILogService,
	) {
		this._proxy = mainContext.getProxy(MainContext.MainThreadChatAgents2);
	}

	createChatAgent(extension: ExtensionIdentifier, name: string, handler: vscode.ChatAgentHandler): vscode.ChatAgent2 {
		const handle = ExtHostChatAgents2._idPool++;
		const agent = new ExtHostChatAgent(extension, name, this._proxy, handle, handler);
		this._agents.set(handle, agent);

		this._proxy.$registerAgent(handle, name, {});
		return agent.apiAgent;
	}

	async $invokeAgent(handle: number, requestId: number, request: IChatAgentRequest, context: { history: IChatMessage[] }, token: CancellationToken): Promise<IChatAgentResult | undefined> {
		const agent = this._agents.get(handle);
		if (!agent) {
			throw new Error(`[CHAT](${handle}) CANNOT invoke agent because the agent is not registered`);
		}

		let done = false;
		function throwIfDone() {
			if (done) {
				throw new Error('Only valid while executing the command');
			}
		}

		const commandExecution = new DeferredPromise<void>();
		token.onCancellationRequested(() => commandExecution.complete());
		setTimeout(() => commandExecution.complete(), 3 * 1000);
		this._extHostChatProvider.allowListExtensionWhile(agent.extension, commandExecution.p);

		const slashCommand = request.command
			? await agent.validateSlashCommand(request.command)
			: undefined;


		try {

			const task = agent.invoke(
				{ prompt: request.message, variables: {}, slashCommand },
				{ history: context.history.map(typeConvert.ChatMessage.to) },
				new Progress<vscode.InteractiveProgress>(p => {
					throwIfDone();
					const convertedProgress = typeConvert.ChatResponseProgress.from(p);
					this._proxy.$handleProgressChunk(requestId, convertedProgress);
				}),
				token
			);

			return await raceCancellation(Promise.resolve(task).then((result) => {
				if (result) {
					// An option would be to call provideFollowups here and send the result back to the renderer, rather than store the result
					// and wait for the renderer to ask for followups
					// agent.provideFollowups(result, token);
					return { errorDetails: result.errorDetails }; // TODO timings here
				}

				return undefined;
			}), token);

		} catch (e) {
			this._logService.error(e, agent.extension);
			return {
				errorDetails: {
					message: toErrorMessage(e)
				}
			};

		} finally {
			done = true;
			commandExecution.complete();
		}
	}

	async $provideSlashCommands(handle: number, token: CancellationToken): Promise<IChatAgentCommand[]> {
		const agent = this._agents.get(handle);
		if (!agent) {
			// this is OK, the agent might have disposed while the request was in flight
			return [];
		}
		return agent.provideSlashCommand(token);
	}

	async $provideFollowups(handle: number, requestId: number, token: CancellationToken): Promise<IChatFollowup[]> {
		const agent = this._agents.get(handle);
		if (!agent) {
			// this is OK, the agent might have disposed while the request was in flight
			return [];
		}

		// TODO look up result object based on requestId
		return agent.provideFollowups(null!, token);
	}
}

class ExtHostChatAgent {

	private _slashCommandProvider: vscode.ChatAgentSlashCommandProvider | undefined;
	private _lastSlashCommands: vscode.ChatAgentSlashCommand[] | undefined;
	private _followupProvider: vscode.FollowupProvider | undefined;
	private _description: string | undefined;
	private _fullName: string | undefined;
	private _iconPath: URI | undefined;

	constructor(
		public readonly extension: ExtensionIdentifier,
		private readonly _id: string,
		private readonly _proxy: MainThreadChatAgentsShape2,
		private readonly _handle: number,
		private readonly _callback: vscode.ChatAgentHandler,
	) { }


	async validateSlashCommand(command: string) {
		if (!this._lastSlashCommands) {
			await this.provideSlashCommand(CancellationToken.None);
			assertType(this._lastSlashCommands);
		}
		const result = this._lastSlashCommands.find(candidate => candidate.name === command);
		if (!result) {
			throw new Error(`Unknown slashCommand: ${command}`);

		}
		return result;
	}

	async provideSlashCommand(token: CancellationToken): Promise<IChatAgentCommand[]> {
		if (!this._slashCommandProvider) {
			return [];
		}
		const result = await this._slashCommandProvider.provideSlashCommands(token);
		if (!result) {
			return [];
		}
		this._lastSlashCommands = result;
		return result.map(c => ({ name: c.name, description: c.description }));
	}

	async provideFollowups(result: vscode.ChatAgentResult2, token: CancellationToken): Promise<IChatFollowup[]> {
		if (!this._followupProvider) {
			return [];
		}
		const followups = await this._followupProvider.provideFollowups(result, token);
		if (!followups) {
			return [];
		}
		return followups.map(f => typeConvert.ChatFollowup.from(f));
	}

	get apiAgent(): vscode.ChatAgent2 {

		let updateScheduled = false;
		const updateMetadataSoon = () => {
			if (updateScheduled) {
				return;
			}
			updateScheduled = true;
			queueMicrotask(() => {
				this._proxy.$updateAgent(this._handle, {
					description: this._description ?? '',
					fullName: this._fullName,
					icon: this._iconPath,
					hasSlashCommands: this._slashCommandProvider !== undefined,
					hasFollowup: this._followupProvider !== undefined,
				});
				updateScheduled = false;
			});
		};

		const that = this;
		return {
			get name() {
				return that._id;
			},
			get description() {
				return that._description ?? '';
			},
			set description(v) {
				that._description = v;
				updateMetadataSoon();
			},
			get fullName() {
				return that._fullName ?? that.extension.value;
			},
			set fullName(v) {
				that._fullName = v;
				updateMetadataSoon();
			},
			get iconPath() {
				return that._iconPath;
			},
			set iconPath(v) {
				that._iconPath = v;
				updateMetadataSoon();
			},
			// onDidPerformAction
			get slashCommandProvider() {
				return that._slashCommandProvider;
			},
			set slashCommandProvider(v) {
				that._slashCommandProvider = v;
				updateMetadataSoon();
			},
			get followupProvider() {
				return that._followupProvider;
			},
			set followupProvider(v) {
				that._followupProvider = v;
				updateMetadataSoon();
			},
			dispose() {
				that._proxy.$unregisterAgent(that._handle);
			},
		} satisfies vscode.ChatAgent2;
	}

	invoke(request: vscode.ChatAgentRequest, context: vscode.ChatAgentContext, progress: Progress<vscode.InteractiveProgress>, token: CancellationToken): vscode.ProviderResult<vscode.ChatAgentResult2> {
		return this._callback(request, context, progress, token);
	}
}
