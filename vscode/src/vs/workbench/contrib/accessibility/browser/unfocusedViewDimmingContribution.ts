/*---------------------------------------------------------------------------------------------
 *  Copyright (c) Microsoft Corporation. All rights reserved.
 *  Licensed under the MIT License. See License.txt in the project root for license information.
 *--------------------------------------------------------------------------------------------*/

import { Event } from 'vs/base/common/event';
import { Disposable, toDisposable } from 'vs/base/common/lifecycle';
import { clamp } from 'vs/base/common/numbers';
import { IConfigurationService } from 'vs/platform/configuration/common/configuration';
import { IWorkbenchContribution } from 'vs/workbench/common/contributions';
import { AccessibilitySettingId } from 'vs/workbench/contrib/accessibility/browser/accessibilityContribution';

export class UnfocusedViewDimmingContribution extends Disposable implements IWorkbenchContribution {
	constructor(
		@IConfigurationService configurationService: IConfigurationService,
	) {
		super();

		const elStyle = document.createElement('style');
		elStyle.className = 'accessibilityUnfocusedViewOpacity';
		document.head.appendChild(elStyle);
		this._register(toDisposable(() => elStyle.remove()));

		this._register(Event.runAndSubscribe(configurationService.onDidChangeConfiguration, e => {
			if (e && !e.affectsConfiguration(AccessibilitySettingId.UnfocusedViewOpacity)) {
				return;
			}

			let opacity: number;
			const opacityConfig = configurationService.getValue(AccessibilitySettingId.UnfocusedViewOpacity);
			if (typeof opacityConfig !== 'number') {
				opacity = 1;
			} else {
				opacity = clamp(opacityConfig, 0.2, 1);
			}

			const rules = new Set<string>();
			rules.add(`.monaco-workbench .terminal.xterm:not(.focus) { filter: opacity(${opacity}); }`);
			rules.add(`.monaco-workbench .editor-instance .monaco-editor:not(.focused) { filter: opacity(${opacity}); }`);

			elStyle.textContent = [...rules].join('\n');
		}));
	}
}
