import { gunzip, gzip } from 'mz/zlib'

/**
 * Return the gzipped JSON representation of `value`.
 *
 * @param value The value to encode.
 */
export function gzipJSON<T>(value: T): Promise<Buffer> {
    return gzip(Buffer.from(dumpJSON(value)))
}

/**
 * Reverse the operation of `gzipJSON`.
 *
 * @param value The value to decode.
 */
export async function gunzipJSON<T>(value: Buffer): Promise<T> {
    return parseJSON((await gunzip(value)).toString())
}

/** The replacer used by dumpJSON to encode map and set values. */
export const jsonReplacer = <T>(key: string, value: T): { type: string; value: unknown } | T => {
    if (value instanceof Map) {
        return {
            type: 'map',
            value: [...value],
        }
    }

    if (value instanceof Set) {
        return {
            type: 'set',
            value: [...value],
        }
    }

    return value
}

/**
 * Return the JSON representation of `value`. This has special logic to
 * convert ES6 map and set structures into a JSON-representable value.
 * This method, along with `parseJSON` should be used over the raw methods
 * if the payload may contain maps.
 *
 * @param value The value to jsonify.
 */
export function dumpJSON<T>(value: T): string {
    return JSON.stringify(value, jsonReplacer)
}

/**
 * Parse the JSON representation of `value`. This has special logic to
 * unmarshal map and set objects as encoded by `dumpJSON`.
 *
 * @param value The value to unmarshal.
 */
export function parseJSON<T>(value: string): T {
    return JSON.parse(value, (_, oldValue) => {
        if (typeof oldValue === 'object' && oldValue !== null) {
            if (oldValue.type === 'map') {
                return new Map(oldValue.value)
            }

            if (oldValue.type === 'set') {
                return new Set(oldValue.value)
            }
        }

        return oldValue
    })
}
