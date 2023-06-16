import React, { useContext, useState, useCallback, useMemo } from 'react'

import { useTemporarySetting } from '@sourcegraph/shared/src/settings/temporary'

import { useCodyChat, CodyChatStore, codyChatStoreMock } from '../useCodyChat'

import { useSidebarSize } from './useSidebarSize'

interface CodySidebarStore extends CodyChatStore {
    readonly isSidebarOpen: boolean
    readonly inputNeedsFocus: boolean
    readonly sidebarSize: number
    setIsSidebarOpen: (isOpen: boolean) => void
    setFocusProvided: () => void
    setSidebarSize: (size: number) => void
}

const CodySidebarContext = React.createContext<CodySidebarStore | null>({
    ...codyChatStoreMock,
    isSidebarOpen: false,
    inputNeedsFocus: false,
    sidebarSize: 0,
    setSidebarSize: () => {},
    setIsSidebarOpen: () => {},
    setFocusProvided: () => {},
})

interface ICodySidebarStoreProviderProps {
    children?: React.ReactNode
}

export const CodySidebarStoreProvider: React.FC<ICodySidebarStoreProviderProps> = ({ children }) => {
    const [isSidebarOpen, setIsSidebarOpenState] = useTemporarySetting('cody.showSidebar', false)
    const [inputNeedsFocus, setInputNeedsFocus] = useState(false)
    const { sidebarSize, setSidebarSize } = useSidebarSize()

    const setFocusProvided = useCallback(() => {
        setInputNeedsFocus(false)
    }, [setInputNeedsFocus])

    const setIsSidebarOpen = useCallback(
        (open: boolean) => {
            setIsSidebarOpenState(open)
            setInputNeedsFocus(true)
        },
        [setIsSidebarOpenState, setInputNeedsFocus]
    )

    const onEvent = useCallback(() => setIsSidebarOpen(true), [setIsSidebarOpen])

    const codyChatStore = useCodyChat({ onEvent })

    const state = useMemo<CodySidebarStore>(
        () => ({
            ...codyChatStore,
            isSidebarOpen: isSidebarOpen ?? false,
            inputNeedsFocus,
            sidebarSize: isSidebarOpen ? sidebarSize : 0,
            setIsSidebarOpen,
            setFocusProvided,
            setSidebarSize,
        }),
        [codyChatStore, isSidebarOpen, sidebarSize, setIsSidebarOpen, setFocusProvided, setSidebarSize, inputNeedsFocus]
    )

    // dirty fix because CodyRecipesWidget is rendered inside a different React DOM tree.
    const global = window as any
    global.codySidebarStore = state

    return <CodySidebarContext.Provider value={state}>{children}</CodySidebarContext.Provider>
}

export const useCodySidebar = (): CodySidebarStore => useContext(CodySidebarContext) as CodySidebarStore
