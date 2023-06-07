import { noop } from 'lodash'
import { NEVER } from 'rxjs'

import { NOOP_TELEMETRY_SERVICE } from '@sourcegraph/shared/src/telemetry/telemetryService'
import { MockedTestProvider } from '@sourcegraph/shared/src/testing/apollo'
import { renderWithBrandedContext } from '@sourcegraph/wildcard/src/testing'

import { SearchPatternType } from '../../graphql-operations'

import { SearchResultsInfoBar, SearchResultsInfoBarProps } from './SearchResultsInfoBar'

const COMMON_PROPS: Omit<SearchResultsInfoBarProps, 'enableCodeMonitoring'> = {
    platformContext: { settings: NEVER, sourcegraphURL: 'https://sourcegraph.com' },
    authenticatedUser: {
        id: 'userID',
        displayName: 'Chuck Cheese',
        emails: [{ email: 'chuck@chuckeecheese.com', isPrimary: true, verified: true }],
        permissions: { nodes: [] },
    },
    allExpanded: true,
    onExpandAllResultsToggle: noop,
    onSaveQueryClick: noop,
    onExportCsvClick: noop,
    stats: <div />,
    telemetryService: NOOP_TELEMETRY_SERVICE,
    patternType: SearchPatternType.standard,
    caseSensitive: false,
    setSidebarCollapsed: noop,
    sidebarCollapsed: false,
    isSourcegraphDotCom: true,
    options: {
        version: 'V3',
        patternType: SearchPatternType.standard,
        caseSensitive: false,
        trace: undefined,
    },
}

const renderSearchResultsInfoBar = (
    props: Pick<SearchResultsInfoBarProps, 'enableCodeMonitoring'> & Partial<SearchResultsInfoBarProps>
) =>
    renderWithBrandedContext(
        <MockedTestProvider>
            <SearchResultsInfoBar {...COMMON_PROPS} {...props} />
        </MockedTestProvider>
    )

describe('SearchResultsInfoBar', () => {
    test('code monitoring feature flag disabled', () => {
        expect(
            renderSearchResultsInfoBar({
                query: 'foo type:diff',
                enableCodeMonitoring: false,
            }).asFragment()
        ).toMatchSnapshot()
    })

    test('code monitoring feature flag enabled, cannot create monitor from query', () => {
        expect(renderSearchResultsInfoBar({ query: 'foo', enableCodeMonitoring: true }).asFragment()).toMatchSnapshot()
    })

    test('code monitoring feature flag enabled, can create monitor from query', () => {
        expect(
            renderSearchResultsInfoBar({ query: 'foo type:diff', enableCodeMonitoring: true }).asFragment()
        ).toMatchSnapshot()
    })

    test('code monitoring feature flag enabled, can create monitor from query, user not logged in', () => {
        expect(
            renderSearchResultsInfoBar({
                query: 'foo type:diff',
                enableCodeMonitoring: true,
                authenticatedUser: null,
            }).asFragment()
        ).toMatchSnapshot()
    })

    test('unauthenticated user', () => {
        expect(
            renderSearchResultsInfoBar({
                query: 'foo type:diff',
                enableCodeMonitoring: true,
                authenticatedUser: null,
            }).asFragment()
        ).toMatchSnapshot()
    })
})
