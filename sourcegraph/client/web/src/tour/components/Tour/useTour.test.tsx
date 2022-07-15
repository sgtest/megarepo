import { renderHook, cleanup, act } from '@testing-library/react'
import { WrapperComponent } from '@testing-library/react-hooks'

import { TemporarySettings } from '@sourcegraph/shared/src/settings/temporary/TemporarySettings'
import { MockTemporarySettings } from '@sourcegraph/shared/src/settings/temporary/testUtils'

import { TourLanguage } from './types'
import { useTour, TourState } from './useTour'

/**
 * Extracts non-function properties from an object.
 */
const getFieldsAsObject = (value: object): object =>
    Object.entries(Object.getOwnPropertyDescriptors(value))
        // eslint-disable-next-line no-prototype-builtins
        .filter(([, desc]) => desc.hasOwnProperty('value') && typeof desc.value !== 'function')
        // eslint-disable-next-line @typescript-eslint/no-unsafe-assignment
        .reduce((result, [key]) => ({ ...result, [key]: (value as any)[key] }), {})

const TourId = 'MockTour'

const setup = (settings: TemporarySettings['onboarding.quickStartTour'] = {}) => {
    const wrapper: WrapperComponent<React.PropsWithChildren<{}>> = ({ children }) => (
        <MockTemporarySettings settings={{ 'onboarding.quickStartTour': settings }}>{children}</MockTemporarySettings>
    )
    return renderHook(() => useTour(TourId), { wrapper })
}

describe('useTour.ts', () => {
    afterAll(cleanup)

    beforeEach(() => {
        localStorage.clear()
    })

    test('returns initial state from temporary settings', () => {
        const initialState: TourState = { completedStepIds: [], status: 'closed', language: TourLanguage.Typescript }
        const { result } = setup({ [TourId]: initialState })
        expect(getFieldsAsObject(result.current)).toMatchObject(initialState)
    })

    test('clears state when restart called', () => {
        const initialState: TourState = { completedStepIds: [], status: 'closed', language: TourLanguage.Typescript }
        const { result } = setup({ [TourId]: initialState })
        expect(getFieldsAsObject(result.current)).toMatchObject(initialState)
        act(() => result.current.restart())
        expect(getFieldsAsObject(result.current)).toMatchObject({})
    })

    test('updates "status" when "setStatus" called', () => {
        const { result } = setup()
        act(() => result.current.setStatus('completed'))
        expect(result.current.status).toEqual('completed')
    })

    test('updates "language" when "setLanguage" called', () => {
        const { result } = setup()
        act(() => result.current.setLanguage(TourLanguage.Go))
        expect(result.current.language).toEqual(TourLanguage.Go)
    })

    test('updates "completedStepIds" as unique array when "setStepCompleted" called', () => {
        const { result } = setup()
        act(() => result.current.setStepCompleted('step1'))
        expect(result.current.completedStepIds).toEqual(['step1'])
        act(() => result.current.setStepCompleted('step2'))
        expect(result.current.completedStepIds).toEqual(['step1', 'step2'])
        act(() => result.current.setStepCompleted('step2'))
        expect(result.current.completedStepIds).toEqual(['step1', 'step2'])
    })
})
