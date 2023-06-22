/*---------------------------------------------------------------------------------------------
 *  Copyright (c) Microsoft Corporation. All rights reserved.
 *  Licensed under the MIT License. See License.txt in the project root for license information.
 *--------------------------------------------------------------------------------------------*/

import { Disposable } from 'vs/base/common/lifecycle';
import { IObservable, derived } from 'vs/base/common/observable';
import { isDefined } from 'vs/base/common/types';
import { CodeEditorWidget } from 'vs/editor/browser/widget/codeEditorWidget';
import { arrowRevertChange, diffAddDecoration, diffAddDecorationEmpty, diffDeleteDecoration, diffDeleteDecorationEmpty, diffLineAddDecorationBackground, diffLineDeleteDecorationBackground } from 'vs/editor/browser/widget/diffEditorWidget2/decorations';
import { DiffModel } from 'vs/editor/browser/widget/diffEditorWidget2/diffModel';
import { MovedBlocksLinesPart } from 'vs/editor/browser/widget/diffEditorWidget2/movedBlocksLines';
import { applyObservableDecorations } from 'vs/editor/browser/widget/diffEditorWidget2/utils';
import { LineRange } from 'vs/editor/common/core/lineRange';
import { Position } from 'vs/editor/common/core/position';
import { Range } from 'vs/editor/common/core/range';
import { IModelDeltaDecoration } from 'vs/editor/common/model';

export class DiffEditorDecorations extends Disposable {
	constructor(
		private readonly _originalEditor: CodeEditorWidget,
		private readonly _modifiedEditor: CodeEditorWidget,
		private readonly _diffModel: IObservable<DiffModel | undefined>,
		private readonly _renderSideBySide: IObservable<boolean>,
	) {
		super();

		this._register(applyObservableDecorations(this._originalEditor, this._decorations.map(d => d?.originalDecorations || [])));
		this._register(applyObservableDecorations(this._modifiedEditor, this._decorations.map(d => d?.modifiedDecorations || [])));
	}


	private readonly _decorations = derived('decorations', (reader) => {
		const diff = this._diffModel.read(reader)?.diff.read(reader);
		if (!diff) {
			return null;
		}

		const currentMove = this._diffModel.read(reader)!.syncedMovedTexts.read(reader);

		const originalDecorations: IModelDeltaDecoration[] = [];
		const modifiedDecorations: IModelDeltaDecoration[] = [];
		for (const m of diff.mappings) {
			const fullRangeOriginal = LineRange.subtract(m.lineRangeMapping.originalRange, currentMove?.lineRangeMapping.originalRange)
				.map(i => i.toInclusiveRange()).filter(isDefined);
			for (const range of fullRangeOriginal) {
				originalDecorations.push({ range, options: diffLineDeleteDecorationBackground });
			}

			const fullRangeModified = LineRange.subtract(m.lineRangeMapping.modifiedRange, currentMove?.lineRangeMapping.modifiedRange)
				.map(i => i.toInclusiveRange()).filter(isDefined);
			for (const range of fullRangeModified) {
				modifiedDecorations.push({ range, options: diffLineAddDecorationBackground });
			}

			for (const i of m.lineRangeMapping.innerChanges || []) {
				if (currentMove
					&& (currentMove.lineRangeMapping.originalRange.intersect(new LineRange(i.originalRange.startLineNumber, i.originalRange.endLineNumber))
						|| currentMove.lineRangeMapping.modifiedRange.intersect(new LineRange(i.modifiedRange.startLineNumber, i.modifiedRange.endLineNumber)))) {
					continue;
				}

				originalDecorations.push({ range: i.originalRange, options: i.originalRange.isEmpty() ? diffDeleteDecorationEmpty : diffDeleteDecoration });
				modifiedDecorations.push({ range: i.modifiedRange, options: i.modifiedRange.isEmpty() ? diffAddDecorationEmpty : diffAddDecoration });
			}

			if (!m.lineRangeMapping.modifiedRange.isEmpty && this._renderSideBySide.read(reader) && !currentMove) {
				modifiedDecorations.push({ range: Range.fromPositions(new Position(m.lineRangeMapping.modifiedRange.startLineNumber, 1)), options: arrowRevertChange });
			}
		}

		if (currentMove) {
			for (const m of currentMove.changes) {
				const fullRangeOriginal = m.originalRange.toInclusiveRange();
				if (fullRangeOriginal) {
					originalDecorations.push({ range: fullRangeOriginal, options: diffLineDeleteDecorationBackground });
				}
				const fullRangeModified = m.modifiedRange.toInclusiveRange();
				if (fullRangeModified) {
					modifiedDecorations.push({ range: fullRangeModified, options: diffLineAddDecorationBackground });
				}

				for (const i of m.innerChanges || []) {
					originalDecorations.push({ range: i.originalRange, options: diffDeleteDecoration });
					modifiedDecorations.push({ range: i.modifiedRange, options: diffAddDecoration });
				}
			}
		}

		for (const m of diff.movedTexts) {
			originalDecorations.push({
				range: m.lineRangeMapping.originalRange.toInclusiveRange()!, options: {
					description: 'moved',
					blockClassName: 'movedOriginal',
					blockPadding: [MovedBlocksLinesPart.movedCodeBlockPadding, 0, MovedBlocksLinesPart.movedCodeBlockPadding, MovedBlocksLinesPart.movedCodeBlockPadding],
				}
			});

			modifiedDecorations.push({
				range: m.lineRangeMapping.modifiedRange.toInclusiveRange()!, options: {
					description: 'moved',
					blockClassName: 'movedModified',
					blockPadding: [4, 0, 4, 4],
				}
			});
		}

		return { originalDecorations, modifiedDecorations };
	});
}
