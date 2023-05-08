import { useEffect, useRef, useState } from 'react'

import { QueryResult } from '@apollo/client'

import { dataOrThrowErrors, useLazyQuery, useQuery } from '@sourcegraph/http-client'

import { Location, buildPreciseLocation } from '../../codeintel/location'
import {
    LOAD_ADDITIONAL_IMPLEMENTATIONS_QUERY,
    LOAD_ADDITIONAL_PROTOTYPES_QUERY,
    LOAD_ADDITIONAL_REFERENCES_QUERY,
    USE_PRECISE_CODE_INTEL_FOR_POSITION_QUERY,
} from '../../codeintel/ReferencesPanelQueries'
import { CodeIntelData, UseCodeIntelParameters, UseCodeIntelResult } from '../../codeintel/useCodeIntel'
import { ConnectionQueryArguments } from '../../components/FilteredConnection'
import { asGraphQLResult } from '../../components/FilteredConnection/utils'
import {
    UsePreciseCodeIntelForPositionVariables,
    UsePreciseCodeIntelForPositionResult,
    LoadAdditionalReferencesResult,
    LoadAdditionalReferencesVariables,
    LoadAdditionalImplementationsResult,
    LoadAdditionalImplementationsVariables,
    LoadAdditionalPrototypesResult,
    LoadAdditionalPrototypesVariables,
} from '../../graphql-operations'

import { useSearchBasedCodeIntel } from './useSearchBasedCodeIntel'

const EMPTY_CODE_INTEL_DATA = {
    implementations: { endCursor: null, nodes: [] },
    prototypes: { endCursor: null, nodes: [] },
    definitions: { endCursor: null, nodes: [] },
    references: { endCursor: null, nodes: [] },
}

