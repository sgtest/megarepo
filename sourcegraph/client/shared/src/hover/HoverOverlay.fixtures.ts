import { action } from '@storybook/addon-actions'
import { boolean } from '@storybook/addon-knobs'
import { createMemoryHistory } from 'history'
import { of } from 'rxjs'
import { MarkupContent, Badged, AggregableBadge } from 'sourcegraph'

import { MarkupKind } from '@sourcegraph/extension-api-classes'

import { ActionItemAction } from '../actions/ActionItem'
import { PlatformContext } from '../platform/context'
import { NOOP_TELEMETRY_SERVICE } from '../telemetry/telemetryService'

import { HoverOverlayProps } from './HoverOverlay'

const history = createMemoryHistory()
const NOOP_EXTENSIONS_CONTROLLER = { executeCommand: () => Promise.resolve() }
const NOOP_PLATFORM_CONTEXT: Pick<PlatformContext, 'forceUpdateTooltip' | 'settings'> = {
    forceUpdateTooltip: () => undefined,
    settings: of({ final: {}, subjects: [] }),
}

export const commonProps = (): HoverOverlayProps => ({
    showCloseButton: boolean('showCloseButton', true),
    location: history.location,
    telemetryService: NOOP_TELEMETRY_SERVICE,
    extensionsController: NOOP_EXTENSIONS_CONTROLLER,
    platformContext: NOOP_PLATFORM_CONTEXT,
    isLightTheme: true,
    overlayPosition: { top: 16, left: 16 },
    onAlertDismissed: action('onAlertDismissed'),
    onCloseButtonClick: action('onCloseButtonClick'),
})

export const FIXTURE_CONTENT: Badged<MarkupContent> = {
    value:
        '```go\nfunc RegisterMiddlewares(m ...*Middleware)\n```\n\n' +
        '---\n\nRegisterMiddlewares registers additional authentication middlewares. Currently this is used to register enterprise-only SSO middleware. This should only be called from an init function.\n',
    kind: MarkupKind.Markdown,
}

export const FIXTURE_SEMANTIC_BADGE: AggregableBadge = {
    text: 'semantic',
    linkURL: 'https://docs.sourcegraph.com/code_intelligence/explanations/precise_code_intelligence',
    hoverMessage: 'Sample hover message',
}

export const FIXTURE_ACTIONS: ActionItemAction[] = [
    {
        action: {
            id: 'goToDefinition.preloaded',
            title: 'Go to definition',
            command: 'open',
            commandArguments: ['/github.com/sourcegraph/codeintellify/-/blob/src/hoverifier.ts?subtree=true#L57:1'],
        },
        active: true,
    },
    {
        action: {
            id: 'findReferences',
            title: 'Find references',
            command: 'open',
            commandArguments: [
                '/github.com/sourcegraph/codeintellify/-/blob/src/hoverifier.ts?subtree=true#L57:18&tab=references',
            ],
        },
        active: true,
    },
]
