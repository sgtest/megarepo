import * as H from 'history'
import * as React from 'react'
import { cleanup, fireEvent, render } from 'react-testing-library'
import _VisibilitySensor from 'react-visibility-sensor'
import { MockVisibilitySensor } from './CodeExcerpt.test'

jest.mock(
    'react-visibility-sensor',
    (): typeof _VisibilitySensor => ({ children, onChange }) => (
        <>
            <MockVisibilitySensor onChange={onChange} children={children} />
        </>
    )
)

import sinon from 'sinon'
import {
    HIGHLIGHTED_FILE_LINES_SIMPLE_REQUEST,
    NOOP_SETTINGS_CASCADE,
    RESULT,
} from '../../../web/src/search/testHelpers'
import { FileMatchChildren } from './FileMatchChildren'
import { setLinkComponent } from './Link'

const history = H.createBrowserHistory()
history.replace({ pathname: '/search' })

const onSelect = sinon.spy()

const defaultProps = {
    location: history.location,
    items: [
        {
            preview: 'third line of code',
            line: 3,
            highlightRanges: [{ start: 7, highlightLength: 4 }],
        },
    ],
    result: RESULT,
    allMatches: true,
    subsetMatches: 10,
    fetchHighlightedFileLines: HIGHLIGHTED_FILE_LINES_SIMPLE_REQUEST,
    onSelect,
    settingsCascade: NOOP_SETTINGS_CASCADE,
    isLightTheme: true,
}

describe('FileMatchChildren', () => {
    setLinkComponent((props: any) => <a {...props} />)
    afterAll(() => {
        setLinkComponent(null as any)
        cleanup()
    })

    it('calls onSelect callback when an item is clicked', () => {
        const { container } = render(<FileMatchChildren {...defaultProps} onSelect={onSelect} />)
        const item = container.querySelector('.file-match-children__item')
        expect(item).toBeTruthy()
        fireEvent.click(item!)
        expect(onSelect.calledOnce).toBe(true)
    })

    it('correctly shows number of context lines when search.contextLines setting is set', () => {
        const settingsCascade = {
            final: { 'search.contextLines': 3 },
            subjects: [
                {
                    lastID: 1,
                    settings: { 'search.contextLines': '3' },
                    extensions: null,
                    subject: {
                        __typename: 'User' as 'User',
                        username: 'f',
                        id: 'abc',
                        settingsURL: '/users/f/settings',
                        viewerCanAdminister: true,
                        displayName: 'f',
                    },
                },
            ],
        }
        const { container } = render(<FileMatchChildren {...defaultProps} settingsCascade={settingsCascade} />)
        const tableRows = container.querySelectorAll('.code-excerpt tr')
        expect(tableRows.length).toBe(7)
    })
})