export const useCodeIntel = ({
    variables,
    searchToken,
    spec,
    fileContent,
    isFork,
    isArchived,
    getSetting,
}: UseCodeIntelParameters): UseCodeIntelResult => {
    const shouldMixPreciseAndSearchBasedReferences = (): boolean =>
        getSetting<boolean>('codeIntel.mixPreciseAndSearchBasedReferences', false)

    const [codeIntelData, setCodeIntelData] = useState<CodeIntelData>()

    const setReferences = (references: Location[]): void => {
        setCodeIntelData(previousData => ({
            ...(previousData || EMPTY_CODE_INTEL_DATA),
            references: {
                endCursor: null,
                nodes: references,
            },
        }))
    }

    const deduplicateAndAddReferences = (searchBasedReferences: Location[]): void => {
        setCodeIntelData(previousData => {
            const previous = previousData || EMPTY_CODE_INTEL_DATA

            const lsifFiles = new Set(previous.references.nodes.map(location => location.file))

            // Filter out any search results that occur in the same file as LSIF results. These
            // results are definitely incorrect and will pollute the ordering of precise and fuzzy
            // results in the references pane.
            const searchResults = searchBasedReferences.filter(location => !lsifFiles.has(location.file))
            if (searchResults.length === 0) {
                return previous
            }

            return {
                ...previous,
                references: {
                    endCursor: previous.references.endCursor,
                    nodes: [...previous.references.nodes, ...searchResults],
                },
            }
        })
    }

    const setDefinitions = (definitions: Location[]): void => {
        setCodeIntelData(previousData => ({
            ...(previousData || EMPTY_CODE_INTEL_DATA),
            definitions: {
                endCursor: null,
                nodes: definitions,
            },
        }))
    }

    const fellBackToSearchBased = useRef(false)
    const shouldFetchPrecise = useRef(true)
    useEffect(() => {
        // We need to fetch again if the variables change
        shouldFetchPrecise.current = true
        fellBackToSearchBased.current = false
    }, [
        variables.repository,
        variables.commit,
        variables.path,
        variables.line,
        variables.character,
        variables.filter,
        variables.firstReferences,
        variables.firstImplementations,
    ])

    const {
        loading: searchBasedLoading,
        error: searchBasedError,
        fetch: fetchSearchBasedCodeIntel,
        fetchReferences: fetchSearchBasedReferences,
        fetchDefinitions: fetchSearchBasedDefinitions,
    } = useSearchBasedCodeIntel({
        repo: variables.repository,
        commit: variables.commit,
        path: variables.path,
        filter: variables.filter ?? undefined,
        searchToken,
        position: {
            line: variables.line,
            character: variables.character,
        },
        fileContent,
        spec,
        isFork,
        isArchived,
        getSetting,
    })

    const { error, loading } = useQuery<
        UsePreciseCodeIntelForPositionResult,
        UsePreciseCodeIntelForPositionVariables & ConnectionQueryArguments
    >(USE_PRECISE_CODE_INTEL_FOR_POSITION_QUERY, {
        variables,
        notifyOnNetworkStatusChange: false,
        fetchPolicy: 'no-cache',
        onCompleted: result => {
            if (shouldFetchPrecise.current) {
                shouldFetchPrecise.current = false

                const lsifData = result ? getLsifData({ data: result }) : undefined
                if (lsifData) {
                    setCodeIntelData(lsifData)

                    // If we've exhausted LSIF data and the flag is enabled, we add search-based data.
                    if (lsifData.references.endCursor === null && shouldMixPreciseAndSearchBasedReferences()) {
                        fetchSearchBasedReferences(deduplicateAndAddReferences)
                    }

                    // When no definitions are found, the hover tooltip falls back to a search based
                    // search, regardless of the mixPreciseAndSearchBasedReferences setting.
                    if (lsifData.definitions.nodes.length === 0) {
                        fetchSearchBasedDefinitions(setDefinitions)
                    }
                } else {
                    fellBackToSearchBased.current = true

                    fetchSearchBasedCodeIntel(setReferences, setDefinitions)
                }
            }
        },
    })

    const [fetchAdditionalReferences, additionalReferencesResult] = useLazyQuery<
        LoadAdditionalReferencesResult,
        LoadAdditionalReferencesVariables & ConnectionQueryArguments
    >(LOAD_ADDITIONAL_REFERENCES_QUERY, {
        fetchPolicy: 'no-cache',
        onCompleted: result => {
            const previousData = codeIntelData

            const newReferenceData = result.repository?.commit?.blob?.lsif?.references

            if (!previousData || !newReferenceData) {
                return
            }

            setCodeIntelData({
                implementations: previousData.implementations,
                prototypes: previousData.prototypes,
                definitions: previousData.definitions,
                references: {
                    endCursor: newReferenceData.pageInfo.endCursor,
                    nodes: dedupeLocations([
                        ...previousData.references.nodes,
                        ...newReferenceData.nodes.map(buildPreciseLocation),
                    ]),
                },
            })

            // If we've exhausted LSIF data and the flag is enabled, we add search-based data.
            if (newReferenceData.pageInfo.endCursor === null && shouldMixPreciseAndSearchBasedReferences()) {
                fetchSearchBasedReferences(deduplicateAndAddReferences)
            }
        },
    })

    const [fetchAdditionalPrototypes, additionalPrototypesResult] = useLazyQuery<
        LoadAdditionalPrototypesResult,
        LoadAdditionalPrototypesVariables & ConnectionQueryArguments
    >(LOAD_ADDITIONAL_PROTOTYPES_QUERY, {
        fetchPolicy: 'no-cache',
        onCompleted: result => {
            const previousData = codeIntelData

            const newPrototypesData = result.repository?.commit?.blob?.lsif?.prototypes

            if (!previousData || !newPrototypesData) {
                return
            }

            setCodeIntelData({
                references: previousData.references,
                definitions: previousData.definitions,
                implementations: previousData.implementations,
                prototypes: {
                    endCursor: newPrototypesData.pageInfo.endCursor,
                    nodes: dedupeLocations([
                        ...previousData.prototypes.nodes,
                        ...newPrototypesData.nodes.map(buildPreciseLocation),
                    ]),
                },
            })
        },
    })

    const [fetchAdditionalImplementations, additionalImplementationsResult] = useLazyQuery<
        LoadAdditionalImplementationsResult,
        LoadAdditionalImplementationsVariables & ConnectionQueryArguments
    >(LOAD_ADDITIONAL_IMPLEMENTATIONS_QUERY, {
        fetchPolicy: 'no-cache',
        onCompleted: result => {
            const previousData = codeIntelData

            const newImplementationsData = result.repository?.commit?.blob?.lsif?.implementations

            if (!previousData || !newImplementationsData) {
                return
            }

            setCodeIntelData({
                references: previousData.references,
                definitions: previousData.definitions,
                prototypes: previousData.prototypes,
                implementations: {
                    endCursor: newImplementationsData.pageInfo.endCursor,
                    nodes: dedupeLocations([
                        ...previousData.implementations.nodes,
                        ...newImplementationsData.nodes.map(buildPreciseLocation),
                    ]),
                },
            })
        },
    })

    const fetchMoreReferences = (): void => {
        const cursor = codeIntelData?.references.endCursor || null

        if (cursor !== null) {
            // eslint-disable-next-line @typescript-eslint/no-floating-promises
            fetchAdditionalReferences({
                variables: {
                    ...variables,
                    ...{ afterReferences: cursor },
                },
            })
        }
    }

    const fetchMoreImplementations = (): void => {
        const cursor = codeIntelData?.implementations.endCursor || null

        if (cursor !== null) {
            // eslint-disable-next-line @typescript-eslint/no-floating-promises
            fetchAdditionalImplementations({
                variables: {
                    ...variables,
                    ...{ afterImplementations: cursor },
                },
            })
        }
    }

    const fetchMorePrototypes = (): void => {
        const cursor = codeIntelData?.prototypes.endCursor || null

        if (cursor !== null) {
            // eslint-disable-next-line @typescript-eslint/no-floating-promises
            fetchAdditionalPrototypes({
                variables: {
                    ...variables,
                    ...{ afterPrototypes: cursor },
                },
            })
        }
    }

    const combinedLoading = loading || (fellBackToSearchBased.current && searchBasedLoading)

    const combinedError = error || searchBasedError

    return {
        data: codeIntelData,
        loading: combinedLoading,

        error: combinedError,

        fetchMoreReferences,
        fetchMoreReferencesLoading: additionalReferencesResult.loading,
        referencesHasNextPage: codeIntelData ? codeIntelData.references.endCursor !== null : false,

        fetchMoreImplementations,
        implementationsHasNextPage: codeIntelData ? codeIntelData.implementations.endCursor !== null : false,
        fetchMoreImplementationsLoading: additionalImplementationsResult.loading,

        fetchMorePrototypes,
        prototypesHasNextPage: codeIntelData ? codeIntelData.prototypes.endCursor !== null : false,
        fetchMorePrototypesLoading: additionalPrototypesResult.loading,
    }
}

