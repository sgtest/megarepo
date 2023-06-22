/*---------------------------------------------------------------------------------------------
 *  Copyright (c) Microsoft Corporation. All rights reserved.
 *  Licensed under the MIT License. See License.txt in the project root for license information.
 *--------------------------------------------------------------------------------------------*/
import { $, h } from 'vs/base/browser/dom';
import { IBoundarySashes } from 'vs/base/browser/ui/sash/sash';
import { findLast } from 'vs/base/common/arrays';
import { onUnexpectedError } from 'vs/base/common/errors';
import { Emitter, Event } from 'vs/base/common/event';
import { deepClone } from 'vs/base/common/objects';
import { IObservable, ISettableObservable, autorun, derived, keepAlive, observableValue } from 'vs/base/common/observable';
import { autorunWithStore2 } from 'vs/base/common/observableImpl/autorun';
import { disposableObservableValue, transaction } from 'vs/base/common/observableImpl/base';
import { derivedWithStore } from 'vs/base/common/observableImpl/derived';
import { Constants } from 'vs/base/common/uint';
import 'vs/css!./style';
import { IEditorConstructionOptions } from 'vs/editor/browser/config/editorConfiguration';
import { ICodeEditor, IDiffEditor, IDiffEditorConstructionOptions, IMouseTargetViewZone } from 'vs/editor/browser/editorBrowser';
import { EditorExtensionsRegistry, IDiffEditorContributionDescription } from 'vs/editor/browser/editorExtensions';
import { ICodeEditorService } from 'vs/editor/browser/services/codeEditorService';
import { CodeEditorWidget, ICodeEditorWidgetOptions } from 'vs/editor/browser/widget/codeEditorWidget';
import { IDiffCodeEditorWidgetOptions } from 'vs/editor/browser/widget/diffEditorWidget';
import { DiffEditorDecorations } from 'vs/editor/browser/widget/diffEditorWidget2/diffEditorDecorations';
import { DiffEditorSash } from 'vs/editor/browser/widget/diffEditorWidget2/diffEditorSash';
import { DiffReview2 } from 'vs/editor/browser/widget/diffEditorWidget2/diffReview';
import { ViewZoneManager } from 'vs/editor/browser/widget/diffEditorWidget2/lineAlignment';
import { MovedBlocksLinesPart } from 'vs/editor/browser/widget/diffEditorWidget2/movedBlocksLines';
import { OverviewRulerPart } from 'vs/editor/browser/widget/diffEditorWidget2/overviewRulerPart';
import { UnchangedRangesFeature } from 'vs/editor/browser/widget/diffEditorWidget2/unchangedRanges';
import { ObservableElementSizeObserver, applyStyle, deepMerge, readHotReloadableExport } from 'vs/editor/browser/widget/diffEditorWidget2/utils';
import { WorkerBasedDocumentDiffProvider } from 'vs/editor/browser/widget/workerBasedDocumentDiffProvider';
import { EditorOptions, IDiffEditorOptions, IEditorOptions, ValidDiffEditorBaseOptions, clampedFloat, clampedInt, boolean as validateBooleanOption, stringSet as validateStringSetOption } from 'vs/editor/common/config/editorOptions';
import { IDimension } from 'vs/editor/common/core/dimension';
import { Position } from 'vs/editor/common/core/position';
import { LineRangeMapping } from 'vs/editor/common/diff/linesDiffComputer';
import { IDiffComputationResult, ILineChange } from 'vs/editor/common/diff/smartLinesDiffComputer';
import { EditorType, IContentSizeChangedEvent, IDiffEditorModel, IDiffEditorViewModel, IDiffEditorViewState } from 'vs/editor/common/editorCommon';
import { localize } from 'vs/nls';
import { IContextKeyService } from 'vs/platform/contextkey/common/contextkey';
import { IInstantiationService } from 'vs/platform/instantiation/common/instantiation';
import { ServiceCollection } from 'vs/platform/instantiation/common/serviceCollection';
import { DelegatingEditor } from './delegatingEditorImpl';
import { DiffMapping, DiffModel } from './diffModel';

