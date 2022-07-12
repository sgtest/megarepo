import 'focus-visible'
import { ReactElement } from 'react'

import { configureActions } from '@storybook/addon-actions'
import { withConsole } from '@storybook/addon-console'
import { DecoratorFn, Parameters } from '@storybook/react'
import { withDesign } from 'storybook-addon-designs'

import { setLinkComponent, AnchorLink } from '@sourcegraph/wildcard'

import { withChromaticThemes } from './decorators/withChromaticThemes'
import { themeDark, themeLight, THEME_DARK_CLASS, THEME_LIGHT_CLASS } from './themes'
import { isChromatic } from './utils/isChromatic'

const withConsoleDecorator: DecoratorFn = (storyFunc, context): ReactElement => withConsole()(storyFunc)(context)

export const decorators = [withDesign, withConsoleDecorator, isChromatic() && withChromaticThemes].filter(Boolean)

export const parameters: Parameters = {
    layout: 'fullscreen',
    options: {
        storySort: {
            order: ['wildcard', 'shared', 'branded', '*'],
            method: 'alphabetical',
        },
    },
    darkMode: {
        stylePreview: true,
        lightClass: THEME_LIGHT_CLASS,
        darkClass: THEME_DARK_CLASS,
        light: themeLight,
        dark: themeDark,
    },
    previewTabs: {
        'storybook/docs/panel': {
            hidden: true,
        },
    },
    // disables snapshotting for all stories by default
    chromatic: { disableSnapshot: true },
}

configureActions({ depth: 100, limit: 20 })

setLinkComponent(AnchorLink)

// Default to light theme for Chromatic and "Open canvas in new tab" button.
// addon-dark-mode will override this if it's running.
if (!document.body.classList.contains('theme-dark')) {
    document.body.classList.add('theme-light')
}

if (isChromatic()) {
    const style = document.createElement('style')
    style.innerHTML = `
      .monaco-editor .cursor {
        visibility: hidden !important;
      }
    `
    document.head.append(style)
}

declare global {
    interface Window {
        STORYBOOK_ENV?: string
    }
}

/**
 * Since we do not use `storiesOf` API, this env variable is not set by `@storybook/react` anymore.
 * The `withConsole` decorator relies on this env variable so we set it manually here.
 */
if (!window.STORYBOOK_ENV) {
    window.STORYBOOK_ENV = 'react'
}
