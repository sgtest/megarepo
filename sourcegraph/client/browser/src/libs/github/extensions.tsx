import * as H from 'history'
import { ContributableMenu } from '../../../../../shared/src/api/protocol'
import { CommandListPopoverButton } from '../../../../../shared/src/commandPalette/CommandList'
import { Context as ExtensionsContext } from '../../../../../shared/src/context'
import { Controller as ClientController } from '../../../../../shared/src/extensions/controller'

import * as React from 'react'
import { render } from 'react-dom'
import { GlobalDebug } from '../../shared/components/GlobalDebug'
import { ShortcutProvider } from '../../shared/components/ShortcutProvider'

export function getCommandPaletteMount(): HTMLElement {
    const headerElem = document.querySelector('div.HeaderMenu>div:last-child')
    if (!headerElem) {
        throw new Error('Unable to find command pallete mount')
    }

    const commandListClass = 'command-palette-button'

    const createCommandList = (): HTMLElement => {
        const commandListElem = document.createElement('div')
        commandListElem.className = commandListClass
        headerElem!.appendChild(commandListElem)

        return commandListElem
    }

    return document.querySelector<HTMLElement>('.' + commandListClass) || createCommandList()
}

export function getGlobalDebugMount(): HTMLElement {
    const globalDebugClass = 'global-debug'

    const createGlobalDebugMount = (): HTMLElement => {
        const globalDebugElem = document.createElement('div')
        globalDebugElem.className = globalDebugClass
        document.body.appendChild(globalDebugElem)

        return globalDebugElem
    }

    return document.querySelector<HTMLElement>('.' + globalDebugClass) || createGlobalDebugMount()
}

// TODO: remove with old inject
export function injectExtensionsGlobalComponents(
    {
        extensionsController,
        extensionsContext,
    }: {
        extensionsController: ClientController
        extensionsContext: ExtensionsContext
    },
    location: H.Location
): void {
    render(
        <ShortcutProvider>
            <CommandListPopoverButton
                extensionsController={extensionsController}
                menu={ContributableMenu.CommandPalette}
                extensionsContext={extensionsContext}
                location={location}
            />
        </ShortcutProvider>,
        getCommandPaletteMount()
    )

    render(<GlobalDebug extensionsController={extensionsController} location={location} />, getGlobalDebugMount())
}
