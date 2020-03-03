import { DiffPart } from '@sourcegraph/codeintellify'
import { DOMFunctions } from '../code_intelligence/code_views'

const getSingleFileLineElementFromLineNumber = (codeView: HTMLElement, line: number): HTMLElement => {
    const lineNumElem = codeView.querySelector<HTMLElement>(`[data-line-number="${line}"]`)
    if (!lineNumElem) {
        throw new Error(`Line ${line} not found in code view`)
    }

    const lineElem = lineNumElem.closest<HTMLElement>('.line')
    if (!lineElem) {
        throw new Error('Could not find line elem for line element')
    }

    return lineElem
}

export const singleFileDOMFunctions: DOMFunctions = {
    getCodeElementFromTarget: target => {
        const container = target.closest('.CodeMirror-line')

        return container ? container.querySelector<HTMLElement>('span[role="presentation"]') : null
    },
    getLineNumberFromCodeElement: codeElement => {
        const line = codeElement.closest('.line')
        if (!line) {
            throw new Error('Could not find line containing code element')
        }

        const lineNumElem = line.querySelector<HTMLElement>('.line-locator')
        if (!lineNumElem) {
            throw new Error('Could not find the line number in a line container')
        }

        const lineNum = parseInt(lineNumElem.dataset.lineNumber || '', 10)
        if (isNaN(lineNum)) {
            throw new Error('data-line-number not set on line number element')
        }

        return lineNum
    },
    getLineElementFromLineNumber: getSingleFileLineElementFromLineNumber,
    getCodeElementFromLineNumber: (codeView, line) =>
        getSingleFileLineElementFromLineNumber(codeView, line).querySelector<HTMLElement>(
            '.CodeMirror-line span[role="presentation"]'
        ),
}

const getDiffLineElementFromLineNumber = (codeView: HTMLElement, line: number, part?: DiffPart): HTMLElement => {
    for (const lineNumElem of codeView.getElementsByClassName(`line-number-${part === 'head' ? 'to' : 'from'}`)) {
        const lineNum = parseInt((lineNumElem.textContent || '').trim(), 10)
        if (!isNaN(lineNum) && lineNum === line) {
            const lineElem = lineNumElem.closest<HTMLElement>('.line')
            if (!lineElem) {
                throw new Error('Could not find lineElem from lineNumElem')
            }

            return lineElem
        }
    }

    throw new Error(`Could not locate line number element for line ${line}, part: ${String(part)}`)
}

export const diffDOMFunctions: DOMFunctions = {
    getCodeElementFromTarget: singleFileDOMFunctions.getCodeElementFromTarget,
    getLineNumberFromCodeElement: codeElement => {
        const line = codeElement.closest('.line')
        if (!line) {
            throw new Error('Could not find line containing code element')
        }

        const lineNumTo = line.querySelector<HTMLElement>('.line-number-to')
        if (lineNumTo) {
            const lineNum = parseInt((lineNumTo.textContent || '').trim(), 10)
            if (!isNaN(lineNum)) {
                return lineNum
            }
        }

        const lineNumFrom = line.querySelector<HTMLElement>('.line-number-from')
        if (lineNumFrom) {
            const lineNum = parseInt((lineNumFrom.textContent || '').trim(), 10)
            if (!isNaN(lineNum)) {
                return lineNum
            }
        }

        throw new Error('Could not find line number element for code element')
    },
    getLineElementFromLineNumber: getDiffLineElementFromLineNumber,
    getCodeElementFromLineNumber: (codeView, line, part) =>
        getDiffLineElementFromLineNumber(codeView, line, part).querySelector<HTMLElement>(
            '.CodeMirror-line span[role="presentation"]'
        ),
    getDiffCodePart: codeElement => {
        if (!document.querySelector('.side-by-side-diff')) {
            return codeElement.closest('.line')!.classList.contains('removed') ? 'base' : 'head'
        }

        const diffSide = codeElement.closest('.diff-editor')!

        return diffSide.previousElementSibling &&
            // If the sibling to the left is the diff divider, it's in the HEAD.
            diffSide.previousElementSibling.classList.contains('segment-connector-column')
            ? 'head'
            : 'base'
    },
    isFirstCharacterDiffIndicator: () => false,
}