const diffEditorDefaultOptions: ValidDiffEditorBaseOptions = {
	enableSplitViewResizing: true,
	splitViewDefaultRatio: 0.5,
	renderSideBySide: true,
	renderMarginRevertIcon: true,
	maxComputationTime: 5000,
	maxFileSize: 50,
	ignoreTrimWhitespace: true,
	renderIndicators: true,
	originalEditable: false,
	diffCodeLens: false,
	renderOverviewRuler: true,
	diffWordWrap: 'inherit',
	diffAlgorithm: 'advanced',
	accessibilityVerbose: false,
	experimental: {
		collapseUnchangedRegions: false,
		showMoves: false,
	}
};

export class DiffEditorWidget2 extends DelegatingEditor implements IDiffEditor {
	private readonly elements = h('div.monaco-diff-editor.side-by-side', { style: { position: 'relative', height: '100%' } }, [
		h('div.noModificationsOverlay@overlay', { style: { position: 'absolute', height: '100%', visibility: 'hidden', } }, [$('span', {}, 'No Changes')]),
		h('div.editor.original@original', { style: { position: 'absolute', height: '100%' } }),
		h('div.editor.modified@modified', { style: { position: 'absolute', height: '100%' } }),
	]);
	private readonly _model = observableValue<IDiffEditorModel | null>('diffEditorModel', null);
	public readonly onDidChangeModel = Event.fromObservableLight(this._model);
	private readonly _diffModel = this._register(disposableObservableValue<DiffModel | undefined>('diffModel', undefined));
	private readonly _onDidContentSizeChange = this._register(new Emitter<IContentSizeChangedEvent>());
	public readonly onDidContentSizeChange = this._onDidContentSizeChange.event;
	private readonly _modifiedEditor: CodeEditorWidget;
	private readonly _originalEditor: CodeEditorWidget;
	private readonly _contextKeyService = this._register(this._parentContextKeyService.createScoped(this._domElement));
	private readonly _instantiationService = this._parentInstantiationService.createChild(
		new ServiceCollection([IContextKeyService, this._contextKeyService])
	);
	private readonly _rootSizeObserver: ObservableElementSizeObserver;
	private readonly _options: ISettableObservable<ValidDiffEditorBaseOptions>;
	private _editorOptions: IEditorOptions;
	private readonly _sash: IObservable<DiffEditorSash | undefined>;
	private readonly _boundarySashes = observableValue<IBoundarySashes | undefined>('boundarySashes', undefined);
	private readonly _renderOverviewRuler = derived('renderOverviewRuler', reader => this._options.read(reader).renderOverviewRuler);
	private readonly _renderSideBySide = derived('renderSideBySide', reader => this._options.read(reader).renderSideBySide);

	private unchangedRangesFeature!: UnchangedRangesFeature;

	private readonly _reviewPane: DiffReview2;

