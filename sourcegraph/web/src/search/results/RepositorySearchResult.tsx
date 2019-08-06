import * as React from 'react'
import { RepositoryIcon } from '../../../../shared/src/components/icons' // TODO: Switch to mdi icon
import { RepoLink } from '../../../../shared/src/components/RepoLink'
import { ResultContainer } from '../../../../shared/src/components/ResultContainer'
import * as GQL from '../../../../shared/src/graphql/schema'
import { eventLogger } from '../../tracking/eventLogger'

interface Props {
    /**
     * The repository that was matched.
     */
    result: GQL.IRepository

    /**
     * Called when the search result is selected.
     */
    onSelect: () => void
}

export const RepositorySearchResult: React.FunctionComponent<Props> = (props: Props) => (
    <ResultContainer
        titleClassName="repository-search-result__title"
        icon={RepositoryIcon}
        title={
            <>
                {/* eslint-disable react/jsx-no-bind */}
                <RepoLink
                    repoName={props.result.name}
                    to={props.result.url}
                    onClick={() => {
                        eventLogger.log('RepositorySearchResultClicked')
                        props.onSelect()
                    }}
                />
                {/* eslint-enable react/jsx-no-bind */}
                <span className="repository-search-result__spacer" />
                <small>Repository name match</small>
            </>
        }
    />
)
