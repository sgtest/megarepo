import type * as H from 'history'

import { findLineKeyInSearchParameters } from '@sourcegraph/common'
import type { RenderMode } from '@sourcegraph/shared/src/util/url'

const URL_QUERY_PARAM = 'view'

/**
 * Reports whether the location's URL displays the blob as rendered or source.
 */
export const getModeFromURL = (location: H.Location): RenderMode => {
    const searchParameters = new URLSearchParams(location.search)

    if (!searchParameters.has(URL_QUERY_PARAM)) {
        return undefined
    }
    return searchParameters.get(URL_QUERY_PARAM) === 'code' ? 'code' : 'rendered' // default to rendered
}

/**
 * Returns the URL that displays the blob using the specified mode.
 */
export const getURLForMode = (location: H.Location, mode: RenderMode): H.Location => {
    const searchParameters = new URLSearchParams(location.search)

    if (mode === 'code') {
        searchParameters.set(URL_QUERY_PARAM, mode)
    } else {
        // We remove any existing line ranges as they are not supported in rendered mode.
        const existingLineRangeKey = findLineKeyInSearchParameters(searchParameters)
        if (existingLineRangeKey) {
            searchParameters.delete(existingLineRangeKey)
        }

        searchParameters.delete(URL_QUERY_PARAM)
    }

    return { ...location, search: searchParameters.toString() }
}