const getLsifData = ({
    data,
    error,
}: Pick<QueryResult<UsePreciseCodeIntelForPositionResult>, 'data' | 'error'>): CodeIntelData | undefined => {
    const result = asGraphQLResult({ data, errors: error?.graphQLErrors || [] })

    const extractedData = dataOrThrowErrors(result)

    // If there weren't any errors and we just didn't receive any data
    if (!extractedData?.repository?.commit?.blob?.lsif) {
        return undefined
    }

    const lsif = extractedData.repository?.commit?.blob?.lsif

    return {
        implementations: {
            endCursor: lsif.implementations.pageInfo.endCursor,
            nodes: dedupeLocations(lsif.implementations.nodes).map(buildPreciseLocation),
        },
        prototypes: {
            endCursor: lsif.prototypes.pageInfo.endCursor,
            nodes: dedupeLocations(lsif.prototypes.nodes).map(buildPreciseLocation),
        },
        references: {
            endCursor: lsif.references.pageInfo.endCursor,
            nodes: dedupeLocations(lsif.references.nodes).map(buildPreciseLocation),
        },
        definitions: {
            endCursor: lsif.definitions.pageInfo.endCursor,
            nodes: lsif.definitions.nodes.map(buildPreciseLocation),
        },
    }
}

const dedupeLocations = <L extends { url: string }>(locations: L[]): L[] => {
    const deduped = []
    const seenURLs = new Set<string>()
    for (const location of locations) {
        if (!seenURLs.has(location.url)) {
            deduped.push(location)
            seenURLs.add(location.url)
        }
    }
    return deduped
}