	constructor(
		private readonly _domElement: HTMLElement,
		options: Readonly<IDiffEditorConstructionOptions>,
		codeEditorWidgetOptions: IDiffCodeEditorWidgetOptions,
		@IContextKeyService private readonly _parentContextKeyService: IContextKeyService,
		@IInstantiationService private readonly _parentInstantiationService: IInstantiationService,
		@ICodeEditorService codeEditorService: ICodeEditorService,
	) {
		super();

		codeEditorService.willCreateDiffEditor();

		this._contextKeyService.createKey('isInDiffEditor', true);
		this._contextKeyService.createKey('diffEditorVersion', 2);
		this._contextKeyService.createKey('isInEmbeddedDiffEditor',
			typeof options.isInEmbeddedEditor !== 'undefined' ? options.isInEmbeddedEditor : false
		);

		this._options = observableValue<ValidDiffEditorBaseOptions>('options', validateDiffEditorOptions(options || {}, diffEditorDefaultOptions));
		this._editorOptions = deepClone(options);

		this._domElement.appendChild(this.elements.root);

		this._rootSizeObserver = this._register(new ObservableElementSizeObserver(this.elements.root, options.dimension));
		this._rootSizeObserver.setAutomaticLayout(options.automaticLayout ?? false);

		this._originalEditor = this._createLeftHandSideEditor(options, codeEditorWidgetOptions.originalEditor || {});
		this._modifiedEditor = this._createRightHandSideEditor(options, codeEditorWidgetOptions.modifiedEditor || {});

		this._sash = derivedWithStore('sash', (reader, store) => {
			const showSash = this._options.read(reader).renderSideBySide;
			this.elements.root.classList.toggle('side-by-side', showSash);
			if (!showSash) {
				return undefined;
			}
			const result = store.add(new DiffEditorSash(
				this._options.map(o => o.enableSplitViewResizing),
				this._options.map(o => o.splitViewDefaultRatio),
				this.elements.root,
				{
					height: this._rootSizeObserver.height,
					width: this._rootSizeObserver.width.map((w, reader) => w - (this._renderOverviewRuler.read(reader) ? OverviewRulerPart.ENTIRE_DIFF_OVERVIEW_WIDTH : 0)),
				}
			));
			store.add(autorun('setBoundarySashes', reader => {
				const boundarySashes = this._boundarySashes.read(reader);
				if (boundarySashes) {
					result.setBoundarySashes(boundarySashes);
				}
			}));
			return result;
		});
		this._register(keepAlive(this._sash, true));

		this._register(autorunWithStore2('unchangedRangesFeature', (reader, store) => {
			this.unchangedRangesFeature = store.add(new (readHotReloadableExport(UnchangedRangesFeature, reader))(
				this._originalEditor, this._modifiedEditor, this._diffModel, this._renderSideBySide,
			));
		}));

		this._register(autorunWithStore2('decorations', (reader, store) => {
			store.add(new (readHotReloadableExport(DiffEditorDecorations, reader))(
				this._originalEditor, this._modifiedEditor, this._diffModel, this._renderSideBySide,
			));
		}));

		this._register(this._instantiationService.createInstance(
			ViewZoneManager,
			this._originalEditor,
			this._modifiedEditor,
			this._diffModel,
			this._renderSideBySide,
			this,
			() => this.unchangedRangesFeature.isUpdatingViewZones,
		));

		this._register(this._instantiationService.createInstance(OverviewRulerPart,
			this._originalEditor,
			this._modifiedEditor,
			this.elements.root,
			this._diffModel,
			this._rootSizeObserver.width,
			this._rootSizeObserver.height,
			this._layoutInfo.map(i => i.modifiedEditor),
			this._renderOverviewRuler,
		));

		this._reviewPane = this._register(this._instantiationService.createInstance(DiffReview2, this));
		this.elements.root.appendChild(this._reviewPane.domNode.domNode);
		this.elements.root.appendChild(this._reviewPane.shadow.domNode);
		this.elements.root.appendChild(this._reviewPane.actionBarContainer.domNode);

		this._createDiffEditorContributions();

		codeEditorService.addDiffEditor(this);

		this._register(keepAlive(this._layoutInfo, true));

		this._register(new MovedBlocksLinesPart(
			this.elements.root,
			this._diffModel,
			this._layoutInfo.map(i => i.originalEditor),
			this._layoutInfo.map(i => i.modifiedEditor),
			this._originalEditor,
			this._modifiedEditor,
		));

		this._register(applyStyle(this.elements.overlay, {
			width: this._layoutInfo.map((i, r) => i.originalEditor.width + (this._options.read(r).renderSideBySide ? 0 : i.modifiedEditor.width)),
			visibility: this._diffModel.map((m, r) => (m && m.hideUnchangedRegions.read(r) && m.diff.read(r)?.mappings.length === 0) ? 'visible' : 'hidden'),
		}));
	}

	private readonly _layoutInfo = derived('modifiedEditorLayoutInfo', (reader) => {
		const width = this._rootSizeObserver.width.read(reader);
		const height = this._rootSizeObserver.height.read(reader);
		const sashLeft = this._sash.read(reader)?.sashLeft.read(reader);

		const originalWidth = sashLeft ?? Math.max(5, this._originalEditor.getLayoutInfo().decorationsLeft);

		this.elements.original.style.width = originalWidth + 'px';
		this.elements.original.style.left = '0px';

		this.elements.modified.style.width = (width - originalWidth) + 'px';
		this.elements.modified.style.left = originalWidth + 'px';

		this._originalEditor.layout({ width: originalWidth, height: height });
		this._modifiedEditor.layout({
			width: width - originalWidth -
				(this._renderOverviewRuler.read(reader) ? OverviewRulerPart.ENTIRE_DIFF_OVERVIEW_WIDTH : 0),
			height
		});
		this._reviewPane.layout(0, width, height);

		return {
			modifiedEditor: this._modifiedEditor.getLayoutInfo(),
			originalEditor: this._originalEditor.getLayoutInfo(),
		};
	});

