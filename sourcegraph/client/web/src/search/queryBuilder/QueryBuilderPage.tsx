import React, { useState } from 'react'
import { Link } from 'react-router-dom'
import { PatternTypeProps, SearchContextProps } from '..'
import { buildSearchURLQuery } from '../../../../shared/src/util/url'
import { QueryBuilder } from './QueryBuilder'
import { PageTitle } from '../../components/PageTitle'
import { VersionContextProps } from '../../../../shared/src/search/util'
import { PageHeader } from '../../components/PageHeader'
import { Page } from '../../components/Page'

interface Props
    extends Pick<PatternTypeProps, 'patternType'>,
        VersionContextProps,
        Pick<SearchContextProps, 'selectedSearchContextSpec'> {}

/**
 * A page with a search query builder form to make it easy to construct search queries.
 */
export const QueryBuilderPage: React.FunctionComponent<Props> = ({
    versionContext,
    patternType,
    selectedSearchContextSpec,
}) => {
    const [query, setQuery] = useState('')

    const helpText = 'Use the fields below to build your query'

    return (
        <Page>
            <PageTitle title="Query builder" />
            <PageHeader path={[{ text: 'Query builder' }]} className="mb-3" />
            <div className="form form-inline mt-3">
                <input
                    type="text"
                    value={query}
                    readOnly={true}
                    placeholder={helpText}
                    className="form-control flex-fill mr-2"
                    data-tooltip={query === '' ? '' : `Read-only field. ${helpText}.`}
                />
                <Link
                    className={`btn btn-primary ${query === '' ? 'disabled' : ''}`}
                    to={`/search?${buildSearchURLQuery(
                        query,
                        patternType,
                        false,
                        versionContext,
                        selectedSearchContextSpec
                    )}`}
                >
                    Search
                </Link>
            </div>
            <QueryBuilder
                onFieldsQueryChange={setQuery}
                isSourcegraphDotCom={window.context?.sourcegraphDotComMode}
                patternType={patternType}
            />
        </Page>
    )
}
