import React, { useCallback, useState } from 'react'

import { mdiMagnify, mdiPlus } from '@mdi/js'
import classNames from 'classnames'
import * as H from 'history'

import { SearchContextProps } from '@sourcegraph/search'
import { PlatformContextProps } from '@sourcegraph/shared/src/platform/context'
import { PageHeader, Link, Button, Icon } from '@sourcegraph/wildcard'

import { AuthenticatedUser } from '../../auth'
import { Page } from '../../components/Page'

import { SearchContextsListTab } from './SearchContextsListTab'

export interface SearchContextsListPageProps
    extends Pick<
            SearchContextProps,
            'fetchSearchContexts' | 'fetchAutoDefinedSearchContexts' | 'getUserSearchContextNamespaces'
        >,
        PlatformContextProps<'requestGraphQL'> {
    location: H.Location
    history: H.History
    isSourcegraphDotCom: boolean
    authenticatedUser: AuthenticatedUser | null
}

type SelectedTab = 'list'

function getSelectedTabFromLocation(locationSearch: string): SelectedTab {
    const urlParameters = new URLSearchParams(locationSearch)
    switch (urlParameters.get('tab')) {
        case 'list':
            return 'list'
    }
    return 'list'
}

function setSelectedLocationTab(location: H.Location, history: H.History, selectedTab: SelectedTab): void {
    const urlParameters = new URLSearchParams(location.search)
    urlParameters.set('tab', selectedTab)
    if (location.search !== urlParameters.toString()) {
        history.replace({ ...location, search: urlParameters.toString() })
    }
}

export const SearchContextsListPage: React.FunctionComponent<
    React.PropsWithChildren<SearchContextsListPageProps>
> = props => {
    const [selectedTab, setSelectedTab] = useState<SelectedTab>(getSelectedTabFromLocation(props.location.search))

    const setTab = useCallback(
        (tab: SelectedTab) => {
            setSelectedTab(tab)
            setSelectedLocationTab(props.location, props.history, tab)
        },
        [props.location, props.history]
    )

    const onSelectSearchContextsList = useCallback<React.MouseEventHandler>(
        event => {
            event.preventDefault()
            setTab('list')
        },
        [setTab]
    )

    return (
        <div data-testid="search-contexts-list-page" className="w-100">
            <Page>
                <PageHeader
                    actions={
                        <Button to="/contexts/new" variant="primary" as={Link}>
                            <Icon aria-hidden={true} svgPath={mdiPlus} />
                            Create search context
                        </Button>
                    }
                    description={
                        <span className="text-muted">
                            Search code you care about with search contexts.{' '}
                            <Link
                                to="/help/code_search/explanations/features#search-contexts"
                                target="_blank"
                                rel="noopener noreferrer"
                            >
                                Learn more
                            </Link>
                        </span>
                    }
                    className="mb-3"
                >
                    <PageHeader.Heading as="h2" styleAs="h1">
                        <PageHeader.Breadcrumb icon={mdiMagnify} to="/search" aria-label="Code Search" />
                        <PageHeader.Breadcrumb>Contexts</PageHeader.Breadcrumb>
                    </PageHeader.Heading>
                </PageHeader>
                <div className="mb-4">
                    <div id="search-context-tabs-list" className="nav nav-tabs">
                        <div className="nav-item">
                            {/* eslint-disable-next-line jsx-a11y/anchor-is-valid */}
                            <Link
                                to=""
                                role="tab"
                                aria-selected={selectedTab === 'list'}
                                aria-controls="search-context-tabs-list"
                                onClick={onSelectSearchContextsList}
                                className={classNames('nav-link', selectedTab === 'list' && 'active')}
                            >
                                <span className="text-content" data-tab-content="Your search contexts">
                                    Your search contexts
                                </span>
                            </Link>
                        </div>
                    </div>
                </div>
                {selectedTab === 'list' && <SearchContextsListTab {...props} />}
            </Page>
        </div>
    )
}
