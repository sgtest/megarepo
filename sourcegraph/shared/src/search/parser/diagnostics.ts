import * as Monaco from 'monaco-editor'
import { Sequence, toMonacoRange } from './parser'
import { validateFilter } from './filters'

/**
 * Returns the diagnostics for a parsed search query to be displayed in the Monaco query input.
 */
export function getDiagnostics({ members }: Pick<Sequence, 'members'>): Monaco.editor.IMarkerData[] {
    const diagnostics: Monaco.editor.IMarkerData[] = []
    for (const { token, range } of members) {
        if (token.type === 'filter') {
            const { filterType, filterValue } = token
            const validationResult = validateFilter(filterType.token.value, filterValue)
            if (validationResult.valid) {
                continue
            }
            diagnostics.push({
                severity: Monaco.MarkerSeverity.Error,
                message: validationResult.reason,
                ...toMonacoRange(filterType.range),
            })
        } else if (token.type === 'literal') {
            if (token.value.includes(':')) {
                diagnostics.push({
                    severity: Monaco.MarkerSeverity.Warning,
                    message: 'Quoting the query may help if you want a literal match.',
                    ...toMonacoRange(range),
                })
            }
        }
    }
    return diagnostics
}
