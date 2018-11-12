import { parse, ParseError, ParseErrorCode } from '@sqs/jsonc-parser/lib/main'
import { asError, createAggregateError, ErrorLike } from './errors'

/**
 * Parses the JSON input using an error-tolerant "JSONC" parser. If an error occurs, it is returned as a value
 * instead of being thrown. This is useful when input is parsed in the background (not in response to any specific
 * user action), because it makes it easier to save the error and only show it to the user when it matters (for
 * some interactive user action).
 */
export function parseJSONCOrError<T>(input: string): T | ErrorLike {
    try {
        return parseJSON(input) as T
    } catch (err) {
        return asError(err)
    }
}

/**
 * Parses the JSON input using an error-tolerant "JSONC" parser.
 */
function parseJSON(input: string): any {
    const errors: ParseError[] = []
    const o = parse(input, errors, { allowTrailingComma: true, disallowComments: false })
    if (errors.length > 0) {
        throw createAggregateError(
            errors.map(v => ({
                ...v,
                code: ParseErrorCode[v.error],
                message: `Configuration parse error, code: ${v.error} (offset: ${v.offset}, length: ${v.length})`,
            }))
        )
    }
    return o
}
