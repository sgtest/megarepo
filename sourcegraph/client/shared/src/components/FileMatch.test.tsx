import { cleanup, getAllByTestId, getByTestId, render } from '@testing-library/react'
import { createBrowserHistory } from 'history'
import FileIcon from 'mdi-react/FileIcon'
import * as React from 'react'
import _VisibilitySensor from 'react-visibility-sensor'
import sinon from 'sinon'

import { ContentMatch } from '../search/stream'
import { NOOP_TELEMETRY_SERVICE } from '../telemetry/telemetryService'
import { HIGHLIGHTED_FILE_LINES_REQUEST, NOOP_SETTINGS_CASCADE, RESULT } from '../util/searchTestHelpers'

import { MockVisibilitySensor } from './CodeExcerpt.test'
import { FileMatch } from './FileMatch'

jest.mock('react-visibility-sensor', (): typeof _VisibilitySensor => ({ children, onChange }) => (
    <>
        <MockVisibilitySensor onChange={onChange}>{children}</MockVisibilitySensor>
    </>
))

describe('FileMatch', () => {
    afterAll(cleanup)
    const history = createBrowserHistory()
    history.replace({ pathname: '/search' })
    const defaultProps = {
        location: history.location,
        result: RESULT,
        icon: FileIcon,
        onSelect: sinon.spy(),
        expanded: true,
        showAllMatches: true,
        isLightTheme: true,
        fetchHighlightedFileLineRanges: HIGHLIGHTED_FILE_LINES_REQUEST,
        settingsCascade: NOOP_SETTINGS_CASCADE,
        telemetryService: NOOP_TELEMETRY_SERVICE,
    }

    it('renders one result container', () => {
        const { container } = render(<FileMatch {...defaultProps} />)
        expect(getByTestId(container, 'result-container')).toBeVisible()
        expect(getAllByTestId(container, 'result-container').length).toBe(1)
    })

    it('correctly shows number of context lines when search.contextLines setting is set', () => {
        const result: ContentMatch = {
            type: 'content',
            name: '.travis.yml',
            repository: 'github.com/golang/oauth2',
            lineMatches: [
                {
                    line: '  - go test -v golang.org/x/oauth2/...',
                    lineNumber: 4,
                    offsetAndLengths: [[7, 4]],
                },
            ],
        }
        const settingsCascade = {
            final: { 'search.contextLines': 3 },
            subjects: [
                {
                    lastID: 1,
                    settings: { 'search.contextLines': '3' },
                    extensions: null,
                    subject: {
                        __typename: 'User' as const,
                        username: 'f',
                        id: 'abc',
                        settingsURL: '/users/f/settings',
                        viewerCanAdminister: true,
                        displayName: 'f',
                    },
                },
            ],
        }
        const { container } = render(<FileMatch {...defaultProps} result={result} settingsCascade={settingsCascade} />)
        const tableRows = container.querySelectorAll('.code-excerpt tr')
        expect(tableRows.length).toBe(7)
    })
})
