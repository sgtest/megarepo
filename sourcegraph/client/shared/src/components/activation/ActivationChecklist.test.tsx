import React from 'react'

import { renderWithBrandedContext } from '../../testing'

import { ActivationChecklist } from './ActivationChecklist'

jest.mock('mdi-react/CheckboxBlankCircleIcon', () => 'CheckboxBlankCircleIcon')
jest.mock('mdi-react/CheckIcon', () => 'CheckIcon')

describe('ActivationChecklist', () => {
    test('render loading', () => {
        const component = renderWithBrandedContext(<ActivationChecklist steps={[]} />)
        expect(component.asFragment()).toMatchSnapshot()
    })
    test('render 0/1 complete', () => {
        {
            const component = renderWithBrandedContext(
                <ActivationChecklist
                    steps={[
                        {
                            id: 'ConnectedCodeHost',
                            title: 'title1',
                            detail: 'detail1',
                        },
                    ]}
                    completed={{}}
                />
            )
            expect(component.asFragment()).toMatchSnapshot()
        }
        {
            const component = renderWithBrandedContext(
                <ActivationChecklist
                    steps={[
                        {
                            id: 'ConnectedCodeHost',
                            title: 'title1',
                            detail: 'detail1',
                        },
                    ]}
                    completed={{ EnabledRepository: true }} // another item
                />
            )
            expect(component.asFragment()).toMatchSnapshot()
        }
    })
    test('render 1/1 complete', () => {
        const component = renderWithBrandedContext(
            <ActivationChecklist
                steps={[
                    {
                        id: 'ConnectedCodeHost',
                        title: 'title1',
                        detail: 'detail1',
                    },
                ]}
                completed={{ ConnectedCodeHost: true }} // same item as in steps
            />
        )
        expect(component.asFragment()).toMatchSnapshot()
    })
})