	private _createDiffEditorContributions() {
		const contributions: IDiffEditorContributionDescription[] = EditorExtensionsRegistry.getDiffEditorContributions();
		for (const desc of contributions) {
			try {
				this._register(this._instantiationService.createInstance(desc.ctor, this));
			} catch (err) {
				onUnexpectedError(err);
			}
		}
	}

	private _createLeftHandSideEditor(options: Readonly<IDiffEditorConstructionOptions>, codeEditorWidgetOptions: ICodeEditorWidgetOptions): CodeEditorWidget {
		const editor = this._constructInnerEditor(this._instantiationService, this.elements.original, this._adjustOptionsForLeftHandSide(options), codeEditorWidgetOptions);
		const isInDiffLeftEditorKey = this._contextKeyService.createKey<boolean>('isInDiffLeftEditor', editor.hasWidgetFocus());
		this._register(editor.onDidFocusEditorWidget(() => isInDiffLeftEditorKey.set(true)));
		this._register(editor.onDidBlurEditorWidget(() => isInDiffLeftEditorKey.set(false)));
		this._register(editor.onDidChangeCursorPosition(e => {
			const m = this._diffModel.get();
			if (!m) { return; }

			const movedText = m.diff.get()!.movedTexts.find(m => m.lineRangeMapping.originalRange.contains(e.position.lineNumber));
			m.syncedMovedTexts.set(movedText, undefined);
		}));
		return editor;
	}

	private _createRightHandSideEditor(options: Readonly<IDiffEditorConstructionOptions>, codeEditorWidgetOptions: ICodeEditorWidgetOptions): CodeEditorWidget {
		const editor = this._constructInnerEditor(this._instantiationService, this.elements.modified, this._adjustOptionsForRightHandSide(options), codeEditorWidgetOptions);
		const isInDiffRightEditorKey = this._contextKeyService.createKey<boolean>('isInDiffRightEditor', editor.hasWidgetFocus());
		this._register(editor.onDidFocusEditorWidget(() => isInDiffRightEditorKey.set(true)));
		this._register(editor.onDidBlurEditorWidget(() => isInDiffRightEditorKey.set(false)));
		this._register(editor.onDidChangeCursorPosition(e => {
			const m = this._diffModel.get();
			if (!m) { return; }

			const movedText = m.diff.get()!.movedTexts.find(m => m.lineRangeMapping.modifiedRange.contains(e.position.lineNumber));
			m.syncedMovedTexts.set(movedText, undefined);
		}));
		// Revert change when an arrow is clicked.
		this._register(editor.onMouseDown(event => {
			if (!event.event.rightButton && event.target.position && event.target.element?.className.includes('arrow-revert-change')) {
				const lineNumber = event.target.position.lineNumber;
				const viewZone = event.target as IMouseTargetViewZone | undefined;

				const model = this._diffModel.get();
				if (!model) {
					return;
				}
				const diffs = model.diff.get()?.mappings;
				if (!diffs) {
					return;
				}
				const diff = diffs.find(d =>
					viewZone?.detail.afterLineNumber === d.lineRangeMapping.modifiedRange.startLineNumber - 1 ||
					d.lineRangeMapping.modifiedRange.startLineNumber === lineNumber
				);
				if (!diff) {
					return;
				}
				this.revert(diff.lineRangeMapping);

				event.event.stopPropagation();
				return;
			}
		}));

		return editor;
	}

	protected _constructInnerEditor(instantiationService: IInstantiationService, container: HTMLElement, options: Readonly<IEditorConstructionOptions>, editorWidgetOptions: ICodeEditorWidgetOptions): CodeEditorWidget {
		const editor = this._createInnerEditor(instantiationService, container, options, editorWidgetOptions);

		this._register(editor.onDidContentSizeChange(e => {
			const width = this._originalEditor.getContentWidth() + this._modifiedEditor.getContentWidth() + OverviewRulerPart.ENTIRE_DIFF_OVERVIEW_WIDTH;
			const height = Math.max(this._modifiedEditor.getContentHeight(), this._originalEditor.getContentHeight());

			this._onDidContentSizeChange.fire({
				contentHeight: height,
				contentWidth: width,
				contentHeightChanged: e.contentHeightChanged,
				contentWidthChanged: e.contentWidthChanged
			});
		}));
		return editor;
	}

	protected _createInnerEditor(instantiationService: IInstantiationService, container: HTMLElement, options: Readonly<IEditorConstructionOptions>, editorWidgetOptions: ICodeEditorWidgetOptions): CodeEditorWidget {
		const editor = instantiationService.createInstance(CodeEditorWidget, container, options, editorWidgetOptions);
		return editor;
	}

