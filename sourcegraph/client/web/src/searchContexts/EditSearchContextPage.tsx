import React, { useCallback, useMemo } from 'react'
import { RouteComponentProps } from 'react-router'
import { Observable, of, throwError } from 'rxjs'
import { catchError, startWith, switchMap } from 'rxjs/operators'

import { isErrorLike } from '@sourcegraph/codeintellify/lib/errors'
import { LoadingSpinner } from '@sourcegraph/react-loading-spinner'
import {
    Scalars,
    SearchContextEditInput,
    SearchContextRepositoryRevisionsInput,
} from '@sourcegraph/shared/src/graphql-operations'
import { ISearchContext } from '@sourcegraph/shared/src/graphql/schema'
import { TelemetryProps } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { ThemeProps } from '@sourcegraph/shared/src/theme'
import { asError } from '@sourcegraph/shared/src/util/errors'
import { useObservable } from '@sourcegraph/shared/src/util/useObservable'

import { AuthenticatedUser } from '../auth'
import { withAuthenticatedUser } from '../auth/withAuthenticatedUser'
import { Page } from '../components/Page'
import { PageTitle } from '../components/PageTitle'
import { SearchContextProps } from '../search'

import { SearchContextForm } from './SearchContextForm'

export interface EditSearchContextPageProps
    extends RouteComponentProps<{ id: Scalars['ID'] }>,
        ThemeProps,
        TelemetryProps,
        Pick<SearchContextProps, 'updateSearchContext' | 'fetchSearchContext' | 'deleteSearchContext'> {
    authenticatedUser: AuthenticatedUser
}

export const AuthenticatedEditSearchContextPage: React.FunctionComponent<EditSearchContextPageProps> = props => {
    const LOADING = 'loading' as const

    const { match, updateSearchContext, fetchSearchContext } = props
    const onSubmit = useCallback(
        (
            id: Scalars['ID'] | undefined,
            searchContext: SearchContextEditInput,
            repositories: SearchContextRepositoryRevisionsInput[]
        ): Observable<ISearchContext> => {
            if (!id) {
                return throwError(new Error('Cannot update search context with undefined ID'))
            }
            return updateSearchContext({ id, searchContext, repositories })
        },
        [updateSearchContext]
    )

    const searchContextOrError = useObservable(
        useMemo(
            () =>
                fetchSearchContext(match.params.id).pipe(
                    switchMap(searchContext => {
                        if (!searchContext.viewerCanManage) {
                            return throwError(new Error('You do not have sufficient permissions to edit this context.'))
                        }
                        return of(searchContext)
                    }),
                    startWith(LOADING),
                    catchError(error => [asError(error)])
                ),
            [match.params.id, fetchSearchContext]
        )
    )

    return (
        <div className="w-100">
            <Page>
                <div className="container col-8">
                    <PageTitle title="Edit search context" />
                    {searchContextOrError === LOADING && (
                        <div className="d-flex justify-content-center">
                            <LoadingSpinner />
                        </div>
                    )}
                    {searchContextOrError && searchContextOrError !== LOADING && !isErrorLike(searchContextOrError) && (
                        <>
                            <h1 className="mb-4">Edit search context</h1>
                            <SearchContextForm {...props} searchContext={searchContextOrError} onSubmit={onSubmit} />
                        </>
                    )}
                    {isErrorLike(searchContextOrError) && (
                        <div className="alert alert-danger">
                            Error while loading the search context: <strong>{searchContextOrError.message}</strong>
                        </div>
                    )}
                </div>
            </Page>
        </div>
    )
}

export const EditSearchContextPage = withAuthenticatedUser(AuthenticatedEditSearchContextPage)
