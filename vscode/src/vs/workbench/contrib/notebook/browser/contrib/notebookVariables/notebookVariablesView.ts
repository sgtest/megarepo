/*---------------------------------------------------------------------------------------------
 *  Copyright (c) Microsoft Corporation. All rights reserved.
 *  Licensed under the MIT License. See License.txt in the project root for license information.
 *--------------------------------------------------------------------------------------------*/

import * as nls from 'vs/nls';
import { ILocalizedString } from 'vs/platform/action/common/action';
import { ICommandService } from 'vs/platform/commands/common/commands';
import { IConfigurationService } from 'vs/platform/configuration/common/configuration';
import { IContextKeyService } from 'vs/platform/contextkey/common/contextkey';
import { IContextMenuService } from 'vs/platform/contextview/browser/contextView';
import { IInstantiationService } from 'vs/platform/instantiation/common/instantiation';
import { IKeybindingService } from 'vs/platform/keybinding/common/keybinding';
import { WorkbenchObjectTree } from 'vs/platform/list/browser/listService';
import { IOpenerService } from 'vs/platform/opener/common/opener';
import { IQuickInputService } from 'vs/platform/quickinput/common/quickInput';
import { ITelemetryService } from 'vs/platform/telemetry/common/telemetry';
import { IThemeService } from 'vs/platform/theme/common/themeService';
import { IViewPaneOptions, ViewPane } from 'vs/workbench/browser/parts/views/viewPane';
import { IViewDescriptorService } from 'vs/workbench/common/views';
import { mockVariables, NotebookVariableAccessibilityProvider, NotebookVariableRenderer, INotebookVariableElement, NotebookVariablesDelegate, NotebookVariablesTree } from 'vs/workbench/contrib/notebook/browser/contrib/notebookVariables/notebookVariablesTree';
import { getNotebookEditorFromEditorPane } from 'vs/workbench/contrib/notebook/browser/notebookBrowser';
import { NotebookTextModel } from 'vs/workbench/contrib/notebook/common/model/notebookTextModel';
import { ICellExecutionStateChangedEvent, IExecutionStateChangedEvent, INotebookExecutionStateService } from 'vs/workbench/contrib/notebook/common/notebookExecutionStateService';
import { INotebookKernelService } from 'vs/workbench/contrib/notebook/common/notebookKernelService';
import { IEditorService } from 'vs/workbench/services/editor/common/editorService';

export class NotebookVariablesView extends ViewPane {

	static readonly ID = 'notebookVariablesView';
	static readonly TITLE: ILocalizedString = nls.localize2('notebook.notebookVariables', "Notebook Variables");

	private tree: NotebookVariablesTree | undefined;
	private activeNotebook: NotebookTextModel | undefined;

	constructor(
		options: IViewPaneOptions,
		@IEditorService private readonly editorService: IEditorService,
		@INotebookKernelService private readonly notebookKernelService: INotebookKernelService,
		@INotebookExecutionStateService private readonly notebookExecutionStateService: INotebookExecutionStateService,
		@IKeybindingService keybindingService: IKeybindingService,
		@IContextMenuService contextMenuService: IContextMenuService,
		@IContextKeyService contextKeyService: IContextKeyService,
		@IConfigurationService configurationService: IConfigurationService,
		@IInstantiationService instantiationService: IInstantiationService,
		@IViewDescriptorService viewDescriptorService: IViewDescriptorService,
		@IOpenerService openerService: IOpenerService,
		@IQuickInputService protected quickInputService: IQuickInputService,
		@ICommandService protected commandService: ICommandService,
		@IThemeService themeService: IThemeService,
		@ITelemetryService telemetryService: ITelemetryService,
	) {
		super(options, keybindingService, contextMenuService, configurationService, contextKeyService, viewDescriptorService, instantiationService, openerService, themeService, telemetryService);

		this._register(this.editorService.onDidActiveEditorChange(this.handleActiveEditorChange.bind(this)));
		this._register(this.notebookExecutionStateService.onDidChangeExecution(this.handleExecutionStateChange.bind(this)));
	}

	public override renderBody(container: HTMLElement): void {
		super.renderBody(container);

		this.tree = <NotebookVariablesTree>this.instantiationService.createInstance(
			WorkbenchObjectTree,
			'notebookVariablesTree',
			container,
			new NotebookVariablesDelegate(),
			[new NotebookVariableRenderer()],
			{
				identityProvider: { getId: (e: INotebookVariableElement) => e.id },
				horizontalScrolling: false,
				supportDynamicHeights: true,
				hideTwistiesOfChildlessElements: true,
				accessibilityProvider: new NotebookVariableAccessibilityProvider(),
				setRowLineHeight: false,
			});

		this.tree.layout();
		this.tree?.setChildren(null, []);
	}

	protected override layoutBody(height: number, width: number): void {
		super.layoutBody(height, width);
		this.tree?.layout(height, width);
	}

	private handleActiveEditorChange() {
		const activeEditorPane = this.editorService.activeEditorPane;
		if (activeEditorPane && activeEditorPane.getId() === 'workbench.editor.notebook') {
			const notebookDocument = getNotebookEditorFromEditorPane(activeEditorPane)?.getViewModel()?.notebookDocument;
			if (notebookDocument && notebookDocument !== this.activeNotebook) {
				this.activeNotebook = notebookDocument;
				this.updateVariables(this.activeNotebook);
			}
		}
	}

	private handleExecutionStateChange(event: ICellExecutionStateChangedEvent | IExecutionStateChangedEvent) {
		if (this.activeNotebook) {
			// changed === undefined -> excecution ended
			if (event.changed === undefined && event.affectsNotebook(this.activeNotebook?.uri)) {
				this.updateVariables(this.activeNotebook);
			}
		}
	}

	private updateVariables(notebook: NotebookTextModel) {
		const selectedKernel = this.notebookKernelService.getMatchingKernel(notebook).selected;
		if (selectedKernel) {

			this.tree?.setChildren(null, mockVariables(notebook.uri.toString(), selectedKernel.label));
		}
	}
}