	private _adjustOptionsForLeftHandSide(options: Readonly<IDiffEditorConstructionOptions>): IEditorConstructionOptions {
		const result = this._adjustOptionsForSubEditor(options);
		if (!this._options.get().renderSideBySide) {
			// never wrap hidden editor
			result.wordWrapOverride1 = 'off';
			result.wordWrapOverride2 = 'off';
			result.stickyScroll = { enabled: false };
		} else {
			result.wordWrapOverride1 = this._options.get().diffWordWrap;
		}
		if (options.originalAriaLabel) {
			result.ariaLabel = options.originalAriaLabel;
		}
		result.ariaLabel = this._updateAriaLabel(result.ariaLabel);
		result.readOnly = !this._options.get().originalEditable;
		result.dropIntoEditor = { enabled: !result.readOnly };
		result.extraEditorClassName = 'original-in-monaco-diff-editor';
		return result;
	}

	private _adjustOptionsForRightHandSide(options: Readonly<IDiffEditorConstructionOptions>): IEditorConstructionOptions {
		const result = this._adjustOptionsForSubEditor(options);
		if (options.modifiedAriaLabel) {
			result.ariaLabel = options.modifiedAriaLabel;
		}
		result.ariaLabel = this._updateAriaLabel(result.ariaLabel);
		result.wordWrapOverride1 = this._options.get().diffWordWrap;
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
		clonedOptions.codeLens = this._options.get().diffCodeLens;
		clonedOptions.fixedOverflowWidgets = true;
		// clonedOptions.lineDecorationsWidth = '2ch';
		// Clone minimap options before changing them
		clonedOptions.minimap = { ...(clonedOptions.minimap || {}) };
		clonedOptions.minimap.enabled = false;

		if (this._options.get().experimental?.collapseUnchangedRegions) {
			clonedOptions.stickyScroll = { enabled: false };
		} else {
			clonedOptions.stickyScroll = this._editorOptions.stickyScroll;
		}
		return clonedOptions;
	}

	private _updateAriaLabel(ariaLabel: string | undefined): string | undefined {
		const ariaNavigationTip = localize('diff-aria-navigation-tip', ' use Shift + F7 to navigate changes');
		if (this._options.get().accessibilityVerbose) {
			return ariaLabel + ariaNavigationTip;
		} else if (ariaLabel) {
			return ariaLabel.replaceAll(ariaNavigationTip, '');
		}
		return undefined;
	}

	protected override get _targetEditor(): CodeEditorWidget { return this._modifiedEditor; }

	override getEditorType(): string { return EditorType.IDiffEditor; }

	override onVisible(): void {
		// TODO: Only compute diffs when diff editor is visible
		this._originalEditor.onVisible();
		this._modifiedEditor.onVisible();
	}

	override onHide(): void {
		this._originalEditor.onHide();
		this._modifiedEditor.onHide();
	}

	override layout(dimension?: IDimension | undefined): void {
		this._rootSizeObserver.observe(dimension);
	}

	override hasTextFocus(): boolean {
		return this._originalEditor.hasTextFocus() || this._modifiedEditor.hasTextFocus();
	}

	public override saveViewState(): IDiffEditorViewState {
		const originalViewState = this._originalEditor.saveViewState();
		const modifiedViewState = this._modifiedEditor.saveViewState();
		return {
			original: originalViewState,
			modified: modifiedViewState
		};
	}

	public override restoreViewState(s: IDiffEditorViewState): void {
		if (s && s.original && s.modified) {
			const diffEditorState = s as IDiffEditorViewState;
			this._originalEditor.restoreViewState(diffEditorState.original);
			this._modifiedEditor.restoreViewState(diffEditorState.modified);
		}
	}

	public createViewModel(model: IDiffEditorModel): IDiffEditorViewModel {
		return new DiffModel(
			model,
			this._options.map(o => o.ignoreTrimWhitespace),
			this._options.map(o => o.maxComputationTime),
			this._options.map(o => o.experimental.collapseUnchangedRegions!),
			this._options.map(o => o.experimental.showMoves! && o.renderSideBySide),
			this._instantiationService.createInstance(WorkerBasedDocumentDiffProvider, this._options.get())
		);
	}

