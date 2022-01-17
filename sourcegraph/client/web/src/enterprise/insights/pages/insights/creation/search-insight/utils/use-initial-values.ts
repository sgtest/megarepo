import { useLocation } from 'react-router-dom'

import { isErrorLike } from '@sourcegraph/common'
import { useLocalStorage } from '@sourcegraph/wildcard'

import { CreateInsightFormFields } from '../types'

import { useURLQueryInsight } from './use-url-query-insight/use-url-query-insight'

export interface UseInitialValuesResult {
    initialValues: Partial<CreateInsightFormFields>
    loading: boolean
    setLocalStorageFormValues: (values: CreateInsightFormFields | undefined) => void
}

export function useSearchInsightInitialValues(): UseInitialValuesResult {
    const { search } = useLocation()

    // Search insight creation UI form can take value from query param in order
    // to support 1-click insight creation from search result page.
    const { hasQueryInsight, data: urlQueryInsightValues } = useURLQueryInsight(search)

    // Creation UI saves all form values in local storage to be able restore these
    // values if page was fully refreshed or user came back from other page.
    const [localStorageFormValues, setLocalStorageFormValues] = useLocalStorage<CreateInsightFormFields | undefined>(
        'insights.search-insight-creation-ui',
        undefined
    )

    if (hasQueryInsight) {
        return {
            initialValues: !isErrorLike(urlQueryInsightValues) ? urlQueryInsightValues ?? {} : {},
            loading: urlQueryInsightValues === undefined,
            setLocalStorageFormValues,
        }
    }

    return {
        initialValues: localStorageFormValues ?? {},
        loading: false,
        setLocalStorageFormValues,
    }
}
