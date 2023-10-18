/*---------------------------------------------------------------------------------------------
 *  Copyright (c) Microsoft Corporation. All rights reserved.
 *  Licensed under the MIT License. See License.txt in the project root for license information.
 *--------------------------------------------------------------------------------------------*/

import { IKeyboardEvent, StandardKeyboardEvent } from 'vs/base/browser/keyboardEvent';
import { Emitter } from 'vs/base/common/event';
import { DisposableStore } from 'vs/base/common/lifecycle';
import { IModelService } from 'vs/editor/common/services/model';
import { IConfigurationService } from 'vs/platform/configuration/common/configuration';
import { IContextKeyService } from 'vs/platform/contextkey/common/contextkey';
import { IContextViewService } from 'vs/platform/contextview/browser/contextView';
import { IKeybindingService } from 'vs/platform/keybinding/common/keybinding';
import { ResultKind } from 'vs/platform/keybinding/common/keybindingResolver';
import { TerminalCapability, ITerminalCommand } from 'vs/platform/terminal/common/capabilities/capabilities';
import { ICurrentPartialCommand } from 'vs/platform/terminal/common/capabilities/commandDetectionCapability';
import { TerminalSettingId } from 'vs/platform/terminal/common/terminal';
import { AccessibilityVerbositySettingId, AccessibleViewProviderId } from 'vs/workbench/contrib/accessibility/browser/accessibilityConfiguration';
import { AccessibleViewType, IAccessibleContentProvider, IAccessibleViewOptions, IAccessibleViewSymbol } from 'vs/workbench/contrib/accessibility/browser/accessibleView';
import { ITerminalInstance, ITerminalService } from 'vs/workbench/contrib/terminal/browser/terminal';
import { BufferContentTracker } from 'vs/workbench/contrib/terminalContrib/accessibility/browser/bufferContentTracker';

export class TerminalAccessibleBufferProvider extends DisposableStore implements IAccessibleContentProvider {
	id = AccessibleViewProviderId.Terminal;
	options: IAccessibleViewOptions = { type: AccessibleViewType.View, language: 'terminal', id: AccessibleViewProviderId.Terminal };
	verbositySettingKey = AccessibilityVerbositySettingId.Terminal;
	private readonly _onDidRequestClearProvider = new Emitter<AccessibleViewProviderId>();
	readonly onDidRequestClearLastProvider = this._onDidRequestClearProvider.event;
	private _focusedInstance: ITerminalInstance | undefined;
	constructor(
		private readonly _instance: Pick<ITerminalInstance, 'onDidRunText' | 'focus' | 'shellType' | 'capabilities' | 'onDidRequestFocus' | 'resource' | 'onDisposed'>,
		private _bufferTracker: BufferContentTracker,
		customHelp: () => string,
		@IModelService _modelService: IModelService,
		@IConfigurationService configurationService: IConfigurationService,
		@IContextKeyService _contextKeyService: IContextKeyService,
		@ITerminalService _terminalService: ITerminalService,
		@IKeybindingService private readonly _keybindingService: IKeybindingService,
		@IContextViewService private readonly _contextViewService: IContextViewService
	) {
		super();
		this.options.customHelp = customHelp;
		this.options.position = configurationService.getValue(TerminalSettingId.AccessibleViewPreserveCursorPosition) ? 'initial-bottom' : 'bottom';
		this.add(this._instance.onDisposed(() => this._onDidRequestClearProvider.fire(AccessibleViewProviderId.Terminal)));
		this.add(configurationService.onDidChangeConfiguration(e => {
			if (e.affectsConfiguration(TerminalSettingId.AccessibleViewPreserveCursorPosition)) {
				this.options.position = configurationService.getValue(TerminalSettingId.AccessibleViewPreserveCursorPosition) ? 'initial-bottom' : 'bottom';
			}
		}));
		this._focusedInstance = _terminalService.activeInstance;
		this.add(_terminalService.onDidChangeActiveInstance(() => {
			if (_terminalService.activeInstance && this._focusedInstance?.instanceId !== _terminalService.activeInstance?.instanceId) {
				this._onDidRequestClearProvider.fire(AccessibleViewProviderId.Terminal);
				this._focusedInstance = _terminalService.activeInstance;
			}
		}));
	}

	onKeyDown(e: IKeyboardEvent): void {
		if (!shouldFocusTerminal(e.browserEvent, this._keybindingService)) {
			return;
		}
		this._contextViewService.hideContextView();
		this._instance.focus();
	}

	onClose() {
		this._instance.focus();
	}

	provideContent(): string {
		this._bufferTracker.update();
		return this._bufferTracker.lines.join('\n');
	}

	getSymbols(): IAccessibleViewSymbol[] {
		const commands = this._getCommandsWithEditorLine() ?? [];
		const symbols: IAccessibleViewSymbol[] = [];
		for (const command of commands) {
			const label = command.command.command;
			if (label) {
				symbols.push({
					label,
					lineNumber: command.lineNumber
				});
			}
		}
		return symbols;
	}

	private _getCommandsWithEditorLine(): ICommandWithEditorLine[] | undefined {
		const capability = this._instance.capabilities.get(TerminalCapability.CommandDetection);
		const commands = capability?.commands;
		const currentCommand = capability?.currentCommand;
		if (!commands?.length) {
			return;
		}
		const result: ICommandWithEditorLine[] = [];
		for (const command of commands) {
			const lineNumber = this._getEditorLineForCommand(command);
			if (lineNumber === undefined) {
				continue;
			}
			result.push({ command, lineNumber });
		}
		if (currentCommand) {
			const lineNumber = this._getEditorLineForCommand(currentCommand);
			if (lineNumber !== undefined) {
				result.push({ command: currentCommand, lineNumber });
			}
		}
		return result;
	}
	private _getEditorLineForCommand(command: ITerminalCommand | ICurrentPartialCommand): number | undefined {
		let line: number | undefined;
		if ('marker' in command) {
			line = command.marker?.line;
		} else if ('commandStartMarker' in command) {
			line = command.commandStartMarker?.line;
		}
		if (line === undefined || line < 0) {
			return;
		}
		line = this._bufferTracker.bufferToEditorLineMapping.get(line);
		if (line === undefined) {
			return;
		}
		return line + 1;
	}
}
export interface ICommandWithEditorLine { command: ITerminalCommand | ICurrentPartialCommand; lineNumber: number }

function shouldFocusTerminal(event: KeyboardEvent, keybindingService: IKeybindingService): boolean {
	const standardKeyboardEvent = new StandardKeyboardEvent(event);
	const resolveResult = keybindingService.softDispatch(standardKeyboardEvent, standardKeyboardEvent.target);

	const isValidChord = resolveResult.kind === ResultKind.MoreChordsNeeded;
	if (keybindingService.inChordMode || isValidChord) {
		return false;
	}
	return event.key.length === 1 && !event.ctrlKey && !event.altKey && !event.metaKey && !event.shiftKey;
}