	override getModel(): IDiffEditorModel | null { return this._model.get(); }

	override setModel(model: IDiffEditorModel | null | IDiffEditorViewModel): void {
		const vm = model ? ('model' in model) ? model : this.createViewModel(model) : undefined;
		this._originalEditor.setModel(vm ? vm.model.original : null);
		this._modifiedEditor.setModel(vm ? vm.model.modified : null);
		transaction(tx => {
			this._model.set(vm?.model ?? null, tx);
			this._diffModel.set(vm as (DiffModel | undefined), tx);
		});
	}

	override updateOptions(_newOptions: IDiffEditorOptions): void {
		const newOptions = validateDiffEditorOptions(_newOptions, this._options.get());
		this._options.set(newOptions, undefined);
		deepMerge(this._editorOptions, deepClone(_newOptions));

		this._modifiedEditor.updateOptions(this._adjustOptionsForRightHandSide(_newOptions));
		this._originalEditor.updateOptions(this._adjustOptionsForLeftHandSide(_newOptions));
	}

	getContainerDomNode(): HTMLElement { return this._domElement; }
	getOriginalEditor(): ICodeEditor { return this._originalEditor; }
	getModifiedEditor(): ICodeEditor { return this._modifiedEditor; }

	setBoundarySashes(sashes: IBoundarySashes): void {
		this._boundarySashes.set(sashes, undefined);
	}

	private readonly _diffValue = this._diffModel.map((m, r) => m?.diff.read(r));
	readonly onDidUpdateDiff: Event<void> = Event.fromObservableLight(this._diffValue);

	get ignoreTrimWhitespace(): boolean {
		return this._options.get().ignoreTrimWhitespace;
	}

	get maxComputationTime(): number {
		return this._options.get().maxComputationTime;
	}

	get renderSideBySide(): boolean {
		return this._options.get().renderSideBySide;
	}

	/**
	 * @deprecated Use `this.getDiffComputationResult().changes2` instead.
	 */
	getLineChanges(): ILineChange[] | null {
		const diffState = this._diffModel.get()?.diff.get();
		if (!diffState) {
			return null;
		}
		return diffState.mappings.map(x => {
			const m = x.lineRangeMapping;
			let originalStartLineNumber: number;
			let originalEndLineNumber: number;
			let modifiedStartLineNumber: number;
			let modifiedEndLineNumber: number;
			let innerChanges = m.innerChanges;

			if (m.originalRange.isEmpty) {
				// Insertion
				originalStartLineNumber = m.originalRange.startLineNumber - 1;
				originalEndLineNumber = 0;
				innerChanges = undefined;
			} else {
				originalStartLineNumber = m.originalRange.startLineNumber;
				originalEndLineNumber = m.originalRange.endLineNumberExclusive - 1;
			}

			if (m.modifiedRange.isEmpty) {
				// Deletion
				modifiedStartLineNumber = m.modifiedRange.startLineNumber - 1;
				modifiedEndLineNumber = 0;
				innerChanges = undefined;
			} else {
				modifiedStartLineNumber = m.modifiedRange.startLineNumber;
				modifiedEndLineNumber = m.modifiedRange.endLineNumberExclusive - 1;
			}

			return {
				originalStartLineNumber,
				originalEndLineNumber,
				modifiedStartLineNumber,
				modifiedEndLineNumber,
				charChanges: innerChanges?.map(m => ({
					originalStartLineNumber: m.originalRange.startLineNumber,
					originalStartColumn: m.originalRange.startColumn,
					originalEndLineNumber: m.originalRange.endLineNumber,
					originalEndColumn: m.originalRange.endColumn,
					modifiedStartLineNumber: m.modifiedRange.startLineNumber,
					modifiedStartColumn: m.modifiedRange.startColumn,
					modifiedEndLineNumber: m.modifiedRange.endLineNumber,
					modifiedEndColumn: m.modifiedRange.endColumn,
				}))
			};
		});
	}

	getDiffComputationResult(): IDiffComputationResult | null {
		const diffState = this._diffModel.get()?.diff.get();
		if (!diffState) {
			return null;
		}

		return {
			changes: this.getLineChanges()!,
			changes2: diffState.mappings.map(m => m.lineRangeMapping),
			identical: diffState.identical,
			quitEarly: diffState.quitEarly,
		};
	}

