/*---------------------------------------------------------------------------------------------
 *  Copyright (c) Microsoft Corporation. All rights reserved.
 *  Licensed under the MIT License. See License.txt in the project root for license information.
 *--------------------------------------------------------------------------------------------*/
import { Emitter } from 'vs/base/common/event';
import { Disposable } from 'vs/base/common/lifecycle';
import { autorunHandleChanges } from 'vs/base/common/observableImpl/autorun';
import { IEditorConstructionOptions } from 'vs/editor/browser/config/editorConfiguration';
import { IDiffEditorConstructionOptions } from 'vs/editor/browser/editorBrowser';
import { CodeEditorWidget, ICodeEditorWidgetOptions } from 'vs/editor/browser/widget/codeEditorWidget';
import { IDiffCodeEditorWidgetOptions } from 'vs/editor/browser/widget/diffEditorWidget';
import { OverviewRulerPart } from 'vs/editor/browser/widget/diffEditorWidget2/overviewRulerPart';
import { EditorOptions, IEditorOptions } from 'vs/editor/common/config/editorOptions';
import { IContentSizeChangedEvent } from 'vs/editor/common/editorCommon';
import { localize } from 'vs/nls';
import { IContextKeyService } from 'vs/platform/contextkey/common/contextkey';
import { IInstantiationService } from 'vs/platform/instantiation/common/instantiation';
import { DiffEditorOptions } from './diffEditorOptions';

export class DiffEditorEditors extends Disposable {
	public readonly modified: CodeEditorWidget;
	public readonly original: CodeEditorWidget;

	private readonly _onDidContentSizeChange = this._register(new Emitter<IContentSizeChangedEvent>());
	public get onDidContentSizeChange() { return this._onDidContentSizeChange.event; }

	constructor(
		private readonly originalEditorElement: HTMLElement,
		private readonly modifiedEditorElement: HTMLElement,
		private readonly _options: DiffEditorOptions,
		codeEditorWidgetOptions: IDiffCodeEditorWidgetOptions,
		private readonly _createInnerEditor: (instantiationService: IInstantiationService, container: HTMLElement, options: Readonly<IEditorOptions>, editorWidgetOptions: ICodeEditorWidgetOptions) => CodeEditorWidget,
		@IContextKeyService private readonly _contextKeyService: IContextKeyService,
		@IInstantiationService private readonly _instantiationService: IInstantiationService
	) {
		super();

		this.original = this._createLeftHandSideEditor(_options.editorOptions.get(), codeEditorWidgetOptions.originalEditor || {});
		this.modified = this._createRightHandSideEditor(_options.editorOptions.get(), codeEditorWidgetOptions.modifiedEditor || {});

		this._register(autorunHandleChanges('update editor options', {
			createEmptyChangeSummary: () => ({} as IDiffEditorConstructionOptions),
			handleChange: (ctx, changeSummary) => {
				if (ctx.didChange(_options.editorOptions)) {
					Object.assign(changeSummary, ctx.change.changedOptions);
				}
				return true;
			}
		}, (reader, changeSummary) => {
			_options.editorOptions.read(reader);

			this.modified.updateOptions(this._adjustOptionsForRightHandSide(changeSummary));
			this.original.updateOptions(this._adjustOptionsForLeftHandSide(changeSummary));
		}));
	}

	private _createLeftHandSideEditor(options: Readonly<IDiffEditorConstructionOptions>, codeEditorWidgetOptions: ICodeEditorWidgetOptions): CodeEditorWidget {
		const editor = this._constructInnerEditor(this._instantiationService, this.originalEditorElement, this._adjustOptionsForLeftHandSide(options), codeEditorWidgetOptions);
		const isInDiffLeftEditorKey = this._contextKeyService.createKey<boolean>('isInDiffLeftEditor', editor.hasWidgetFocus());
		this._register(editor.onDidFocusEditorWidget(() => isInDiffLeftEditorKey.set(true)));
		this._register(editor.onDidBlurEditorWidget(() => isInDiffLeftEditorKey.set(false)));
		return editor;
	}

	private _createRightHandSideEditor(options: Readonly<IDiffEditorConstructionOptions>, codeEditorWidgetOptions: ICodeEditorWidgetOptions): CodeEditorWidget {
		const editor = this._constructInnerEditor(this._instantiationService, this.modifiedEditorElement, this._adjustOptionsForRightHandSide(options), codeEditorWidgetOptions);
		const isInDiffRightEditorKey = this._contextKeyService.createKey<boolean>('isInDiffRightEditor', editor.hasWidgetFocus());
		this._register(editor.onDidFocusEditorWidget(() => isInDiffRightEditorKey.set(true)));
		this._register(editor.onDidBlurEditorWidget(() => isInDiffRightEditorKey.set(false)));
		return editor;
	}

