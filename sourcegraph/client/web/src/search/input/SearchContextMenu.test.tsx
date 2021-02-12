import { mount } from 'enzyme'
import React, { ChangeEvent } from 'react'
import { DropdownItem, DropdownMenu, UncontrolledDropdown } from 'reactstrap'
import sinon from 'sinon'
import { SearchContextMenu, SearchContextMenuProps } from './SearchContextMenu'

describe('SearchContextMenu', () => {
    const defaultProps: SearchContextMenuProps = {
        availableSearchContexts: [
            {
                __typename: 'SearchContext',
                id: '1',
                spec: 'global',
                autoDefined: true,
                description: 'All repositories on Sourcegraph',
            },
            {
                __typename: 'SearchContext',
                id: '2',
                spec: '@username',
                autoDefined: true,
                description: 'Your repositories on Sourcegraph',
            },
            {
                __typename: 'SearchContext',
                id: '3',
                spec: '@username/test-version-1.5',
                autoDefined: true,
                description: 'Only code in version 1.5',
            },
        ],
        defaultSearchContextSpec: 'global',
        selectedSearchContextSpec: 'global',
        setSelectedSearchContextSpec: () => {},
        closeMenu: () => {},
    }

    it('should select item when clicking on it', () => {
        const setSelectedSearchContextSpec = sinon.spy()

        const root = mount(
            <UncontrolledDropdown>
                <DropdownMenu>
                    <SearchContextMenu {...defaultProps} setSelectedSearchContextSpec={setSelectedSearchContextSpec} />
                </DropdownMenu>
            </UncontrolledDropdown>
        )
        const item = root.find(DropdownItem).at(1)
        item.simulate('click')

        sinon.assert.calledOnce(setSelectedSearchContextSpec)
        sinon.assert.calledWithExactly(setSelectedSearchContextSpec, '@username')
    })

    it('should reset back to default when clicking on Reset button', () => {
        const setSelectedSearchContextSpec = sinon.spy()
        const closeMenu = sinon.spy()

        const root = mount(
            <UncontrolledDropdown>
                <DropdownMenu>
                    <SearchContextMenu
                        {...defaultProps}
                        setSelectedSearchContextSpec={setSelectedSearchContextSpec}
                        selectedSearchContextSpec="@username"
                        closeMenu={closeMenu}
                    />
                </DropdownMenu>
            </UncontrolledDropdown>
        )
        const button = root.find('.search-context-menu__footer-button').at(0)
        button.simulate('click')

        sinon.assert.calledOnce(setSelectedSearchContextSpec)
        sinon.assert.calledWithExactly(setSelectedSearchContextSpec, 'global')

        sinon.assert.calledOnce(closeMenu)
    })

    it('should filter list by spec when searching', () => {
        const root = mount(
            <UncontrolledDropdown>
                <DropdownMenu>
                    <SearchContextMenu {...defaultProps} />
                </DropdownMenu>
            </UncontrolledDropdown>
        )

        const searchInput = root.find('input')

        // Search by spec
        searchInput.invoke('onInput')?.({
            currentTarget: { value: '@user' },
        } as ChangeEvent<HTMLInputElement>)

        const items = root.find(DropdownItem)
        expect(items.length).toBe(2)
        expect(items.at(0).text()).toBe('@usernameYour repositories on Sourcegraph')
        expect(items.at(1).text()).toBe('@username/test-version-1.5Only code in version 1.5')
    })

    it('should show message if search does not find anything', () => {
        const root = mount(
            <UncontrolledDropdown>
                <DropdownMenu>
                    <SearchContextMenu {...defaultProps} />
                </DropdownMenu>
            </UncontrolledDropdown>
        )

        const searchInput = root.find('input')

        // Search by spec
        searchInput.invoke('onInput')?.({
            currentTarget: { value: 'nothing' },
        } as ChangeEvent<HTMLInputElement>)

        const items = root.find(DropdownItem)
        expect(items.length).toBe(1)
        expect(items.at(0).text()).toBe('No contexts found')
    })

    it('should not search by description', () => {
        const root = mount(
            <UncontrolledDropdown>
                <DropdownMenu>
                    <SearchContextMenu {...defaultProps} />
                </DropdownMenu>
            </UncontrolledDropdown>
        )

        const searchInput = root.find('input')

        searchInput.invoke('onInput')?.({
            currentTarget: { value: 'version 1.5' },
        } as ChangeEvent<HTMLInputElement>)

        const items = root.find(DropdownItem)
        expect(items.length).toBe(1)
        expect(items.at(0).text()).toBe('No contexts found')
    })
})