	public revert(diff: LineRangeMapping): void {
		const model = this._model.get();
		if (!model) {
			return;
		}
		const originalText = model.original.getValueInRange(diff.originalRange.toExclusiveRange());
		this._modifiedEditor.executeEdits('diffEditor', [
			{ range: diff.modifiedRange.toExclusiveRange(), text: originalText }
		]);
	}

	private _goTo(diff: DiffMapping): void {
		this._modifiedEditor.setPosition(new Position(diff.lineRangeMapping.modifiedRange.startLineNumber, 1));
		this._modifiedEditor.revealRangeInCenter(diff.lineRangeMapping.modifiedRange.toExclusiveRange());
	}

	goToDiff(target: 'previous' | 'next'): void {
		const diffs = this._diffModel.get()?.diff.get()?.mappings;
		if (!diffs || diffs.length === 0) {
			return;
		}

		const curLineNumber = this._modifiedEditor.getPosition()!.lineNumber;

		let diff: DiffMapping | undefined;
		if (target === 'next') {
			diff = diffs.find(d => d.lineRangeMapping.modifiedRange.startLineNumber > curLineNumber) ?? diffs[0];
		} else {
			diff = findLast(diffs, d => d.lineRangeMapping.modifiedRange.startLineNumber < curLineNumber) ?? diffs[diffs.length - 1];
		}
		this._goTo(diff);
	}

	revealFirstDiff(): void {
		const diffModel = this._diffModel.get();
		if (!diffModel) {
			return;
		}
		// wait for the diff computation to finish
		this.waitForDiff().then(() => {
			const diffs = diffModel.diff.get()?.mappings;
			if (!diffs || diffs.length === 0) {
				return;
			}
			this._goTo(diffs[0]);
		});
	}

	public diffReviewNext(): void {
		this._reviewPane.next();
	}

	public diffReviewPrev(): void {
		this._reviewPane.prev();
	}

	public async waitForDiff(): Promise<void> {
		const diffModel = this._diffModel.get();
		if (!diffModel) {
			return;
		}
		await diffModel.waitForDiff();
	}
}

function validateDiffEditorOptions(options: Readonly<IDiffEditorOptions>, defaults: ValidDiffEditorBaseOptions): ValidDiffEditorBaseOptions {
	return {
		enableSplitViewResizing: validateBooleanOption(options.enableSplitViewResizing, defaults.enableSplitViewResizing),
		splitViewDefaultRatio: clampedFloat(options.splitViewDefaultRatio, 0.5, 0.1, 0.9),
		renderSideBySide: validateBooleanOption(options.renderSideBySide, defaults.renderSideBySide),
		renderMarginRevertIcon: validateBooleanOption(options.renderMarginRevertIcon, defaults.renderMarginRevertIcon),
		maxComputationTime: clampedInt(options.maxComputationTime, defaults.maxComputationTime, 0, Constants.MAX_SAFE_SMALL_INTEGER),
		maxFileSize: clampedInt(options.maxFileSize, defaults.maxFileSize, 0, Constants.MAX_SAFE_SMALL_INTEGER),
		ignoreTrimWhitespace: validateBooleanOption(options.ignoreTrimWhitespace, defaults.ignoreTrimWhitespace),
		renderIndicators: validateBooleanOption(options.renderIndicators, defaults.renderIndicators),
		originalEditable: validateBooleanOption(options.originalEditable, defaults.originalEditable),
		diffCodeLens: validateBooleanOption(options.diffCodeLens, defaults.diffCodeLens),
		renderOverviewRuler: validateBooleanOption(options.renderOverviewRuler, defaults.renderOverviewRuler),
		diffWordWrap: validateStringSetOption<'off' | 'on' | 'inherit'>(options.diffWordWrap, defaults.diffWordWrap, ['off', 'on', 'inherit']),
		diffAlgorithm: validateStringSetOption(options.diffAlgorithm, defaults.diffAlgorithm, ['legacy', 'advanced'], { 'smart': 'legacy', 'experimental': 'advanced' }),
		accessibilityVerbose: validateBooleanOption(options.accessibilityVerbose, defaults.accessibilityVerbose),
		experimental: {
			collapseUnchangedRegions: validateBooleanOption(options.experimental?.collapseUnchangedRegions, defaults.experimental.collapseUnchangedRegions!),
			showMoves: validateBooleanOption(options.experimental?.showMoves, defaults.experimental.showMoves!),
		},
	};
}
