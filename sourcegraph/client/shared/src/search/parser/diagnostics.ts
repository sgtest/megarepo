import * as Monaco from 'monaco-editor'
import { Token, toMonacoRange } from './scanner'
import { validateFilter } from './filters'
import { SearchPatternType } from '../../graphql-operations'

/**
 * Returns the diagnostics for a scanned search query to be displayed in the Monaco query input.
 */
export function getDiagnostics(tokens: Token[], patternType: SearchPatternType): Monaco.editor.IMarkerData[] {
    const diagnostics: Monaco.editor.IMarkerData[] = []
    for (const token of tokens) {
        if (token.type === 'filter') {
            const { filterType, filterValue } = token
            const validationResult = validateFilter(filterType.value, filterValue)
            if (validationResult.valid) {
                continue
            }
            diagnostics.push({
                severity: Monaco.MarkerSeverity.Error,
                message: validationResult.reason,
                ...toMonacoRange(filterType.range),
            })
        } else if (token.type === 'quoted') {
            if (patternType === SearchPatternType.literal) {
                diagnostics.push({
                    severity: Monaco.MarkerSeverity.Warning,
                    message:
                        'Your search is interpreted literally and contains quotes. Did you mean to search for quotes?',
                    ...toMonacoRange(token.range),
                })
            }
        }
    }
    return diagnostics
}
