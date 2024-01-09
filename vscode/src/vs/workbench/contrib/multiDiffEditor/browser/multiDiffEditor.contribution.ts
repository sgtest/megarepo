/*---------------------------------------------------------------------------------------------
 *  Copyright (c) Microsoft Corporation. All rights reserved.
 *  Licensed under the MIT License. See License.txt in the project root for license information.
 *--------------------------------------------------------------------------------------------*/

import { localize } from 'vs/nls';
import { registerAction2 } from 'vs/platform/actions/common/actions';
import { Extensions, IConfigurationRegistry } from 'vs/platform/configuration/common/configurationRegistry';
import { SyncDescriptor } from 'vs/platform/instantiation/common/descriptors';
import { Registry } from 'vs/platform/registry/common/platform';
import { EditorPaneDescriptor, IEditorPaneRegistry } from 'vs/workbench/browser/editor';
import { IWorkbenchContributionsRegistry, Extensions as WorkbenchExtensions } from 'vs/workbench/common/contributions';
import { EditorExtensions, IEditorFactoryRegistry } from 'vs/workbench/common/editor';
import { MultiDiffEditor } from 'vs/workbench/contrib/multiDiffEditor/browser/multiDiffEditor';
import { MultiDiffEditorInput, MultiDiffEditorResolverContribution, MultiDiffEditorSerializer } from 'vs/workbench/contrib/multiDiffEditor/browser/multiDiffEditorInput';
import { LifecyclePhase } from 'vs/workbench/services/lifecycle/common/lifecycle';
import { CollapseAllAction, ExpandAllAction, GoToFileAction } from './actions';
import { IMultiDiffSourceResolverService, MultiDiffSourceResolverService } from 'vs/workbench/contrib/multiDiffEditor/browser/multiDiffSourceResolverService';
import { InstantiationType, registerSingleton } from 'vs/platform/instantiation/common/extensions';

registerAction2(GoToFileAction);
registerAction2(CollapseAllAction);
registerAction2(ExpandAllAction);

Registry.as<IConfigurationRegistry>(Extensions.Configuration)
	.registerConfiguration({
		properties: {
			'multiDiffEditor.experimental.enabled': {
				type: 'boolean',
				default: false,
				description: 'Enable experimental multi diff editor.',
			},
		}
	});

registerSingleton(IMultiDiffSourceResolverService, MultiDiffSourceResolverService, InstantiationType.Delayed);

// Editor Integration
Registry.as<IWorkbenchContributionsRegistry>(WorkbenchExtensions.Workbench)
	.registerWorkbenchContribution(MultiDiffEditorResolverContribution, LifecyclePhase.Starting);

Registry.as<IEditorPaneRegistry>(EditorExtensions.EditorPane)
	.registerEditorPane(
		EditorPaneDescriptor.create(MultiDiffEditor, MultiDiffEditor.ID, localize('name', "Multi Diff Editor")),
		[new SyncDescriptor(MultiDiffEditorInput)]
	);

Registry.as<IEditorFactoryRegistry>(EditorExtensions.EditorFactory)
	.registerEditorSerializer(MultiDiffEditorInput.ID, MultiDiffEditorSerializer);