	private _constructInnerEditor(instantiationService: IInstantiationService, container: HTMLElement, options: Readonly<IEditorConstructionOptions>, editorWidgetOptions: ICodeEditorWidgetOptions): CodeEditorWidget {
		const editor = this._createInnerEditor(instantiationService, container, options, editorWidgetOptions);

		this._register(editor.onDidContentSizeChange(e => {
			const width = this.original.getContentWidth() + this.modified.getContentWidth() + OverviewRulerPart.ENTIRE_DIFF_OVERVIEW_WIDTH;
			const height = Math.max(this.modified.getContentHeight(), this.original.getContentHeight());

			this._onDidContentSizeChange.fire({
				contentHeight: height,
				contentWidth: width,
				contentHeightChanged: e.contentHeightChanged,
				contentWidthChanged: e.contentWidthChanged
			});
		}));
		return editor;
	}

	private _adjustOptionsForLeftHandSide(changedOptions: Readonly<IDiffEditorConstructionOptions>): IEditorConstructionOptions {
		const result = this._adjustOptionsForSubEditor(changedOptions);
		if (!this._options.renderSideBySide.get()) {
			// never wrap hidden editor
			result.wordWrapOverride1 = 'off';
			result.wordWrapOverride2 = 'off';
			result.stickyScroll = { enabled: false };
		} else {
			result.wordWrapOverride1 = this._options.diffWordWrap.get();
		}
		if (changedOptions.originalAriaLabel) {
			result.ariaLabel = changedOptions.originalAriaLabel;
		}
		result.ariaLabel = this._updateAriaLabel(result.ariaLabel);
		result.readOnly = !this._options.originalEditable.get();
		result.dropIntoEditor = { enabled: !result.readOnly };
		result.extraEditorClassName = 'original-in-monaco-diff-editor';
		return result;
	}

	private _adjustOptionsForRightHandSide(changedOptions: Readonly<IDiffEditorConstructionOptions>): IEditorConstructionOptions {
		const result = this._adjustOptionsForSubEditor(changedOptions);
		if (changedOptions.modifiedAriaLabel) {
			result.ariaLabel = changedOptions.modifiedAriaLabel;
		}
		result.ariaLabel = this._updateAriaLabel(result.ariaLabel);
		result.wordWrapOverride1 = this._options.diffWordWrap.get();
		result.revealHorizontalRightPadding = EditorOptions.revealHorizontalRightPadding.defaultValue + OverviewRulerPart.ENTIRE_DIFF_OVERVIEW_WIDTH;
		result.scrollbar!.verticalHasArrows = false;
		result.extraEditorClassName = 'modified-in-monaco-diff-editor';
		return result;
	}

	private _adjustOptionsForSubEditor(options: Readonly<IDiffEditorConstructionOptions>): IEditorConstructionOptions {
		const clonedOptions = {
			...options,
			dimension: {
				height: 0,
				width: 0
			},
		};
		clonedOptions.inDiffEditor = true;
		clonedOptions.automaticLayout = false;
		// Clone scrollbar options before changing them
		clonedOptions.scrollbar = { ...(clonedOptions.scrollbar || {}) };
		clonedOptions.scrollbar.vertical = 'visible';
		clonedOptions.folding = false;
		clonedOptions.codeLens = this._options.diffCodeLens.get();
		clonedOptions.fixedOverflowWidgets = true;
		// clonedOptions.lineDecorationsWidth = '2ch';
		// Clone minimap options before changing them
		clonedOptions.minimap = { ...(clonedOptions.minimap || {}) };
		clonedOptions.minimap.enabled = false;

		if (this._options.collapseUnchangedRegions.get()) {
			clonedOptions.stickyScroll = { enabled: false };
		} else {
			clonedOptions.stickyScroll = this._options.editorOptions.get().stickyScroll;
		}
		return clonedOptions;
	}

	private _updateAriaLabel(ariaLabel: string | undefined): string | undefined {
		const ariaNavigationTip = localize('diff-aria-navigation-tip', ' use Shift + F7 to navigate changes');
		if (this._options.accessibilityVerbose.get()) {
			return ariaLabel + ariaNavigationTip;
		} else if (ariaLabel) {
			return ariaLabel.replaceAll(ariaNavigationTip, '');
		}
		return undefined;
	}
}
